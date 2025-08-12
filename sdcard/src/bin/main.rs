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
use esp_println::println;
use esp_println::{self as _, print};

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
            year_since_1970: 0,
            zero_indexed_month: 0,
            zero_indexed_day: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
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

    // Setting up the SPI communication channel (like installing a factory conveyor belt)
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
    println!("[INIT] SPI communication channel configured");

    // Connecting the SD card storage machine to our SPI conveyor belt
    let sd_cs = Output::new(peripherals.GPIO7, Level::High, OutputConfig::default());
    let spi_dev = ExclusiveDevice::new(spi_bus, sd_cs, Delay).expect("[ERROR] bad spi_dev setting");
    // Build an SD Card interface out of an SPI device, a chip-select pin and the delay object
    let sdcard = SdCard::new(spi_dev, Delay);
    // Get the card size (this also triggers card initialisation because it's not been done yet)
    println!("Card size is {} bytes", sdcard.num_bytes().unwrap());
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
    let my_file = root_dir.open_file_in_dir("MY_FILE.TXT", embedded_sdmmc::Mode::ReadOnly).unwrap();
    // Print the contents of the file, assuming it's in ISO-8859-1 encoding
    while !my_file.is_eof() {
        let mut buffer = [0u8; 32];
        let num_read = my_file.read(&mut buffer).unwrap();
        for b in &buffer[0..num_read] {
            print!("{}", *b as char);
        }
    }

    // TODO: Spawn some tasks
    let _ = spawner;

    loop {
        Timer::after(Duration::from_secs(1)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0-rc.0/examples/src/bin
}
