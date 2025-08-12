#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::analog::adc::{Adc, AdcCalLine, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::timer::systimer::SystemTimer;
use esp_println as _;

/// Panic handler for the application
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

/// Convert temperature from Kelvin to Celsius
const fn kelvin_to_celsius(kelvin: f64) -> f64 {
    kelvin - 273.15
}

/// Convert temperature from Celsius to Kelvin
const fn celsius_to_kelvin(celsius: f64) -> f64 {
    celsius + (273.15 - 10.0) 
}

/// Maximum ADC value for 12-bit ADC
const ADC_MAX: f64 = 4095.0;

/// B-value of the thermistor (material constant)
const B_VALUE: f64 = 3950.0;

/// Reference temperature in Celsius (25°C = 298.15K)
const REF_TEMP: f64 = 25.0;

/// Thermistor resistance at reference temperature (25°C)
const REF_RES: f64 = 10_000.0;

/// Reference temperature in Kelvin
const REF_TEMP_K: f64 = celsius_to_kelvin(REF_TEMP);

/// Pull-up resistor value in ohms (10kΩ)
const R1_RES: f64 = REF_RES;

/// Convert ADC value to resistance using voltage divider formula
/// 
/// Formula: R2 = R1 * (ADC / (ADC_MAX - ADC))
/// Where R1 is the pull-up resistor and R2 is the thermistor
fn adc_to_resistance(adc_value: f64) -> f64 {
    let x: f64 = adc_value / (ADC_MAX - adc_value);
    R1_RES * x

    // Alternative method calculating Vout first then R2
    // let vout = (adc_value as f64 / ADC_MAX as f64) * VREF;
    // R1_RES * (vout / (VREF - vout))
}

/// Calculate temperature using the B-parameter equation
/// 
/// Formula: 1/T = 1/T0 + (1/B) * ln(R/R0)
/// Where:
/// - T is the temperature in Kelvin
/// - T0 is the reference temperature in Kelvin
/// - B is the B-value of the thermistor
/// - R is the current resistance
/// - R0 is the reference resistance
fn calculate_temperature(current_res: f64, b_val: f64) -> f64 {
    let ln_value = libm::log(current_res / REF_RES); // Use libm for `no_std`
    let inv_t = (1.0 / REF_TEMP_K) + ((1.0 / b_val) * ln_value);
    1.0 / inv_t
}

/// Main application entry point
#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.5.0

    // Initialize the ESP32-C3 with maximum CPU clock speed
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Initialize the system timer for Embassy async runtime
    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    // Configure ADC pin (GPIO0) for thermistor reading
    let adc_pin = peripherals.GPIO0;
    let mut adc2_config = AdcConfig::new();
    let mut pin = adc2_config
        .enable_pin_with_cal::<_, AdcCalLine<_>>(adc_pin, Attenuation::_11dB);
    let mut adc2 = Adc::new(peripherals.ADC1, adc2_config);

    // TODO: Spawn some tasks
    let _ = spawner;

    // Main loop for continuous temperature monitoring
    loop {
        // Read ADC value from thermistor
        let adc_value: u16 = nb::block!(adc2.read_oneshot(&mut pin)).unwrap();
        esp_println::println!("ADC: {}", adc_value);
        // let adc_value: f64 = ADC_LUT[adc_value as usize];
        // esp_println::println!("Corrected ADC: {}", adc_value);

        // Convert ADC value to resistance
        let current_res = adc_to_resistance(adc_value as f64);
        esp_println::println!("R2: {}", current_res);

        // Calculate temperature from resistance
        let temperature_kelvin = calculate_temperature(current_res, B_VALUE);
        let temperature_celsius = kelvin_to_celsius(temperature_kelvin);
        esp_println::println!("Temperature:{:.2} °C", temperature_celsius);

        // Wait for 1 second before next reading
        Timer::after(Duration::from_secs(1)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
