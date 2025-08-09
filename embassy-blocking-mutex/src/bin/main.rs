// This program demonstrates async programming in embedded systems using the Embassy framework
// We'll use a shared counter protected by a mutex to illustrate safe concurrent access

// Analogies to help understand:
// - Embassy Executor: Like a meeting coordinator who manages who gets to speak and when
// - Async Tasks: Like individual speakers who can pause (await) and let others speak
// - Mutex: Like a conference room that only one speaker can use at a time
// - Shared State: Like a shared whiteboard in the conference room

// Embedded-specific notes:
// - `no_std`: We're not using the standard library (typical for embedded)
// - `no_main`: We're defining our own custom entry point
// - The deny attribute prevents unsafe memory operations specific to ESP hardware
#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use core::cell::RefCell;


use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::systimer::SystemTimer;
use esp_println::{self as _, println};

// Panic handler: In embedded systems without an OS, we define what happens on errors
// Here we just loop forever - a safe behavior for embedded devices
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();


// Shared state: A counter protected by a mutex
// Think of this as a shared whiteboard in a conference room:
// - The mutex ensures only one task can "use the room" at a time
// - CriticalSectionRawMutex uses hardware interrupts for safety
// - RefCell provides interior mutability (safe mutation through shared reference)
static SHARED: Mutex<CriticalSectionRawMutex, RefCell<u32>,> =Mutex::new(RefCell::new(0));

// Async task: Runs independently in the background
// Think of this as a speaker who periodically updates the shared whiteboard
#[embassy_executor::task]
async fn async_task() {
    loop {
        // Lock the mutex to access the shared counter
        // Like requesting exclusive access to the conference room
        SHARED.lock(|f|{
            let val = f.borrow_mut().wrapping_add(1);
            f.replace(val);
            println!("THREAD: {val}") // Show current value
        });
        
        // Non-blocking delay: "Take a break" for 1 second
        // While waiting, other tasks can run (like letting another speaker talk)
        Timer::after(Duration::from_millis(1000)).await;
    }
}

// Main async function: The entry point managed by Embassy
#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // Hardware initialization
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Set up timer for Embassy's async runtime
    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!"); // Log successful setup

    // Spawn our async task (like inviting a speaker to the meeting)
    spawner.spawn(async_task()).unwrap();

    // Main loop: Periodically check the shared value
    loop {
        
        // Wait 5 seconds without blocking other tasks
        Timer::after_millis(5000).await;
        
        // Briefly lock the mutex to read the shared value
        SHARED.lock(|f|{
            let val = f.clone().into_inner(); 
            println!("MAIN: {val}"); // Report current value
        });
    }


    // For more examples: https://github.com/esp-rs/esp-hal
}
