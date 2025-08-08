//! Real-time Timer for ESP32-C3
//! 
//! This program implements a timer with start, pause, and reset functionality using:
//! - Hardware timer with interrupt for precise timing
//! - GPIO buttons for user control
//! - Critical sections for thread-safe shared data access
//!
//! Key concepts:
//! 1. Timer interrupt triggers every second to update time
//! 2. Buttons with software debouncing to prevent multiple triggers
//! 3. Global variables shared between main loop and interrupt handler
//! 4. Critical sections to protect shared resources

#![no_std]       // No standard library (embedded system)
#![no_main]      // No main function (custom entry point)
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

// Hardware abstraction and peripheral access
use esp_hal::{
    clock::CpuClock, 
    gpio::{Input, InputConfig, Pull}, 
    handler, main, 
    time::Duration, 
    timer::{timg::{Timer, TimerGroup}, Timer as TimerTrait}
};
use esp_println::println;  // For serial output

// Concurrency primitives for safe shared memory access
use core::cell::{Cell, RefCell};
use critical_section::Mutex;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Global timer instance shared between main loop and interrupt handler
// Wrapped in Mutex and RefCell for safe concurrent access
static GLOBAL_TIMER: Mutex<RefCell<Option<Timer>>> = Mutex::new(RefCell::new(None));

// Global flag to signal timer interrupt occurred
// Used to coordinate between interrupt context and main loop
static GLOBAL_FLAG: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));

/// Time tracking structure
/// 
/// Tracks hours, minutes, and seconds of elapsed time
/// Implements rollover at 24 hours
struct Time {
    seconds: u32,  // 0-59
    minutes: u32,  // 0-59
    hours: u32,    // 0-23
}

// Timer Interrupt Service Routine (ISR)
// Called automatically when timer reaches zero
#[handler]
fn tg0_t0_level() {
    // Enter critical section to safely access shared resources
    critical_section::with(|cs| {
        // Clear the timer interrupt flag so it can trigger again
        GLOBAL_TIMER
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap()
            .clear_interrupt();

        // Set global flag to notify main loop that a timer event occurred
        GLOBAL_FLAG.borrow(cs).set(true);
    });
}

#[main]  // Special attribute for embedded entry point
fn main() -> ! {
    // generator version: 0.5.0

    // Configure ESP32 hardware
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::_80MHz);
    let peripherals = esp_hal::init(config);

    // Configure buttons with internal pull-up resistors
    let button_config = InputConfig::default().with_pull(Pull::Up);

    // Initialize buttons on GPIO pins
    // Note: Active low (pressed = LOW, released = HIGH)
    let start_button = Input::new(peripherals.GPIO0, button_config);   // Start/resume timer
    let pause_button = Input::new(peripherals.GPIO1, button_config);   // Pause timer
    let reset_button = Input::new(peripherals.GPIO2, button_config);   // Reset timer to zero

    // Setup hardware timer (Timer Group 0, Timer 0)
    let timer_group_0 = TimerGroup::new(peripherals.TIMG0);
    let timer_0 = timer_group_0.timer0;

    // Configure timer to trigger interrupt every second
    timer_0
        .load_value(Duration::from_secs(1))  // Set interval to 1 second
        .unwrap();

    // Configure timer interrupt
    timer_0.set_interrupt_handler(tg0_t0_level);  // Attach ISR
    timer_0.enable_interrupt(true);               // Enable interrupt generation
    timer_0.start();                              // Start counting down

    // Move timer instance to global variable for access in ISR
    critical_section::with(|cs| GLOBAL_TIMER.borrow_ref_mut(cs).replace(timer_0));

    // Initialize time tracking structure
    let mut time = Time {
        seconds: 0_u32,
        minutes: 0_u32,
        hours: 0_u32,
    };

    // Button debounce flags
    // Prevents multiple triggers from a single button press
    let mut start_debounce = false;  // For start button
    let mut pause_debounce = false;  // For pause button
    let mut reset_debounce = false;  // For reset button

    // Main application loop
    loop {
        // Check if timer interrupt occurred (in critical section)
        critical_section::with(|cs| {
            if GLOBAL_FLAG.borrow(cs).get() {
                // Clear flag to wait for next interrupt
                GLOBAL_FLAG.borrow(cs).set(false);
                
                // Update time (seconds, minutes, hours)
                time.seconds = time.seconds.wrapping_add(1);
                
                // Handle rollover from seconds to minutes
                if time.seconds > 59 {
                    time.minutes += 1;
                    time.seconds = 0;
                }
                
                // Handle rollover from minutes to hours
                if time.minutes > 59 {
                    time.hours += 1;
                    time.minutes = 0;
                }
                
                // Handle rollover after 24 hours
                if time.hours > 23 {
                    time.seconds = 0;
                    time.minutes = 0;
                    time.hours = 0;
                }
                
                // Display updated time over serial
                println!(
                    "Elapsed Time {:0>2}:{:0>2}:{:0>2}",
                    time.hours, time.minutes, time.seconds
                );
            }
        });

        // START button handling (active low)
        if start_button.is_low() {
            if !start_debounce {
                // First detection of button press
                start_debounce = true;
                
                // Start/resume timer (in critical section)
                critical_section::with(|cs| {
                    if let Some(timer) = GLOBAL_TIMER.borrow_ref_mut(cs).as_mut() {
                        timer.start();
                    }
                });
            }
        } else {
            // Button released, reset debounce flag
            start_debounce = false;
        }

        // PAUSE button handling (active low)
        if pause_button.is_low() {
            if !pause_debounce {
                // First detection of button press
                pause_debounce = true;
                
                // Pause timer (in critical section)
                critical_section::with(|cs| {
                    if let Some(timer) = GLOBAL_TIMER.borrow_ref_mut(cs).as_mut() {
                        timer.stop();
                    }
                });
            }
        } else {
            // Button released, reset debounce flag
            pause_debounce = false;
        }

        // RESET button handling (active low)
        if reset_button.is_low() {
            if !reset_debounce {
                // First detection of button press
                reset_debounce = true;
                
                // Critical section for shared resources
                critical_section::with(|cs| {
                    // Clear any pending timer flags
                    GLOBAL_FLAG.borrow(cs).set(false);
                    
                    // Reset timer counter
                    if let Some(timer) = GLOBAL_TIMER.borrow_ref_mut(cs).as_mut() {
                        timer.reset();
                    }
                });
                
                // Reset time display to 00:00:00
                time.seconds = 0;
                time.minutes = 0;
                time.hours = 0;
            }
        } else {
            // Button released, reset debounce flag
            reset_debounce = false;
        }
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
