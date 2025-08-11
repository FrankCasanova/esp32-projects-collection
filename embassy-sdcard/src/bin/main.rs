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

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timer0 = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(timer0.alarm0);

    info!("Embassy initialized!");

    // Initialize I2S for audio output
    let dma_channel = peripherals.DMA_CH1;
    let (tx_buffer, tx_descriptors, _, _) = dma_buffers!(4096, 0);

    let i2s = I2s::new(
        peripherals.I2S0,
        Standard::Philips,
        DataFormat::Data16Channel16,
        Rate::from_hz(44100),
        dma_channel,
    );
    let mut i2s_tx = i2s
        .i2s_tx
        .with_bclk(peripherals.GPIO1)
        .with_ws(peripherals.GPIO2)
        .with_dout(peripherals.GPIO3)
        .build(tx_descriptors);

    // Initialize SPI for SD card
    let spi_config = Config::default()
        .with_frequency(Rate::from_khz(400))
        .with_mode(Mode::_0);
    let spi_bus = Spi::new(peripherals.SPI2, spi_config)
        .unwrap()
        .with_sck(peripherals.GPIO4)
        .with_mosi(peripherals.GPIO5)
        .with_miso(peripherals.GPIO8)
        .into_async();

    let chip_select = Output::new(peripherals.GPIO7, Level::High, OutputConfig::default());
    let spi_dev = ExclusiveDevice::new(spi_bus, chip_select, Delay).unwrap();

    let sdcard = SdCard::new(spi_dev, Delay);

    println!("Init SD card controller and retrieve card size...");
    println!("Card size is {} bytes", sdcard.num_bytes().unwrap());
    let volume_mgr = VolumeManager::new(sdcard, DummyTimesource::default());

    let volume0 = volume_mgr.open_volume(VolumeIdx(0)).unwrap();
    let root_dir = volume0.open_root_dir().unwrap();

    let my_file = root_dir
        .open_file_in_dir("a.wav", embedded_sdmmc::Mode::ReadOnly)
        .unwrap();

    // Read and validate WAV header
    let mut header = [0u8; 44];
    let mut bytes_read = 0;
    while bytes_read < 44 {
        let n = my_file.read(&mut header[bytes_read..]).unwrap();
        if n == 0 { break; }
        bytes_read += n;
    }
    
    // Verify WAV header format with debug info
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" || &header[12..16] != b"fmt " {
        println!("Invalid WAV header format. Header bytes:");
        println!("{:02X?}", header);
        println!("Expected positions:");
        println!("[0-3]: RIFF -> {:?}", &header[0..4]);
        println!("[8-11]: WAVE -> {:?}", &header[8..12]);
        println!("[12-15]: fmt  -> {:?}", &header[12..16]);
        loop {}
    }
    
    let sample_rate = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
    let bits_per_sample = u16::from_le_bytes([header[34], header[35]]);
    println!("WAV file info: {}Hz, {} bits/sample", sample_rate, bits_per_sample);
    
    if sample_rate != 44100 || bits_per_sample != 16 {
        println!("Unsupported WAV format - must be 16-bit 44.1kHz");
        loop {}
    }

    // Start DMA transfer
    let mut transfer = i2s_tx.write_dma_circular(tx_buffer).unwrap();

    // Stream audio data to I2S with debug
    let mut total_bytes = 0;
    while !my_file.is_eof() {
        let avail = transfer.available().unwrap();
        if avail > 0 {
            let mut chunk = [0u8; 4096];
            let read_size = avail.min(4096);
            let n = my_file.read(&mut chunk[..read_size]).unwrap();
            transfer.push(&chunk[..n]).unwrap();
            total_bytes += n;
            println!("Sent {} bytes (total: {})", n, total_bytes);
        }
    }
    println!("Finished streaming audio");
   

    // TODO: Spawn some tasks
    let _ = spawner;

    loop {
        Timer::after(Duration::from_secs(2)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
