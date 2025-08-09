#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]


use esp_hal::analog::adc::{Adc, AdcCalBasic, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::ledc::{LSGlobalClkSource, Ledc, LowSpeed, timer, channel};
use esp_hal::ledc::timer::TimerIFace;
use esp_hal::ledc::channel::ChannelIFace;
use esp_hal::main;
use esp_hal::peripherals::ADC1;
use esp_hal::time::Rate;
use esp_println as _;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // generator version: 0.5.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::_80MHz);
    let peripherals = esp_hal::init(config);

    let led = peripherals.GPIO7;
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
    let mut channel0 = ledc.channel(channel::Number::Channel0, led);
    channel0
        .configure(channel::config::Config {
            timer: &lstimer0,  // Use the previously configured timer
            duty_pct: 10,      // Initial duty cycle (10%)
            pin_config: channel::config::PinConfig::PushPull,  // Output mode
        })
        .unwrap();

        // Configure the ADC (Analog-to-Digital Converter)
    let mut adc_config = AdcConfig::new();
    
    // Enable GPIO0 as an analog input pin with calibration
    // - Uses AdcCalBasic calibration scheme for ADC1
    // - Sets attenuation to 11dB (allows measuring up to ~3.3V)
    let mut adc_pin = adc_config
        .enable_pin_with_cal::<_, AdcCalBasic<ADC1>>(
            peripherals.GPIO0,   // Analog input pin
            Attenuation::_11dB,  // Measurement range
        );
    
    // Initialize the ADC peripheral with our configuration
    let mut adc = Adc::new(peripherals.ADC1, adc_config);

    let delay = Delay::new();

    loop {
        let sample = nb::block!(adc.read_oneshot(&mut adc_pin)).unwrap();

        // Map ADC reading (0-4095) to duty cycle (0-31) for 5-bit resolution
        let duty = ((sample as u32 * 31) / 4095) as u8;
        channel0.set_duty(duty).unwrap();
        
        // Shorter delay for more responsive control
        delay.delay_millis(50);
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
