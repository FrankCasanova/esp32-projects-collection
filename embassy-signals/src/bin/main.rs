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
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
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

static SHARED: Signal<CriticalSectionRawMutex, u8> = Signal::new();

#[embassy_executor::task]
async fn async_task() {
    // Create a counter that starts at 0 (like a scoreboard starting fresh)
    let mut counter = 0_u8;
    
    // This loop runs forever, like a clock ticking every second
    loop {
        // SAFELY increment our counter (wrapping_add handles overflow like an odometer rolling over)
        counter = counter.wrapping_add(1);
        
        // Send the updated value through our SHARED "mailbox" (signal)
        // Think of this like putting a new number in a shared bulletin board
        SHARED.signal(counter);
        
        // Wait for 1000 milliseconds (1 second) before repeating
        // Like setting a kitchen timer between updates
        Timer::after_millis(1000).await;
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

    // Main loop acts like a receptionist checking the mailbox constantly
    loop {
        // Wait for a new value to arrive in our SHARED "mailbox"
        // (This .await politely waits without wasting CPU resources)
        let val = SHARED.wait().await;
        
        // Print the received value to the console
        // Like announcing the latest score over a loudspeaker
        println!("MAIN: {val}");
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
