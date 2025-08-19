#![no_std]
#![no_main]

use alloc::format;
use alloc::string::String;
use defmt::{error, info};

use embassy_executor::Spawner;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{DhcpConfig, Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};
use embedded_io_async::Read;
use esp_hal::clock::CpuClock;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use esp_wifi::wifi::{self, WifiController, WifiDevice, WifiEvent, WifiState};
use esp_wifi::EspWifiController;
use reqwless::client::{HttpClient, TlsConfig};
use reqwless::request::RequestBuilder;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

esp_bootloader_esp_idf::esp_app_desc!();
const SSID: &str = "sagemcomD440";
const PASSWORD: &str = "QMN2Q2YWUWMXEM";

/// Parse time string in "HH:MM:SS" format to minutes since midnight
fn parse_time_to_minutes(time_str: &str) -> Option<u32> {
    let mut parts = heapless::Vec::<&str, 3>::new();
    for part in time_str.split(':') {
        if parts.push(part).is_err() {
            return None;
        }
    }

    if parts.len() != 3 {
        return None;
    }

    let hours = parts[0].parse::<u32>().ok()?;
    let minutes = parts[1].parse::<u32>().ok()?;

    if hours > 23 || minutes > 59 {
        return None;
    }
    println!("Time parsed: {}", hours *60+minutes);

    Some(hours * 60 + minutes)
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::_80MHz);
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let timer0 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer0.timer0);
    info!("Embassy initialized!");

    let timer1 = TimerGroup::new(peripherals.TIMG0);
    let mut rng = Rng::new(peripherals.RNG);
    let esp_wifi_ctrl = &*mk_static!(
        EspWifiController<'static>,
        esp_wifi::init(timer1.timer0, rng).unwrap()
    );
    
    let (controller, interfaces) = esp_wifi::wifi::new(&esp_wifi_ctrl, peripherals.WIFI).unwrap();
    let wifi_interface = interfaces.sta;

    let net_seed = rng.random() as u64 | ((rng.random() as u64) << 32);
    let tls_seed = rng.random() as u64 | ((rng.random() as u64) << 32);

    let dhcp_config = DhcpConfig::default();
    let config = embassy_net::Config::dhcpv4(dhcp_config);
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        net_seed,
    );

    spawner.spawn(connection(controller)).ok();
    spawner.spawn(net_task(runner)).ok();
    wait_for_connection(stack).await;

    // Fetch time once at startup
    let initial_time = match access_website(stack, tls_seed).await {
        Some(time_str) => {
            parse_time_to_minutes(&time_str)
        },
        None => {
            println!("Failed to get initial time");
            None
        }
    };
    

    loop {
        // Calculate operational parameters based on initial time
        let (start_delay, operational_duration) = match initial_time {
            Some(minutes) => {
                if minutes < 570 {
                    // Before 9:30am
                    ((570 - minutes) * 60, 810) // Wait until 9:30, then run 13.5hrs
                } else if minutes <= 1380 {
                    // 9:30am-11:30pm
                    (0, 1380 - minutes) // Start immediately
                } else {
                    // After 11:30pm
                    ((1440 - minutes + 570) * 60, 810) // Wait until next day 9:30am
                }
            }
            None => (0, 810), // Default to full day if time unavailable
        };

        // Wait until start time
        if start_delay > 0 {
            println!("Delaying {} minutes until start time", start_delay / 60);
            Timer::after(Duration::from_secs(start_delay as u64)).await;
        }

        // Main operational loop
        let start_instant = embassy_time::Instant::now();
        let operational_seconds = operational_duration * 60;

        while embassy_time::Instant::now() - start_instant
            < Duration::from_secs(operational_seconds as u64)
        {
            println!("Within operational window");
            // ****************************************************
            // MAIN APPLICATION LOGIC GOES HERE
            // Run your sensors and other tasks here
            // ****************************************************

            Timer::after_secs(10).await;
        }

        println!("Operational window ended");
        // Go into low-power mode until next day
        Timer::after(Duration::from_secs(3600)).await; // Check hourly
    }
}

async fn wait_for_connection(stack: Stack<'_>) {
    println!("Waiting for link to be up");
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    println!("Waiting to get IP address...");
    loop {
        if let Some(config) = stack.config_v4() {
            println!("Got IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("start connection task");
    loop {
        match esp_wifi::wifi::wifi_state() {
            WifiState::StaConnected => {
                controller.wait_for_event(WifiEvent::StaDisconnected).await;
                Timer::after(Duration::from_millis(5000)).await
            }
            _ => {}
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = wifi::Configuration::Client(wifi::ClientConfiguration {
                ssid: SSID.try_into().unwrap(),
                password: PASSWORD.try_into().unwrap(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            println!("Starting wifi");
            controller.start_async().await.unwrap();
            println!("Wifi started!");
        }
        println!("About to connect...");

        match controller.connect_async().await {
            Ok(_) => println!("Wifi connected!"),
            Err(e) => {
                println!("Failed to connect to wifi: {:?}", e);
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}

async fn access_website(stack: Stack<'_>, tls_seed: u64) -> Option<heapless::String<32>> {
    let mut rx_buffer = [0; 16384];
    let mut tx_buffer = [0; 16384];
    let dns = DnsSocket::new(stack);
    let tcp_state = TcpClientState::<1, 8192, 8192>::new();
    let tcp = TcpClient::new(stack, &tcp_state);

    let tls = TlsConfig::new(
        tls_seed,
        &mut rx_buffer,
        &mut tx_buffer,
        reqwless::client::TlsVerify::None,
    );

    let mut client = HttpClient::new_with_tls(&tcp, &dns, tls);
    let mut buffer = [0u8; 8096];

    let mut http_req = match client
        .request(
            reqwless::request::Method::GET,
            "https://www.timeanddate.com",
        )
        .await
    {
        Ok(req) => req.headers(&[
            ("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36"),
            ("Connection", "close"),
        ]),
        Err(e) => {
            println!("Request error: {:?}", e);
            return None;
        }
    };

    let response = match http_req.send(&mut buffer).await {
        Ok(resp) => resp,
        Err(e) => {
            println!("Request error: {:?}", e);
            return None;
        }
    };

    let mut body_reader = response.body().reader();
    let mut result_string: Option<heapless::String<32>> = None;
    let deadline = embassy_time::Instant::now() + Duration::from_secs(30);

    while embassy_time::Instant::now() < deadline && result_string.is_none() {
        let mut chunk_buffer = [0u8; 512];
        match body_reader.read(&mut chunk_buffer).await {
            Ok(0) => break,
            Ok(len) => match core::str::from_utf8(&chunk_buffer[..len]) {
                Ok(html_text) => {
                    if let Some(title) = extract_by_id(&html_text, "clk_hm") {
                        if let Some(secs) = extract_by_id(&html_text, "ij0") {
                            let mut result = heapless::String::<32>::new();
                            let _ = result.push_str(&title);
                            let _ = result.push_str(":");
                            let _ = result.push_str(&secs);
                            println!("Access Website hour: {result}");
                            result_string = Some(result);
                        }
                    }
                }
                _ => {}
            },
            _ => break,
        }
    }

    result_string
}

/// Extract content by id attribute
fn extract_by_id(html: &str, id_value: &str) -> Option<heapless::String<256>> {
    let id_attr = format!("id=\"{}\"", id_value);

    // Find element with this id
    let mut pos = 0;
    while let Some(start_idx) = html[pos..].find(&id_attr) {
        let actual_start = pos + start_idx;

        // Find the start of the tag (go backwards to '<')
        let tag_start = match html[..actual_start].rfind('<') {
            Some(pos) => pos,
            None => {
                pos = actual_start + 1;
                continue;
            }
        };

        // Find the end of the opening tag
        let tag_end = match html[actual_start..].find('>') {
            Some(pos) => pos,
            None => {
                pos = actual_start + 1;
                continue;
            }
        };

        let content_start = actual_start + tag_end + 1;

        // Check if this is a self-closing tag
        if html[actual_start..(actual_start + tag_end)].contains("/") {
            // Self-closing tag, no content to extract
            pos = actual_start + 1;
            continue;
        }

        // Find the closing tag
        // We need to determine the tag name first
        let tag_name_start = tag_start + 1;
        let tag_name_end = html[tag_name_start..actual_start]
            .find(' ')
            .unwrap_or(actual_start - tag_name_start);
        let tag_name = &html[tag_name_start..tag_name_start + tag_name_end];

        let close_tag = format!("</{}>", tag_name);
        if let Some(close_idx) = html[content_start..].find(&close_tag) {
            let content_end = content_start + close_idx;
            let content = &html[content_start..content_end];

            // Try to create heapless String
            if let Ok(result) = heapless::String::try_from(content) {
                return Some(result);
            }
        }

        // Move past this element and continue searching
        pos = actual_start + 1;
    }

    None
}
