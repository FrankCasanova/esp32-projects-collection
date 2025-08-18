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
use embassy_time::Timer;
use esp_hal::analog::adc::{Adc, AdcCalBasic, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::dma_buffers;
use esp_hal::i2s::master::I2sTx;
use esp_hal::i2s::master::{DataFormat, I2s, Standard};
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use esp_hal::Blocking;
use esp_println::{self as _, println};
use wav_hex_player::audio_task::audio;
use wav_hex_player::{AudioClip, CURRENT_AUDIO, AUDIO_TRIGGER, DMA_BUFFER_SIZE};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

/// Code from https://github.com/rp-rs/rp-hal-boards/blob/main/boards/rp-pico/examples/pico_spi_sd_card.rs

static AUDIO_MACHINE: Mutex<CriticalSectionRawMutex, Option<I2sTx<'static, Blocking>>> =
    Mutex::new(None);

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
        Timer::after_secs(4).await;
        info!("WAITING FINISHED");
        
        let lecture = nb::block!(adc.read_oneshot(&mut adc_pin)).unwrap();
        let is_dry = lecture > 3200;
        
        // Check state transition to dry
        while is_dry && prev_moisture.map(|prev| prev <= 3200).unwrap_or(true) {
            // Set audio to fairy caution for dry condition
            {
                let mut guard = CURRENT_AUDIO.lock().await;
                *guard = AudioClip::FairyCaution;
            }
            AUDIO_TRIGGER.signal(());
            info!("SIGNAL SENT");
            info!("Plant needs water (value: {})", lecture);
            Timer::after_secs(10).await
        }
        
        // Set audio to wav audio for wet condition
        if !is_dry {
            let mut guard = CURRENT_AUDIO.lock().await;
            *guard = AudioClip::WavAudio;
        }
        
        prev_moisture = Some(lecture); // Update state
        println!("{}", prev_moisture.unwrap())
    }
}

// for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
