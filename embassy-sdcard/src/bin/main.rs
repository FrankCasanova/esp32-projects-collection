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
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::spi::master::{Config, Spi};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use esp_hal::timer::systimer::SystemTimer;
use esp_println::{self as _, print, println};

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
        .open_file_in_dir("song.wav", embedded_sdmmc::Mode::ReadOnly)
        .unwrap();

    while !my_file.is_eof() {
        let mut buffer = [0u8; 32];

        if let Ok(n) = my_file.read(&mut buffer) {
            for b in &buffer[..n] {
                print!("{}", *b as char);
            }
        }
    }
    my_file.flush().unwrap();
    let my_other_file =
        root_dir.open_file_in_dir("MY_DATA2.CSV", embedded_sdmmc::Mode::ReadWriteCreateOrAppend).unwrap();
    my_other_file.write(b"Timestamp,Signal,Value\n").unwrap();
    my_other_file.write(b"2025-01-01T00:00:00Z,TEMP,25.0\n").unwrap();
    my_other_file.write(b"2025-01-01T00:00:01Z,TEMP,25.1\n").unwrap();
    my_other_file.write(b"2025-01-01T00:00:02Z,TEMP,25.2\n").unwrap();
    my_other_file.write(b"2025-01-01T01:00:02Z,TEMP,24.2\n").unwrap();
    my_other_file.write(b"ok then... it not update...").unwrap();


    // Don't forget to flush the file so that the directory entry is updated
    my_other_file.flush().unwrap();

    // TODO: Spawn some tasks
    let _ = spawner;

    loop {
        Timer::after(Duration::from_secs(2)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
