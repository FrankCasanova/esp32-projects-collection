#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use esp_hal::clock::CpuClock;
use esp_hal::gpio::{DriveMode, DriveStrength, Level, Output, OutputConfig, Pull};
use esp_hal::main;
use esp_hal::time::{Duration, Instant};
use {esp_backtrace as _, esp_println as _};

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // generator version: 0.5.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Output pin configuration
    let led_pin_conf = OutputConfig::default()
        .with_drive_mode(DriveMode::PushPull)
        .with_drive_strength(DriveStrength::_5mA)
        .with_pull(Pull::None);

    let mut led_red = Output::new(peripherals.GPIO1, Level::Low, led_pin_conf);

    let mut led_yellow = Output::new(peripherals.GPIO2, Level::Low, led_pin_conf);

    let mut led_green = Output::new(peripherals.GPIO3, Level::Low, led_pin_conf);

    loop {
        led_red.set_high();
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(10000) {}
        led_red.set_low();
        led_green.set_high();
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(7000) {}
        led_green.set_low();
        led_yellow.set_high();
        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(3000) {}
        led_yellow.set_low();
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
