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
use embassy_sync::pubsub::PubSubChannel;
use embassy_time::Timer;
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

// Imagine our PubSubChannel is like a radio station frequency that multiple DJs can broadcast on.
// 5 publishers (DJs) can share this channel, with 1 subscriber (listener) and message history of 5.
static SHARED: PubSubChannel<CriticalSectionRawMutex, u8, 5, 1, 5> = PubSubChannel::new();

#[embassy_executor::task]
async fn async_task_one() {
    // Each task is like a DJ getting access to the radio station's broadcast equipment
    let pub1 = SHARED.publisher().unwrap();
    loop {
        // DJ 1 "goes live" with their message (number 1)
        pub1.publish_immediate(1);
        // DJ takes a break before next broadcast - like a scheduled show time
        Timer::after_millis(1000).await;
    }
}
#[embassy_executor::task]
async fn async_task_two() {
    // Another DJ gets their own broadcast equipment
    let pub2 = SHARED.publisher().unwrap();
    loop {
        // DJ 2 broadcasts their message (number 2)
        pub2.publish_immediate(2);
        // This DJ has a longer break between shows
        Timer::after_millis(2000).await;
    }
}
#[embassy_executor::task]
async fn async_task_three() {
    // DJ 3 joins the radio station
    let pub3 = SHARED.publisher().unwrap();
    loop {
        // Broadcasting number 3 on the shared frequency
        pub3.publish_immediate(3);
        // Even longer break between broadcasts
        Timer::after_millis(3000).await;
    }
}
#[embassy_executor::task]
async fn async_task_four() {
    // DJ 4 gets their broadcast slot
    let pub4 = SHARED.publisher().unwrap();
    loop {
        // Sending out number 4 signals
        pub4.publish_immediate(4);
        // Taking a 4 second breather
        Timer::after_millis(4000).await;
    }
}
#[embassy_executor::task]
async fn async_task_five() {
    // The final DJ joins the station
    let pub5 = SHARED.publisher().unwrap();
    loop {
        // Broadcasting number 5
        pub5.publish_immediate(5);
        // Longest break between shows
        Timer::after_millis(5000).await;
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
    spawner.spawn(async_task_one()).unwrap();
    spawner.spawn(async_task_two()).unwrap();
    spawner.spawn(async_task_three()).unwrap();
    spawner.spawn(async_task_four()).unwrap();
    spawner.spawn(async_task_five()).unwrap();

    // This is our radio listener - they have a receiver tuned to our station
    let mut sub = SHARED.subscriber().unwrap();

    loop {
        // The listener waits for any DJ to broadcast (like waiting for a song on the radio)
        let data = sub.next_message_pure().await;
        // When a message comes through, it's like hearing a song on the radio
        println!("DATA: {data} - Heard on the radio!")
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
