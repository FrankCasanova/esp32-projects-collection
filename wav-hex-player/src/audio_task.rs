#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use crate::audios::{AudioClip, FAIRY_CAUTION, FAIRY_SONG_1, WAV_DATA, FAIRY_SONG_2, FAIRY_SONG_3};
use crate::CURRENT_AUDIO;
use defmt::info;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_hal::i2s::master::I2sTx;
use esp_hal::Blocking;
use esp_println::{self as _, println};

use crate::AUDIO_TRIGGER;
use crate::{DMA_BUFFER_SIZE, HEADER_SIZE};

// Or whatever size dma_buffers! creates 4 * 4092 * 4
// static AUDIO_TRIGGER: Signal<CriticalSectionRawMutex, ()> = Signal::new(); // Replace AUDIO_ENABLED

#[embassy_executor::task]
pub async fn audio(
    audio_machine: &'static Mutex<CriticalSectionRawMutex, Option<I2sTx<'static, Blocking>>>,
    tx_buffer: &'static mut [u8],
) {
    // Get current audio selection
    loop {
        AUDIO_TRIGGER.wait().await;
        let current_audio = {
            let guard = CURRENT_AUDIO.lock().await;
            *guard
        };

        println!("{:?}", current_audio);
        // Select audio clip based on current selection
        let pcm_data = match current_audio {
            AudioClip::FairyCaution => &FAIRY_CAUTION[HEADER_SIZE..],
            AudioClip::WavAudio => &WAV_DATA[HEADER_SIZE..],
            AudioClip::FairySong1 => &FAIRY_SONG_1[HEADER_SIZE..],
            AudioClip::FairySong2 => &FAIRY_SONG_2[HEADER_SIZE..],
            AudioClip::FairySong3 => &FAIRY_SONG_3[HEADER_SIZE..],
            AudioClip::None => &[],
        };

        let pcm_len = pcm_data.len();
        println!("PCM Length: {}", pcm_len);
        info!("STARTING LOOP FROM AUDIO TASK");
        // Check if audio playback is enabled based on temperature

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
