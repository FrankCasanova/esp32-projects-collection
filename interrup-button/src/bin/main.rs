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

static GLOBAL_PIN: Mutex<RefCell<Option<Input>>> = Mutex::new(RefCell::new(None));
static GLOBAL_FLAG: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));

// OTHER THREAD
#[handler]
fn gpio() {
    critical_section::with(|cs| {
    GLOBAL_PIN
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap()
            .clear_interrupt();
    GLOBAL_FLAG.borrow(cs).set(true);        
    })
}

#[main]
fn main() -> ! {
    // generator version: 0.5.0
    
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
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
        critical_section::with(
            |cs| {
                if GLOBAL_FLAG.borrow(cs).get() {
                    GLOBAL_FLAG.borrow(cs).set(false);
                    count += 1;
                    println!("Button Press Count = {count}");
                    if led.is_set_high() {
                        led.set_low();
                    } else {
                        led.set_high();
                    }
                }
            }
         )
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}


