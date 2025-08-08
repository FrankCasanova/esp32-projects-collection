#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use esp_hal::analog::adc::AdcCalBasic;
use esp_hal::analog::adc::{Adc, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{DriveStrength, Level, Output, OutputConfig};
use esp_hal::main;
use esp_hal::peripherals::ADC1;

use esp_println::{self as _, println};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Light level thresholds for controlling the LEDs
// The ADC reading ranges from 0 (dark) to 4095 (bright) for 12-bit resolution
const LOW_LIGHT_THRESHOLD: u16 = 1000;       // Below this: very dark -> no LEDs
const MEDIUM_LIGHT_THRESHOLD: u16 = 2000;    // Between LOW and MEDIUM: dim -> LED1 on
const HIGH_LIGHT_THRESHOLD: u16 = 3000;      // Between MEDIUM and HIGH: medium -> LED1 and LED2 on
                                             // Above HIGH: bright -> all LEDs on

#[main]
fn main() -> ! {
    // Configure the system clock to run at 80 MHz
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::_80MHz);
    // Initialize the peripherals
    let peripherals = esp_hal::init(config);

    // Configure the LED pins as outputs
    let led_config = OutputConfig::default().with_drive_strength(DriveStrength::_5mA);
    let mut led_1 = Output::new(peripherals.GPIO1, Level::Low, led_config);
    let mut led_2 = Output::new(peripherals.GPIO2, Level::Low, led_config);
    let mut led_3 = Output::new(peripherals.GPIO3, Level::Low, led_config);

    // Configure the ADC (Analog to Digital Converter)
    let mut adc_config: AdcConfig<ADC1> = AdcConfig::new();
    // Enable the ADC pin (GPIO4) with basic calibration and 11dB attenuation (for 0-3.3V range)
    let mut adc_pin = adc_config
        .enable_pin_with_cal::<_, AdcCalBasic<ADC1>>(peripherals.GPIO4, Attenuation::_11dB);
    // Create the ADC instance
    let mut adc = Adc::new(peripherals.ADC1, adc_config);

    // Create a delay instance for timing
    let delay = Delay::new();

    // Main program loop
    loop {
        // Read the analog value from the LDR sensor
        // nb::block! waits until the ADC conversion is complete
        let lecture = nb::block!(adc.read_oneshot(&mut adc_pin)).unwrap();

        // Control LEDs based on the light level
        if lecture < LOW_LIGHT_THRESHOLD {
            // Very dark: turn off all LEDs
            led_1.set_low();
            led_2.set_low();
            led_3.set_low();
        } else if lecture < MEDIUM_LIGHT_THRESHOLD {
            // Dim light: turn on only LED1
            led_1.set_high();
            led_2.set_low();
            led_3.set_low();
        } else if lecture < HIGH_LIGHT_THRESHOLD {
            // Medium light: turn on LED1 and LED2
            led_1.set_high();
            led_2.set_high();
            led_3.set_low();
        } else {
            // Bright light: turn on all LEDs
            led_1.set_high();
            led_2.set_high();
            led_3.set_high();
        }

        // Print the ADC reading for debugging and calibration
        // Students can monitor this output to adjust thresholds
        println!("ADC Reading: {lecture}");

        // Add 1-second delay to stabilize readings and reduce flickering
        // This also makes the serial output easier to read
        delay.delay_millis(1000u32);
    }
    // For more examples, see: 
    // https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
