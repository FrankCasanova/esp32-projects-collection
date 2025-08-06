#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]


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

// Shared resources protected by critical sections (Mutex pattern):
// - GLOBAL_PIN: RefCell<Option<Input>> - Wrapped GPIO input pin (needs RefCell 
//   because Input isn't Copy and we need mutable access to the Option)
// - GLOBAL_PIN2: RefCell<Option<Output>> - Wrapped GPIO output pin (same rationale)
// - GLOBAL_FLAG: Cell<bool> - Simple flag (Cell allows atomic access for primitive types)
// - LAST_TRIGGER: Cell<Option<Instant>> - Timestamp (Cell handles Option's interior mutability)
// - COUNT: Cell<u32> - Press counter (Cell provides thread-safe counter for primitive type)
//
// Safety architecture:
// 1. Mutex guarantees exclusive access across threads
// 2. RefCell enables safe interior mutability for non-Copy types
// 3. Cell provides atomic access for simple primitives
// 4. Critical sections enforce atomic operation blocks
static GLOBAL_PIN: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static GLOBAL_PIN2: Mutex<RefCell<Option<Output>>> = Mutex::new(RefCell::new(None));
static GLOBAL_FLAG: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static LAST_TRIGGER: Mutex<Cell<Option<Instant>>> = Mutex::new(Cell::new(None));
static COUNT: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));

// OTHER THREAD
// Critical sections ensure safe access to shared resources between
// the main code and interrupt handlers by temporarily disabling interrupts
#[handler]
fn gpio() {
    // Interrupt Service Routine (ISR) - executes when button is pressed:
    // Concurrency Safety Process:
    // 1. Enter critical section (atomically locks access to shared resources)
    // 2. Access GPIO pins through layered protection:
    //    a. Mutex lock: Ensures exclusive access across threads
    //    b. RefCell borrow: Runtime-checked mutable access to Option-wrapped GPIO
    //    c. unwrap(): Safe because we initialize before interrupts are enabled
    // 3. Clear interrupt flag first to prevent retriggering during handling
    // 4. Update LED state using same protection pattern
    // 5. Update counter using Cell's atomic access for primitive type
    // 
    // Analog: Like a security checkpoint where you must:
    // 1. Get clearance (critical section)
    // 2. Check out a shared tool (Mutex+RefCell)
    // 3. Use the tool (GPIO access)
    // 4. Check it back in automatically (drop guards)
    critical_section::with(|cs| {
        GLOBAL_PIN
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap()
            .clear_interrupt();
        GLOBAL_PIN2
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap();
        GLOBAL_FLAG.borrow(cs).set(true);
        if GLOBAL_FLAG.borrow(cs).get() {
                // Get current time and last valid trigger timestamp
                let now = Instant::now();
                let last_trigger = LAST_TRIGGER.borrow(cs).get();
                
                // Check if enough time has passed since last valid press
                if last_trigger.is_none() || (now - last_trigger.unwrap()) >= DEBOUNCE_DELAY {
                    // Toggle LED state (safe because we're in critical section)
                    if GLOBAL_PIN2.borrow_ref_mut(cs).as_mut().unwrap().is_set_high() {
                        GLOBAL_PIN2.borrow_ref_mut(cs).as_mut().unwrap().set_low();
                    } else {
                        GLOBAL_PIN2.borrow_ref_mut(cs).as_mut().unwrap().set_high();
                    }
                    GLOBAL_FLAG.borrow(cs).set(false);
                    let count = COUNT.borrow(cs).get() + 1;
                    COUNT.borrow(cs).set(count);
                    println!("Button Press Count = {count}");
                    LAST_TRIGGER.borrow(cs).set(Some(now));
                } else {
                    GLOBAL_FLAG.borrow(cs).set(false);
                }
            }        
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
    let led = Output::new(peripherals.GPIO4, Level::Low,  led_config);

    let input_conf = InputConfig::default().with_pull(Pull::Up);
    let mut button = Input::new(peripherals.GPIO0, input_conf);

    button.listen(Event::FallingEdge);
    
    critical_section::with(|cs| {
        GLOBAL_PIN.borrow_ref_mut(cs).replace(button);
        GLOBAL_PIN2.borrow_ref_mut(cs).replace(led);
    });

    // let mut count = 0_u32;
    
    loop {}

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
