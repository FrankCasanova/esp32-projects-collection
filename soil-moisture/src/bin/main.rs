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
use esp_hal::main;
use esp_hal::peripherals::ADC1;
use esp_println::println;
use {esp_backtrace as _, esp_println as _};

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    // generator version: 0.5.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let mut adc_config: AdcConfig<ADC1> = AdcConfig::new();
    let mut adc_pin = adc_config.enable_pin_with_cal::<_, AdcCalBasic<ADC1>>(peripherals.GPIO4, Attenuation::_11dB);
    let mut adc = Adc::new(peripherals.ADC1, adc_config);

    let delay = Delay::new();


    loop {
        let lecture = nb::block!(adc.read_oneshot(&mut adc_pin)).unwrap();
        
        if lecture < 1500 {
            println!("Soil is wet (value: {})", lecture);
        } else if lecture >= 1500 && lecture <= 3200 {
            println!("Ideal moisture level (value: {})", lecture);
        } else {
            println!("Plant needs water (value: {})", lecture);
        }
        
        delay.delay_millis(2000);
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
