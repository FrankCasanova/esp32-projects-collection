#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use esp_hal::analog::adc::{Adc, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{DriveStrength, Level, Output, OutputConfig};
use esp_hal::peripherals::ADC1;
use esp_hal::main;
use esp_println::println;
use esp_hal::analog::adc::AdcCalBasic;

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

    // Configure the ESP32-C3 with maximum CPU clock speed
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    
    // Initialize peripherals (GPIO, ADC, etc.)
    let peripherals = esp_hal::init(config);

    // Initialize LEDs with their colors:
    // - Green LEDs on pins 2, 3 and 7 (low voltage indicators)
    // - Yellow LEDs on pins 4 and 5 (medium voltage indicators)
    // - Red LED on pin 6 (high voltage indicator)
    let led_config = OutputConfig::default()
        .with_drive_strength(DriveStrength::_5mA);
        
    let mut green1 = Output::new(peripherals.GPIO1, Level::Low, led_config.clone());
    let mut green2 = Output::new(peripherals.GPIO2, Level::Low, led_config.clone());
    let mut green3 = Output::new(peripherals.GPIO3, Level::Low, led_config.clone());
    let mut yellow1 = Output::new(peripherals.GPIO4, Level::Low, led_config.clone());
    let mut yellow2 = Output::new(peripherals.GPIO5, Level::Low, led_config.clone());
    let mut red = Output::new(peripherals.GPIO6, Level::Low, led_config);

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
    // Main loop runs forever
    loop {
        // Read analog value from GPIO0 (blocks until reading is complete)
        let sample = nb::block!(adc.read_oneshot(&mut adc_pin)).unwrap();
        
        // Convert raw ADC value (0-4095) to millivolts (0-3300mV)
        // 3300mV is the reference voltage for the ESP32-C3
        let voltage = sample as u32 * 3300 / 4095;
        
        // Print readings to serial console for debugging
        println!("Raw Reading: {sample}, Voltage Reading: {voltage}mV");

        // Control LEDs based on voltage thresholds
        // Each LED turns on when voltage exceeds its threshold
        green1.set_level(if voltage > 500 { Level::High } else { Level::Low });   // Green LED 1
        green2.set_level(if voltage > 1000 { Level::High } else { Level::Low });  // Green LED 2
        green3.set_level(if voltage > 1500 { Level::High } else { Level::Low });  // Green LED 3
        yellow1.set_level(if voltage > 2000 { Level::High } else { Level::Low }); // Yellow LED 1
        yellow2.set_level(if voltage > 2500 { Level::High } else { Level::Low }); // Yellow LED 2
        red.set_level(if voltage > 3000 { Level::High } else { Level::Low });     // Red LED
        
        // Wait 500ms before taking next reading
        delay.delay_millis(500_u32);
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
