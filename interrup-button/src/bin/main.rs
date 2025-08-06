#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{DriveStrength, Event, Input, InputConfig, Io, Level, Output, OutputConfig, Pull};
use esp_hal::{handler, main};
use esp_hal::time::{Duration, Instant};
use esp_println::println;
use {esp_backtrace as _, esp_println as _};
use critical_section::Mutex;
use core::cell::{RefCell, Cell};



// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

const DEBOUNCE_DELAY: Duration = Duration::from_millis(500);

// Shared resources protected by critical sections:
// - GLOBAL_PIN: Stores button instance for interrupt clearing
// - GLOBAL_FLAG: Communication flag between ISR and main loop
// - LAST_TRIGGER: Timestamp for debounce timing
static GLOBAL_PIN: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static GLOBAL_FLAG: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static LAST_TRIGGER: Mutex<Cell<Option<Instant>>> = Mutex::new(Cell::new(None));

// OTHER THREAD
// Critical sections ensure safe access to shared resources between
// the main code and interrupt handlers by temporarily disabling interrupts
#[handler]
fn gpio() {
    // Interrupt Service Routine (ISR) - executes when button is pressed:
    // 1. Enter critical section to safely access shared resources
    // 2. Clear the interrupt flag to prevent retriggering
    // 3. Set the global flag to notify main loop
    critical_section::with(|cs| {
        GLOBAL_PIN
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap()
            .clear_interrupt();
        GLOBAL_FLAG.borrow(cs).set(true);        
    })
    // Critical section automatically exits here
}

#[main]
fn main() -> ! {
    // generator version: 0.5.0
    
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::_80MHz);
    let peripherals = esp_hal::init(config);

    let mut io = Io::new(peripherals.IO_MUX);
    io.set_interrupt_handler(gpio);
    

    let led_config = OutputConfig::default().with_drive_strength(DriveStrength::_5mA);
    let mut led = Output::new(peripherals.GPIO4, Level::Low,  led_config);

    let input_conf = InputConfig::default().with_pull(Pull::Up);
    let mut button = Input::new(peripherals.GPIO0, input_conf);

    button.listen(Event::FallingEdge);
    
    critical_section::with(|cs|
        GLOBAL_PIN.borrow_ref_mut(cs).replace(button)
    );

    let mut count = 0_u32;
    
    loop {
        // Main loop critical section:
        // Safely access shared resources to check button press validity
        critical_section::with(
            |cs| {
            // Check if ISR has detected a button press
            if GLOBAL_FLAG.borrow(cs).get() {
                // Get current time and last valid trigger timestamp
                let now = Instant::now();
                let last_trigger = LAST_TRIGGER.borrow(cs).get();
                
                // Check if enough time has passed since last valid press
                if last_trigger.is_none() || (now - last_trigger.unwrap()) >= DEBOUNCE_DELAY {
                    // Toggle LED state (safe because we're in critical section)
                    if led.is_set_high() {
                        led.set_low();
                    } else {
                        led.set_high();
                    }
                    GLOBAL_FLAG.borrow(cs).set(false);
                    count += 1;
                    println!("Button Press Count = {count}");
                    LAST_TRIGGER.borrow(cs).set(Some(now));
                } else {
                    GLOBAL_FLAG.borrow(cs).set(false);
                }
            }
            }
         )
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
