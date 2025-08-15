#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Delay, Duration, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{SdCard, TimeSource, Timestamp, VolumeIdx, VolumeManager};
use esp_hal::clock::CpuClock;
use esp_hal::dma_buffers;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::i2s::master::{DataFormat, I2s, Standard};
use esp_hal::spi::master::{Config, Spi};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use esp_println::{self as _, println};
use core::cell::RefCell;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

/// Code from https://github.com/rp-rs/rp-hal-boards/blob/main/boards/rp-pico/examples/pico_spi_sd_card.rs
/// A dummy timesource, which is mostly important for creating files.
#[derive(Default)]
pub struct DummyTimesource();

impl TimeSource for DummyTimesource {
    // In theory you could use the RTC of the rp2040 here, if you had
    // any external time synchronizing device.
    fn get_timestamp(&self) -> Timestamp {
        Timestamp {
            year_since_1970: 1,
            zero_indexed_month: 2,
            zero_indexed_day: 3,
            hours: 1,
            minutes: 4,
            seconds: 5,
        }
    }
}



#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.5.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::_80MHz);
    let peripherals = esp_hal::init(config);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    // Initialize I2S for audio output
    let dma_channel = peripherals.DMA_CH0;
    let (tx_buffer, tx_descriptors, _, _) = dma_buffers!(4 * 4092, 0);

    // Fill buffer with square wave data
    // Each frame: two 16-bit signed samples (left, right)
    fill_square_wave(tx_buffer, 440, 44100);

    println!(
        "Filled DMA buffer with {} bytes of square wave",
        tx_buffer.len()
    );

    let i2s = I2s::new(
        peripherals.I2S0,
        Standard::Philips,
        DataFormat::Data8Channel8,
        Rate::from_hz(44100),
        dma_channel,
    );
    let i2s = i2s.with_mclk(peripherals.GPIO0); // MCLK not used but required by driver
    let mut i2s_tx = i2s
        .i2s_tx
        .with_bclk(peripherals.GPIO1)
        .with_ws(peripherals.GPIO2)
        .with_dout(peripherals.GPIO3)
        .build(tx_descriptors);

    // Start DMA transfer with the pre-filled buffer
    println!("Starting DMA transfer with GPIOs:");
    println!("- MCLK: GPIO0 (unused)");
    println!("- BCLK: GPIO1");
    println!("- WS:   GPIO2");
    println!("- DOUT: GPIO5");

    // TODO: Spawn some tasks
    let _ = spawner;

    let mut transfer = i2s_tx.write_dma_circular(&tx_buffer).unwrap();
    // Now keep the main task alive but do NOT restart transfer inside the loop
    loop {
        Timer::after_millis(1000).await;
    }
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin


// Fill DMA buffer with a stereo square wave at a given frequency
fn fill_square_wave(buffer: &mut [u8], freq_hz: u32, sample_rate: u32) {
    let samples_per_cycle = sample_rate / freq_hz;
    let half_cycle = samples_per_cycle / 2;
    let frame_size = 4; // Stereo: 16-bit left + 16-bit right
    let mut i = 0;

    while i < buffer.len() / frame_size {
        let sample_val: i16 = if (i % samples_per_cycle as usize) < half_cycle as usize {
            0x7FFF  // max positive
        } else {
            0x8000u16 as i16  // max negative
        };
        // Left channel
        buffer[i * frame_size + 0] = sample_val as u8;
        buffer[i * frame_size + 1] = (sample_val >> 8) as u8;
        // Right channel
        buffer[i * frame_size + 2] = sample_val as u8;
        buffer[i * frame_size + 3] = (sample_val >> 8) as u8;
        i += 1;
    }
}
