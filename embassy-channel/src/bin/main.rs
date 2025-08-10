#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

//! ASYNC CHANNEL DEMO WITH WORKER ANALOGY
//! 
//! Imagine this program as a factory with:
//! - 5 workers (async tasks) producing different items
//! - 1 conveyor belt (channel) transporting items
//! - 1 supervisor (main loop) collecting finished products
//! 
//! Key concepts:
//! - Workers operate independently but coordinate through the conveyor belt
//! - .await = worker waits WITHOUT blocking others (efficient waiting)
//! - Channel = thread-safe communication line (like a physical conveyor belt)
//! - Spawning tasks = hiring workers to start their jobs

use defmt::info;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Timer;
use embassy_sync::channel::Channel;
use esp_hal::clock::CpuClock;
use esp_hal::timer::systimer::SystemTimer;
use esp_println::{self as _, println};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Our shared conveyor belt (channel) characteristics:
// - Safety: CriticalSectionRawMutex (like a safety lock on the belt)
// - Capacity: 5 items max (physical belt size limit)
// - Item type: u8 (simple numeric "products")
static SHARED: Channel<CriticalSectionRawMutex, u8, 5> = Channel::new();


#[embassy_executor::task]
async fn async_task_one() {
    // Worker 1's job: 
    // 1. Place product "11" on conveyor belt (non-blocking)
    // 2. Take 1111ms break before next item
    loop {
        // Like a worker placing an item on the belt
        SHARED.send(11).await;  // .await = waits if belt is full WITHOUT blocking others
        Timer::after_millis(1111).await;  // Worker takes a timed break
    }
}

#[embassy_executor::task]
async fn async_task_two() {
    // Worker 2's pattern:
    // - Different product (22)
    // - Slower production rate (2222ms)
    // Note: Runs CONCURRENTLY with other workers
    loop {
        SHARED.send(22).await;
        Timer::after_millis(2222).await;
    }
}

#[embassy_executor::task]
async fn async_task_three() {
    // Worker 3 demonstrates:
    // - Each worker maintains OWN STATE
    // - No direct communication between workers
    // - All coordination via conveyor belt (channel)
    loop {
        SHARED.send(33).await;
        Timer::after_millis(3333).await;
    }
}

#[embassy_executor::task]
async fn async_task_four() {
    // Worker 4's characteristics:
    // - Unique product ID (44)
    // - Medium-slow production cycle (4444ms)
    // Demonstrates: All workers share same channel but don't interfere
    loop {
        SHARED.send(44).await;
        Timer::after_millis(4444).await;
    }
}

#[embassy_executor::task]
async fn async_task_five() {
    // Worker 5 shows:
    // - Longest production cycle (5555ms)
    // - Despite different speeds, all workers coexist peacefully
    // - Channel handles synchronization automatically
    loop {
        SHARED.send(55).await;
        Timer::after_millis(5555).await;
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
    // Hiring our factory workers (starting async tasks)
    // Note: .spawn() is instantaneous - like handing a worker their job instructions
    spawner.spawn(async_task_one()).unwrap();   // Worker 1 starts
    spawner.spawn(async_task_two()).unwrap();   // Worker 2 starts
    spawner.spawn(async_task_three()).unwrap(); // Worker 3 starts
    spawner.spawn(async_task_four()).unwrap();  // Worker 4 starts
    spawner.spawn(async_task_five()).unwrap();  // Worker 5 starts


    // Supervisor loop: Collects items from conveyor belt
    loop {
        // .await here is CRUCIAL - supervisor waits for items WITHOUT:
        // - Wasting CPU cycles (efficient)
        // - Blocking workers (they keep producing)
        let data = SHARED.receive().await;  // Like taking an item off the belt
        
        // Important: This print is NOT async-blocking because:
        // 1. It's fast
        // 2. Workers continue during this operation
        println!("DATA: {data}");  // Logging the received product
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
