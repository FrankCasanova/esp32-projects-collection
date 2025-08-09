//! Async Programming in Embedded Systems with Embassy
//!
//! This example demonstrates how to use async/await in embedded systems using the Embassy framework.
//! Embassy provides an efficient async executor that allows writing non-blocking code while maximizing
//! hardware utilization - perfect for resource-constrained embedded devices.
//!
//! Key concepts demonstrated:
//! 1. Creating and spawning async tasks
//! 2. Using atomic variables for safe shared state
//! 3. Non-blocking delays with Embassy timers
//! 4. The Embassy executor lifecycle

#![no_std]  // No standard library - we're running on bare metal!
#![no_main] // No standard main function - we define our own entry point

// Safety guard: Prevent accidental use of mem::forget with esp_hal types
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use core::sync::atomic::{AtomicU32, Ordering};
use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::systimer::SystemTimer;
use esp_println::{self as _, println};

/// Our custom panic handler
///
/// In embedded systems, we need to define what happens when something goes wrong.
/// This simple implementation just loops forever - in a real application you might
/// want to log the error and reset the device.
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// Application Descriptor for ESP-IDF Bootloader
//
// Think of this as the "ID card" for your application that the bootloader checks
// before allowing it to run. It includes metadata like the application name,
// version, and security flags.
esp_bootloader_esp_idf::esp_app_desc!();

/// Shared Atomic Counter
///
/// This atomic variable is shared between our main task and the async task.
/// Atomics are like a "shared whiteboard" that multiple tasks can safely read and
/// write to without causing data races. The operations are indivisible (atomic) - 
/// they can't be interrupted by other tasks.
///
/// Why AtomicU32?
/// - Atomic operations are safe for concurrent access
/// - No need for locks in async contexts
/// - Perfect for simple shared state like counters
static SHARED: AtomicU32 = AtomicU32::new(0);

/// Async Task: Shared Counter Incrementer
///
/// This is an independent async task that runs concurrently with the main loop.
/// It demonstrates how we can have multiple "virtual threads" of execution on a 
/// single core using async/await.
///
/// Analogy: Think of async tasks like kitchen timers. You can set multiple timers
/// (tasks) and the chef (CPU) can work on other things while waiting for them to finish.
#[embassy_executor::task]
async fn async_task() {
    loop {
        // 1. Load the current value from the shared atomic counter
        //    Using Relaxed ordering since we don't need strict synchronization 
        //    between tasks for this simple counter
        let shared_value = SHARED.load(Ordering::Relaxed);
        
        // 2. Increment the value (wrapping_add prevents overflow panics)
        //    This is equivalent to shared_value + 1 but safely wraps around
        SHARED.store(shared_value.wrapping_add(1), Ordering::Relaxed);
        println!("THREAD 1: Current counter value: {shared_value}");
        
        // 3. Non-blocking delay - the magic of async!
        //    This .await yields control back to the executor while waiting,
        //    allowing other tasks to run. The CPU isn't blocked during this wait!
        Timer::after(Duration::from_millis(1000)).await;
    }
}

/// Main Application Entry Point
///
/// The #[embassy_executor::main] attribute sets up the Embassy async executor.
/// This is the heart of our async system - it manages task scheduling and execution.
#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // Configure the hardware
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::_80MHz);
    let peripherals = esp_hal::init(config);

    // Set up system timer and initialize Embassy with an alarm
    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    // Embassy is now ready to run async tasks!
    info!("Embassy initialized!");

    // Spawn our async counter task
    // This is like launching a background worker. The executor will manage its execution.
    spawner.spawn(async_task()).unwrap();

    // Main loop - also async!
    // This loop runs concurrently with the async_task we spawned.
    loop {
        
        // Wait for 1 second without blocking
        Timer::after(Duration::from_millis(5000)).await;
        
        // Read the current value of the shared counter
        let shared = SHARED.load(Ordering::Relaxed);
        
        // Print the current counter value
        // Note: println! is async-friendly and won't block execution
        println!("MAIN: Current counter value: {shared}")
    }

    // For more examples and inspiration:
    // https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
