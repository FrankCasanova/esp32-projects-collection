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
use esp_hal::dma::DmaTxBuf;
use esp_hal::dma_buffers;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::i2s::master::{DataFormat, I2s, Standard};
use esp_hal::spi::master::{Config, Spi};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use esp_println::println;
use esp_println::{self as _, print};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// extern crate alloc;

/// Code from https://github.com/rp-rs/rp-hal-boards/blob/main/boards/rp-pico/examples/pico_spi_sd_card.rs
/// A dummy timesource, which is mostly important for creating files.
#[derive(Default)]
pub struct DummyTimesource();

impl TimeSource for DummyTimesource {
    // In theory you could use the RTC of the rp2040 here, if you had
    // any external time synchronizing device.
    fn get_timestamp(&self) -> Timestamp {
        Timestamp {
            year_since_1970: 0,
            zero_indexed_month: 0,
            zero_indexed_day: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
        }
    }
}

/// Minimal WAV header info
pub struct WavInfo {
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub channels: u16,
    pub data_offset: usize,
    pub data_len: usize,
}

fn parse_wav_header(buf: &[u8]) -> Result<WavInfo, &'static str> {
    if buf.len() < 44 {
        return Err("header too small");
    }
    if &buf[0..4] != b"RIFF" {
        return Err("not RIFF");
    }
    if &buf[8..12] != b"WAVE" {
        return Err("not WAVE");
    }

    let mut idx = 12usize;
    let mut sample_rate = 0u32;
    let mut bits_per_sample = 0u16;
    let mut num_channels = 0u16;

    while idx + 8 <= buf.len() {
        let id = &buf[idx..idx + 4];
        let chunk_size =
            u32::from_le_bytes([buf[idx + 4], buf[idx + 5], buf[idx + 6], buf[idx + 7]]) as usize;
        idx += 8;

        if id == b"fmt " {
            if idx + chunk_size > buf.len() {
                return Err("fmt chunk truncated");
            }
            num_channels = u16::from_le_bytes([buf[idx + 2], buf[idx + 3]]);
            sample_rate =
                u32::from_le_bytes([buf[idx + 4], buf[idx + 5], buf[idx + 6], buf[idx + 7]]);
            bits_per_sample = u16::from_le_bytes([buf[idx + 14], buf[idx + 15]]);
        } else if id == b"data" {
            return Ok(WavInfo {
                sample_rate,
                bits_per_sample,
                channels: num_channels,
                data_offset: idx,
                data_len: chunk_size,
            });
        }

        // advance to next chunk (and respect pad byte)
        idx = idx.saturating_add(chunk_size);
        if chunk_size % 2 == 1 {
            idx += 1;
        }
    }
    Err("no data chunk")
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.5.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::_80MHz);
    let peripherals = esp_hal::init(config);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    // Setting up the SPI communication channel (like installing a factory conveyor belt)
    // Initialize SPI for SD card
    let spi_config = Config::default()
        .with_frequency(Rate::from_khz(400))
        .with_mode(Mode::_0);
    let spi_bus = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_miso(peripherals.GPIO4)
        .with_mosi(peripherals.GPIO5)
        .with_sck(peripherals.GPIO6)
        .into_async();
    println!("[INIT] SPI communication channel configured");

    // Connecting the SD card storage machine to our SPI conveyor belt
    let sd_cs = Output::new(peripherals.GPIO0, Level::High, OutputConfig::default());
    let spi_dev = ExclusiveDevice::new(spi_bus, sd_cs, Delay)
        .unwrap();
    info!("SPI_DEV");
    // Build an SD Card interface out of an SPI device, a chip-select pin and the delay object
    let sdcard = SdCard::new(spi_dev, Delay);
    info!("SDCARD");

    // Get the card size (this also triggers card initialisation because it's not been done yet)
    println!("Card size is {} bytes", sdcard.num_bytes().expect("critical error"));
    info!("SDCARD.NU_BYTES");

    // Now let's look for volumes (also known as partitions) on our block device.
    // To do this we need a Volume Manager. It will take ownership of the block device.
    let volume_mgr = VolumeManager::new(sdcard, DummyTimesource::default());
    // Try and access Volume 0 (i.e. the first partition).
    // The volume object holds information about the filesystem on that volume.
    let volume0 = volume_mgr.open_volume(VolumeIdx(0)).unwrap();
    println!("Volume 0: {:?}", volume0);
    // Open the root directory (mutably borrows from the volume).
    let root_dir = volume0.open_root_dir().unwrap();
    // Open a file called "MY_FILE.TXT" in the root directory
    // This mutably borrows the directory.
    let my_file = root_dir
        .open_file_in_dir("a.wav", embedded_sdmmc::Mode::ReadOnly)
        .unwrap();

    let mut header_buf = [0_u8; 512];
    let n = my_file.read(&mut header_buf).unwrap();
    if n == 0 {
        println!("Empty file?");
    }
    let wav_info = parse_wav_header(&header_buf).expect("wav parse failed");
    println!(
        "WAV: {} Hz, {}-bit, {} channels",
        wav_info.sample_rate, wav_info.bits_per_sample, wav_info.channels
    );
    my_file
        .seek_from_start(wav_info.data_offset as u32)
        .unwrap();

    // ------------------------------------------------------------------------------------------------------
    // ------------------------------------------------------------------------------------------------------
    // ------------------------------------------------------------------------------------------------------
    // ------------------------------------------------------------------------------------------------------
    // ------------------------------------------------------------------------------------------------------
    // ------------------------------------------------------------------------------------------------------

    // --------------------------------
    // I2S configuration (esp-hal style)
    // --------------------------------
    // Pins chosen to avoid your SPI pins (GPIO4/5/6 are used by SPI above).
    // We'll use: GPIO1=BCLK, GPIO2=LRCLK (WS), GPIO3=DOUT
    // If your board ties UART0 to those pins, just pick other free pins (esp32-c3 has matrix).
    let bclk_pin = peripherals.GPIO1;
    let ws_pin = peripherals.GPIO2;
    let dout_pin = peripherals.GPIO3;

    // Create DMA buffers (size: tune if underruns occur)
    // NOTE: the exact return-order of `dma_buffers!` macro differs between esp-hal versions;
    // the common pattern below matches many examples—if your compiler errors about tuple ordering,
    // swap the order as the compiler suggests.
    let (mut tx_buffer, mut tx_descriptors, _,  _) =
        dma_buffers!(8192usize, 0);
    // Configure DMA channel - use DMA channel 0 (common on examples)
    let dma_channel = peripherals.DMA_CH0; // if your version exposes this differently, adapt.
                                           // Build DmaTxBuf object from descriptors + buffer (API in esp-hal)
                                           // If constructor name differs in your version, the compiler will show the correct name.
                                           // let mut dma_tx = DmaTxBuf::new(tx_descriptors, tx_buffer).expect("DmaTxBuf::new");
                                           // Now construct the I2S peripheral.
                                           // The i2s::new signature in esp-hal typically looks like:
                                           // I2s::new(periph, Standard::Philips, DataFormat::Data16Channel16, Rate::from_hz(...), dma_channel.configure(...), &clocks)
                                           // The `&clocks` param may or may not be required in your release; the compiler will indicate it.
                                           // We'll attempt the common form below; if your version differs, the compiler error will indicate the missing argument(s).
    let i2s = I2s::new(
        peripherals.I2S0,
        Standard::Philips,
        DataFormat::Data16Channel16,
        Rate::from_hz(wav_info.sample_rate),
        dma_channel,
    );
    let i2s = i2s.with_mclk(peripherals.GPIO7);
    let mut i2s_tx = i2s
        .i2s_tx
        .with_bclk(bclk_pin)
        .with_ws(ws_pin)
        .with_dout(dout_pin)
        .build(tx_descriptors);

    // ------------------------
    // PCM streaming loop
    // ------------------------
    // read chunks from file, convert bytes -> i16 little-endian samples
    // Define a chunk size for reading. 4096 is a good value.
    const CHUNK_SIZE: usize = 4096;

    loop {
        // Read a chunk from the file directly into the DMA buffer.
        // We use a slice of tx_buffer to act as our temporary read buffer.
        let bytes_read = my_file.read(&mut tx_buffer[..CHUNK_SIZE]).unwrap();

        if bytes_read == 0 {
            // End of file
            break;
        }
        
        // Ensure we only process the bytes we actually read.
        let buffer_to_play = &tx_buffer[..bytes_read];

        // The I2S write function expects samples (i16), not bytes (u8).
        // This unsafe block is generally safe here because we know the source is 16-bit PCM
        // and have ensured `bytes_read` will be a multiple of 2.
        let samples: &[i16] = unsafe {
            core::slice::from_raw_parts(
                buffer_to_play.as_ptr() as *const i16,
                buffer_to_play.len() / 2, // Each sample is 2 bytes
            )
        };
        
        // Write the chunk of samples. This function will wait until there's space
        // in the DMA buffer, but because we're using smaller chunks, the waits
        // will be short and frequent, preventing underruns.
        i2s_tx.write_words(samples).unwrap();
    }

    my_file.close().unwrap();
    println!("Playback finished (end of file).");

    let _ = spawner;

    

    loop {
        // Timer::after(Duration::from_secs(1)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/b
}