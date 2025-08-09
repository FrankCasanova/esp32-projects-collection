#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::sha;
use esp_hal::timer::systimer::SystemTimer;
use esp_println::{self as _, println};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

/// Shared counter protected by a mutex (like a classroom microphone - only one speaker at a time)
/// Think of this as a whiteboard that multiple students want to write on. The mutex ensures only 
/// one student can write at a time to prevent chaotic overwrites.
static SHARED: Mutex<CriticalSectionRawMutex, u32> =
    Mutex::new(0);

/// Background task that increments the shared counter every second
/// Like a student repeatedly raising their hand to update the whiteboard
#[embassy_executor::task]
async fn async_task() {
    // This task acts like an eager student constantly updating the shared counter
    loop {
        {
            // "Raise hand" for the microphone (lock) to update the value
            let mut shared = SHARED.lock().await;
            *shared = shared.wrapping_add(1);  // Safely increment counter
            
            // Hold microphone while thinking (500ms wait) - NOT IDEAL IN REAL CODE!
            // We only do this here to demonstrate lock behavior
            Timer::after(Duration::from_millis(500)).await;
            println!("THREAD: {shared}")       // Announce update
        } // Microphone released here when `shared` goes out of scope
        
        // Wait another 500ms WITHOUT holding the lock (better practice)
        Timer::after(Duration::from_millis(500)).await;
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.5.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    // TODO: Spawn some tasks
    spawner.spawn(async_task()).unwrap();

    // CRITICAL SECTION ORDER EXPLANATION:
    // We put the timer FIRST (like waiting your turn) before grabbing the lock because:
    // 1. The 5-second wait allows other tasks to freely modify SHARED (like letting other students write on the whiteboard)
    // 2. Only AFTER waiting do we briefly "grab the microphone" (lock) to read the current value
    // 3. This pattern ensures we don't block others while waiting (good async citizenship!)
    //
    // ANALOGY: Imagine waiting in line for pizza (timer) BEFORE checking the menu board (lock).
    // You wouldn't stand in front of the menu board while waiting in line - that blocks others!
    loop {
        Timer::after_millis(5000).await;       // Wait 5s WITHOUT blocking others
        let shared = SHARED.lock().await;      // Quickly "grab the microphone" to read value
        println!("MAIN: {shared}");            // Announce current value
    }                                          // Lock automatically released here

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
