#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use core::future::IntoFuture;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration as du;
use defmt::info;
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use esp_hal::analog::adc::{Adc, AdcCalBasic, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::i2s::master::I2sTx;
use esp_hal::i2s::master::{DataFormat, I2s, Standard};
use esp_hal::rtc_cntl::sleep::TimerWakeupSource;
use esp_hal::rtc_cntl::{reset_reason, wakeup_cause, Rtc};
use esp_hal::system::Cpu;
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::Blocking;
use esp_hal::{dma_buffers, Async};
use esp_println::{self as _, println};
use wav_hex_player::audios::FAIRY_CAUTION;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

/// Code from https://github.com/rp-rs/rp-hal-boards/blob/main/boards/rp-pico/examples/pico_spi_sd_card.rs

const HEADER_SIZE: usize = 44;
const DMA_BUFFER_SIZE: usize = 65472; // Or whatever size dma_buffers! creates 4 * 4092 * 4
static AUDIO_TRIGGER: Signal<CriticalSectionRawMutex, ()> = Signal::new(); // Replace AUDIO_ENABLED
static AUDIO_MACHINE: Mutex<CriticalSectionRawMutex, Option<I2sTx<'static, Blocking>>> =
    Mutex::new(None);

#[embassy_executor::task]
async fn audio(
    audio_machine: &'static Mutex<CriticalSectionRawMutex, Option<I2sTx<'static, Blocking>>>,
    tx_buffer: &'static mut [u8],
) {
    let pcm_data = &FAIRY_CAUTION[HEADER_SIZE..];
    let pcm_len = pcm_data.len();
    println!("PCM Length: {}", pcm_len);

    loop {
        info!("STARTING LOOP FROM AUDIO TASK");
        // Check if audio playback is enabled based on temperature
        AUDIO_TRIGGER.wait().await;

        println!("Temperature condition met. Starting audio playback...");

        let mut offset = 0;
        // Play the entire audio clip in chunks
        while offset < pcm_len {
            let chunk_size = core::cmp::min(DMA_BUFFER_SIZE, pcm_len - offset);
            println!("offset: {offset}");

            // Copy PCM data to the DMA buffer
            tx_buffer[..chunk_size].copy_from_slice(&pcm_data[offset..offset + chunk_size]);

            // Zero-pad the rest of the buffer if necessary
            if chunk_size < DMA_BUFFER_SIZE {
                tx_buffer[chunk_size..].fill(0);
            }

            // Perform the DMA transfer
            let mut transfer_guard = audio_machine.lock().await;
            if let Some(i2s_tx) = transfer_guard.as_mut() {
                // Start transfer and wait for completion
                i2s_tx.write_dma(&tx_buffer).unwrap().is_done();
            }
            // Release the lock as soon as possible
            drop(transfer_guard);

            offset += chunk_size;

            // Optional: Small delay between chunks if needed
            // Timer::after_micros(10).await;
        }
        println!("Audio playback finished for this loop.");
        // Optional: Add a small delay before checking the condition again
        // to avoid playing back-to-back immediately if the clip is short.
        // Timer::after_millis(100).await;

        // Small delay at the end of the loop to prevent excessive checking
        // Timer::after_millis(100).await;
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
    let (tx_buffer, tx_descriptors, _, _) = dma_buffers!(DMA_BUFFER_SIZE, 0);

    let i2s = I2s::new(
        peripherals.I2S0,
        Standard::Philips,
        DataFormat::Data16Channel16,
        Rate::from_hz(11025),
        dma_channel,
    );
    let i2s = i2s.with_mclk(peripherals.GPIO0); // MCLK not used but required by driver
    let i2s_tx: esp_hal::i2s::master::I2sTx<'_, esp_hal::Blocking> = i2s
        .i2s_tx
        .with_bclk(peripherals.GPIO1)
        .with_ws(peripherals.GPIO2)
        .with_dout(peripherals.GPIO3)
        .build(tx_descriptors);

    {
        *(AUDIO_MACHINE.lock().await) = Some(i2s_tx);
    }

    let mut adc_config = AdcConfig::new();
    let mut adc_pin =
        adc_config.enable_pin_with_cal::<_, AdcCalBasic<_>>(peripherals.GPIO4, Attenuation::_11dB);
    let mut adc = Adc::new(peripherals.ADC1, adc_config);

    spawner.spawn(audio(&AUDIO_MACHINE, tx_buffer)).unwrap();

    let mut prev_moisture: Option<u16> = None; // Track previous state
    
    loop {
        Timer::after_secs(900).await;
        
        let lecture = nb::block!(adc.read_oneshot(&mut adc_pin)).unwrap();
        let is_dry = lecture > 3200;
        
        // Check state transition to dry
        while is_dry && prev_moisture.map(|prev| prev <= 3200).unwrap_or(true) {
            info!("Plant needs water (value: {})", lecture);
            AUDIO_TRIGGER.signal(());
        }
        
        prev_moisture = Some(lecture); // Update state
        println!("{}", prev_moisture.unwrap())
    }
}

// for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
