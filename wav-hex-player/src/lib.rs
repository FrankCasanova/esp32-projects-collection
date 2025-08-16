#![no_std]

pub mod audios;

// // Fill DMA buffer with a stereo square wave at a given frequency
// fn fill_square_wave(buffer: &mut [u8], freq_hz: u32, sample_rate: u32) {
//     let samples_per_cycle = sample_rate / freq_hz;
//     let half_cycle = samples_per_cycle / 2;
//     let frame_size = 4; // Stereo: 16-bit left + 16-bit right
//     let mut i = 0;

//     while i < buffer.len() / frame_size {
//         let sample_val: i16 = if (i % samples_per_cycle as usize) < half_cycle as usize {
//             0x7FFF  // max positive
//         } else {
//             0x8000u16 as i16  // max negative
//         };
//         // Left channel
//         buffer[i * frame_size + 0] = sample_val as u8;
//         buffer[i * frame_size + 1] = (sample_val >> 8) as u8;
//         // Right channel
//         buffer[i * frame_size + 2] = sample_val as u8;
//         buffer[i * frame_size + 3] = (sample_val >> 8) as u8;
//         i += 1;
//     }
// }

// // fn fill_from_wav(buffer: &mut [u8]) {
// //     const HEADER_SIZE: usize = 44; // Standard PCM header size
// //     let pcm_data = &WAV_DATA[HEADER_SIZE..];

// //     // Copy WAV data into DMA buffer (wrap if buffer bigger)
// //     for (i, byte) in buffer.iter_mut().enumerate() {
// //         *byte = pcm_data[i % pcm_data.len()];
// //     }
// // }

// fn fill_from_wav(buffer: &mut [u8]) {
//     const HEADER_SIZE: usize = 44; // Standard PCM header size for simple WAVs
//     // Ensure WAV_DATA is large enough to contain a header
//     if WAV_DATA.len() < HEADER_SIZE {
//         // Handle error or fill with silence
//         buffer.fill(0);
//         return;
//     }

//     // Get the raw sample data (assuming 16-bit stereo PCM)
//     let raw_pcm_data: &[u8] = &WAV_DATA[HEADER_SIZE..];

//     // Check if buffer and data lengths are compatible (ideally, buffer should be a multiple or vice-versa)
//     let data_len = raw_pcm_data.len();
//     if data_len == 0 {
//          buffer.fill(0);
//         return;
//     }

//     // Copy PCM data, wrapping if buffer is larger, truncating if data is larger
//     // This assumes raw_pcm_data contains interleaved L/R 16-bit samples as bytes
//     // e.g., [L0_low, L0_high, R0_low, R0_high, L1_low, L1_high, ...]
//     // And buffer expects the same format.
//     // for (i, byte) in buffer.iter_mut().enumerate() {
//     //     *byte = raw_pcm_data[i % data_len];
//     // }

//     // Optional: If the DMA buffer is significantly larger and you want true looping
//     // without byte-level wrapping artifacts, calculate how many full loops fit:
    
//     let full_copies = buffer.len() / data_len;
//     let remainder = buffer.len() % data_len;

//     for i in 0..full_copies {
//         let start_idx = i * data_len;
//         let end_idx = start_idx + data_len;
//         buffer[start_idx..end_idx].copy_from_slice(raw_pcm_data);
//     }
//     // Copy the remainder
//     buffer[full_copies * data_len..].copy_from_slice(&raw_pcm_data[..remainder]);
    
// }

// // Example for 16-bit Mono -> Stereo (simplified logic)
// fn fill_from_wav_mono_to_stereo(buffer: &mut [u8]) {
//     const HEADER_SIZE: usize = 44;
//     if WAV_DATA.len() < HEADER_SIZE {
//         buffer.fill(0);
//         return;
//     }

//     let raw_mono_data: &[u8] = &WAV_DATA[HEADER_SIZE..];
//     let mono_sample_count = raw_mono_data.len() / 2; // Each sample is 2 bytes

//     if mono_sample_count == 0 {
//         buffer.fill(0);
//         return;
//     }

//     let mut buffer_idx = 0;
//     let mut mono_idx = 0;
//     while buffer_idx < buffer.len().saturating_sub(3) && mono_idx < raw_mono_data.len().saturating_sub(1) {
//         let sample_byte_0 = raw_mono_data[mono_idx];
//         let sample_byte_1 = raw_mono_data[mono_idx + 1];

//         // Write sample to Left channel (L)
//         buffer[buffer_idx] = sample_byte_0;
//         buffer[buffer_idx + 1] = sample_byte_1;
//         // Write same sample to Right channel (R)
//         buffer[buffer_idx + 2] = sample_byte_0;
//         buffer[buffer_idx + 3] = sample_byte_1;

//         buffer_idx += 4; // Move to next stereo frame in buffer
//         mono_idx += 2;   // Move to next mono sample in data
//     }

//     // Fill remaining bytes with silence if any
//     while buffer_idx < buffer.len() {
//         buffer[buffer_idx] = 0;
//         buffer_idx += 1;
//     }
// }