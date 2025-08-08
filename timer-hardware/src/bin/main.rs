// Embedded Rust program for ESP32-C3 microcontroller
// This program blinks an onboard LED every second

// Special attributes for embedded systems:
#![no_std]       // No standard library - direct hardware access
#![no_main]       // Custom entry point instead of main()
// Safety enforcement: Prevent unsafe memory operations
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

// Import necessary hardware abstraction layer (HAL) components
use esp_hal::clock::CpuClock;          // CPU clock configuration
use esp_hal::gpio::{Level, Output, OutputConfig}; // GPIO pin control
use esp_hal::main;                      // Custom main attribute
use esp_hal::timer::timg::TimerGroup;   // Timer group peripheral
use esp_hal::timer::Timer;              // Timer functionality
use {esp_backtrace as _, esp_println as _}; // Debugging utilities

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Program entry point with custom attribute
#[main]
fn main() -> ! {
    // Generator version: 0.5.0
    // This function never returns (! = diverging function)

    // Initialize hardware configuration with 80Mhz CPU clock speed
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::_80MHz);
    let peripherals = esp_hal::init(config);

    // Configure onboard LED (GPIO8) as output with initial LOW state
    let led_config = OutputConfig::default();
    let mut onboard_led = Output::new(peripherals.GPIO8, Level::Low, led_config);

    // Initialize timer group and get first timer
    let timer_group_0 = TimerGroup::new(peripherals.TIMG0);
    let timer = timer_group_0.timer0;
    let mut start = timer.now();  // Capture initial timer value
    timer.start();                // Start the timer
    


    // Main program loop runs continuously
    loop {
        // Check if 3 second has elapsed since last toggle
        if start.elapsed().as_millis() >= 200 {
            onboard_led.toggle();  // Change LED state
            start = timer.now();    // Reset timer reference
        }
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
