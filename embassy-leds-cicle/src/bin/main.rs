#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{DriveStrength, Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::timer::systimer::SystemTimer;
use esp_println::{self as _};
use core::sync::atomic::{AtomicU32, Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

static BLINK_DELAY: AtomicU32 = 
    AtomicU32::new(500_u32);

static BUTTON: Mutex<CriticalSectionRawMutex, Option<Input<'static>>> = 
    Mutex::new(None);
#[embassy_executor::task]
async fn press_button(button: &'static Mutex<CriticalSectionRawMutex, Option<Input<'static>>>) {
    // Main button handling loop - runs continuously
    loop {
        // SECTION 1: Button press detection with debouncing
        {
            // Acquire lock to access the button resource (shared Mutex)
            let mut button_guard = button.lock().await;
            
            // Check if button is initialized
            if let Some(btn) = button_guard.as_mut() {
                // Wait for physical button press (rising edge detection)
                btn.wait_for_rising_edge().await;
                info!("BUTTON PRESSED - Edge detected");
                
                // DEBOUNCING: Wait 50ms to ignore mechanical switch bouncing
                // Analogy: Like waiting for a spring to settle before reading its position
                Timer::after_millis(50).await;
            }
        }

        // SECTION 2: Blink delay adjustment logic
        // Load current blink delay from atomic variable
        let delay = BLINK_DELAY.load(Ordering::Relaxed);
        
        if delay < 50 {
            // Reset to initial delay when we reach minimum value
            // (Prevent negative values and maintain usability)
            BLINK_DELAY.store(500, Ordering::Relaxed);
            info!("DELAY RESET TO 500ms");
        } else {
            // Decrease delay by 50ms each button press
            let new_delay = delay - 50;
            BLINK_DELAY.store(new_delay, Ordering::Relaxed);
            info!("Delay decreased to {}ms", new_delay);
        }
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // Initialize hardware with maximum CPU clock speed
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Set up system timer for async executor
    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy executor initialized - async runtime ready!");

    // Configure Delay Button to Pull Up input
    let button_config = InputConfig::default().with_pull(Pull::Up);
    let button = Input::new(peripherals.GPIO0, button_config);
    // Inner scope is so that once the mutex is written to, the MutexHuard is dropped
    // thus the Mutex is released
    {
        *(BUTTON.lock().await) = Some(button);
    }

    // LED CONFIGURATION: Create array of 5 LEDs (GPIO1-GPIO5)
    // Using 5mA drive strength - enough for most LEDs without resistors
    let led_config = OutputConfig::default().with_drive_strength(DriveStrength::_5mA);
    let mut leds: [Output; 5] = [
        Output::new(peripherals.GPIO1, Level::Low, led_config),  // LED 1
        Output::new(peripherals.GPIO2, Level::Low, led_config),  // LED 2
        Output::new(peripherals.GPIO3, Level::Low, led_config),  // LED 3
        Output::new(peripherals.GPIO4, Level::Low, led_config),  // LED 4
        Output::new(peripherals.GPIO5, Level::Low, led_config),  // LED 5
    ];

    // TODO: Spawn some tasks
    spawner.spawn(press_button(&BUTTON)).unwrap();

    // MAIN LOOP: Knight Rider-style LED animation
    loop {
        // Forward pass: Light LEDs from first to last
        for led in &mut leds {
            led.set_high();  // Turn on current LED
            // Wait with dynamic delay controlled by button presses
            Timer::after_millis(BLINK_DELAY.load(Ordering::Relaxed) as u64).await;
            led.set_low();   // Turn off before moving to next
            Timer::after_millis(50).await;  // Fixed pause between LEDs
        }
        
        // Backward pass: Light LEDs from last to first
        for led in leds.iter_mut().rev() {
            led.set_high();
            Timer::after_millis(BLINK_DELAY.load(Ordering::Relaxed) as u64).await;
            led.set_low();
            Timer::after_millis(50).await;
        }
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
