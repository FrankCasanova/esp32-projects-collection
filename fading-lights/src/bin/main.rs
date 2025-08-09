#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use esp_hal::clock::CpuClock;  // Import CPU clock configuration options
use esp_hal::ledc::channel::ChannelIFace;  // LEDC channel interface
use esp_hal::ledc::timer::TimerIFace;  // LEDC timer interface
use esp_hal::ledc::{channel, timer, LSGlobalClkSource, Ledc, LowSpeed};  // Various LEDC components
use esp_hal::main;  // Import the main attribute macro for entry point
use esp_hal::time::Rate;  // Import rate configuration for timers
use esp_println::println;  // A print macro that works without std

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[main]  // Mark this as the entry point for the application
fn main() -> ! {
    // generator version: 0.5.0

    // Configure the CPU clock speed
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::_80MHz);
    // Initialize the ESP32 peripherals with the given configuration
    let peripherals = esp_hal::init(config);

    // Assign GPIO pins 23 and 22 to control the LEDs
    let led1 = peripherals.GPIO1;
    let led2 = peripherals.GPIO2;
    let led3 = peripherals.GPIO3;


    // Initialize the LEDC peripheral
    let mut ledc = Ledc::new(peripherals.LEDC);
    // Set the global clock source for LEDC to APB clock
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
    
    // Configure LEDC timer 0 for low-speed channel
    let mut lstimer0 = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    // Set timer configuration: 5-bit duty cycle, APB clock source, 12kHz frequency
    lstimer0
        .configure(timer::config::Config {
            duty: timer::config::Duty::Duty5Bit,
            clock_source: timer::LSClockSource::APBClk,
            frequency: Rate::from_khz(12),
        })
        .unwrap();  // Assume configuration succeeds; in production, handle errors here

    // Configure channel 0 for LED1
    let mut channel0 = ledc.channel(channel::Number::Channel0, led1);
    channel0
        .configure(channel::config::Config {
            timer: &lstimer0,  // Use the previously configured timer
            duty_pct: 10,      // Initial duty cycle (10%)
            pin_config: channel::config::PinConfig::PushPull,  // Output mode
        })
        .unwrap();

    // Configure channel 1 for LED2 similarly
    let mut channel1 = ledc.channel(channel::Number::Channel1, led2);
    channel1
        .configure(channel::config::Config {
            timer: &lstimer0,
            duty_pct: 10,
            pin_config: channel::config::PinConfig::PushPull,
        })
        .unwrap();

    // Configure channel 2 for LED3 similarly
    let mut channel2 = ledc.channel(channel::Number::Channel1, led3);
    channel2
        .configure(channel::config::Config {
            timer: &lstimer0,
            duty_pct: 10,
            pin_config: channel::config::PinConfig::PushPull,
        })
        .unwrap();

    println!("STARTING THE PROGRAM");
    
    loop {  // Main loop to continuously fade the LEDs
        // Fade LED1 from 0% to 100% over 2 seconds
        channel0.start_duty_fade(0, 100, 2000).unwrap();
        while channel0.is_duty_fade_running() {}  // Wait until fade completes
        
        // Fade LED2 from 0% to 100% over 2 seconds
        channel1.start_duty_fade(0, 100, 2000).unwrap();
        while channel1.is_duty_fade_running() {}

        // Fade LED3 from 0% to 100% over 2 seconds
        channel2.start_duty_fade(0, 100, 2000).unwrap();
        while channel2.is_duty_fade_running() {}

        // Fade LED1 from 100% to 0% over 0.5 seconds
        channel0.start_duty_fade(100, 0, 500).unwrap();
        while channel0.is_duty_fade_running() {}
        
        // Fade LED2 from 100% to 0% over 0.5 seconds
        channel1.start_duty_fade(100, 0, 500).unwrap();
        while channel1.is_duty_fade_running() {}

        // Fade LED3 from 100% to 0% over 0.5 seconds
        channel2.start_duty_fade(100, 0, 500).unwrap();
        while channel2.is_duty_fade_running() {}
    }
}
