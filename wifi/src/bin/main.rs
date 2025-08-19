#![no_std]
#![no_main]

// use alloc::vec::Vec;
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

// If you are okay with using a nightly compiler, you can use the macro provided by the static_cell crate: https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
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

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.3.1
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::_80MHz);
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 72 * 1024);

    let timer0 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer0.timer0);

    info!("Embassy initialized!");

    let timer1 = TimerGroup::new(peripherals.TIMG0);
    // let _init = esp_wifi::init(
    //     timer1.timer0,
    //     esp_hal::rng::Rng::new(peripherals.RNG),
    //     peripherals.RADIO_CLK,
    // )
    // .unwrap();
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
    // dhcp_config.hostname = Some(String::from_str("implRust").unwrap());

    let config = embassy_net::Config::dhcpv4(dhcp_config);
    // Init network stack
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        net_seed,
    );

    spawner.spawn(connection(controller)).ok();
    spawner.spawn(net_task(runner)).ok();

    wait_for_connection(stack).await;

    access_website(stack, tls_seed).await
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
    println!("Device capabilities: {:?}", controller.capabilities());
    loop {
        match esp_wifi::wifi::wifi_state() {
            WifiState::StaConnected => {
                // wait until we're no longer connected
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

async fn access_website(stack: Stack<'_>, tls_seed: u64)  {
    let mut rx_buffer = [0; 16384]; // Increased buffer size
    let mut tx_buffer = [0; 16384]; // Increased buffer size
    let dns = DnsSocket::new(stack);
    let tcp_state = TcpClientState::<1, 8192, 8192>::new(); // Increased buffer sizes
    let tcp = TcpClient::new(stack, &tcp_state);

    // Try different TLS configurations
    info!("Creating TLS config...");
    let tls = TlsConfig::new(
        tls_seed,
        &mut rx_buffer,
        &mut tx_buffer,
        reqwless::client::TlsVerify::None,
    );
    info!("TLS config created");

    let mut client = HttpClient::new_with_tls(&tcp, &dns, tls);
    
    // Try to connect with a simpler request first
    info!("Attempting connection with simpler request...");
    let mut buffer = [0u8; 8096];

    // Try to connect to the server
    info!("Attempting to connect to server...");
    let mut http_req = match client
        .request(
            reqwless::request::Method::GET,
            "https://www.timeanddate.com",
        )
        .await
    {
        Ok(req) => {
            info!("Request created successfully");
            req.headers(&[
                ("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36"),
                ("Connection", "close"),
            ])
        },
        Err(e) => {
            error!("Error creating request");
            println!("Detailed error: {e:?}");
            return;
        }
    };

    info!("Request prepared, sending...");

    // Send request
    let response = match http_req.send(&mut buffer).await {
        Ok(resp) => resp,
        Err(e) => {
            error!("Error sending request");
            println!("{e:?}");
            return;
        }
    };

    info!("Request made");
    println!("Status Code: {:?}", response.status);

    // Read the body in chunks
    let mut body_reader = response.body().reader();
    let mut chunk_count = 0;
    let deadline = embassy_time::Instant::now() + Duration::from_secs(30);

    while embassy_time::Instant::now() < deadline {
        let mut chunk_buffer = [0u8; 512];
        match body_reader.read(&mut chunk_buffer).await {
            Ok(0) => {
                info!("End of response");
                break;
            }
            Ok(len) => {
                chunk_count += 1;
                match core::str::from_utf8(&chunk_buffer[..len]) {
                    Ok(html_text) => {
                        // println!("--- Chunk {} ---", chunk_count);
                        // println!("{}", html_text);
                        if let Some(title) = extract_by_id(&html_text, "clk_hm") {
                            info!("Hour and minutes: {}", &title);
                            if let Some(secs) = extract_by_id(&html_text, "ij0") {
                                let mut result = heapless::String::<32>::new();
                                if let Err(_) = result.push_str(&title) {
                                    info!("Title too long for buffer");
                                }
                                if let Err(_) = result.push_str(":") {
                                    info!("Failed to add colon");
                                }
                                if let Err(_) = result.push_str(&secs) {
                                    info!("Seconds too long for buffer");
                                }
                                println!("{}", result);
                                // Here you could return/store the result
                            } else {
                                info!("Seconds not found for ID 'ij0'");
                            }
                        }
                        
                    }
                    Err(e) => {
                        info!("Received non-UTF8 data in chunk");
                        println!("{e:?}");
                    }
                }
            }
            Err(e) => {
                info!("Response bytes finished o Chunk error...");
                println!("{e:?}");
                break;
            }
        }
    }
    
}

fn extract_tag_content(html: &str, tag_name: &str) -> Option<heapless::String<256>> {
    let open_tag_start = format!("<{}", tag_name);
    let close_tag = format!("</{}>", tag_name);

    // Find opening tag
    let mut pos = 0;
    while let Some(start_idx) = html[pos..].find(&open_tag_start) {
        let actual_start = pos + start_idx;

        // Find the end of the opening tag (the '>' character)
        if let Some(tag_end_idx) = html[actual_start..].find('>') {
            let content_start = actual_start + tag_end_idx + 1;

            // Find closing tag
            if let Some(close_idx) = html[content_start..].find(&close_tag) {
                let content_end = content_start + close_idx;
                let content = &html[content_start..content_end];

                // Try to create heapless String
                if let Ok(result) = heapless::String::try_from(content) {
                    return Some(result);
                }
            }
        }

        // Move past this tag and continue searching
        pos = actual_start + 1;
    }

    None
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
        let tag_name_end = html[tag_name_start..actual_start].find(' ').unwrap_or(actual_start - tag_name_start);
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

/// Extract content by class attribute
fn extract_by_class(html: &str, class_value: &str) -> Option<heapless::String<256>> {
    let class_attr = format!("class=\"{}\"", class_value);
    let class_attr_space = format!("class=\"{} ", class_value);
    let class_attr_end = format!(" {}\"", class_value);
    
    // Find element with this class
    let mut pos = 0;
    while let Some(start_idx) = html[pos..].find(&class_attr) {
        let actual_start = pos + start_idx;
        
        // Check if this is the exact class or part of multiple classes
        let is_exact = {
            // Check if it's at the beginning of class attribute
            let before_class = &html[pos..actual_start];
            let after_class = &html[(actual_start + class_attr.len())..];
            
            // If it's at the start and either ends with " or is followed by a space, it's exact
            (before_class.ends_with("class=\"") && (after_class.starts_with("\"") || after_class.starts_with(" "))) ||
            // Or if it's in the middle/end and surrounded by spaces or quotes
            (!before_class.is_empty() && (before_class.ends_with(" ") || before_class.ends_with("\"")) && 
             (after_class.starts_with(" ") || after_class.starts_with("\"")))
        };
        
        // If it's not an exact match, continue searching
        if !is_exact {
            pos = actual_start + 1;
            continue;
        }

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
        let tag_name_end = html[tag_name_start..actual_start].find(' ').unwrap_or(actual_start - tag_name_start);
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

// For multiple matches:
fn extract_all_tag_content<const N: usize>(
    html: &str,
    tag_name: &str,
) -> heapless::Vec<heapless::String<256>, N> {
    let mut results: heapless::Vec<heapless::String<256>, N> = heapless::Vec::new();
    let open_tag_start = format!("<{}", tag_name);
    let close_tag = format!("</{}>", tag_name);

    let mut pos = 0;
    while let Some(start_idx) = html[pos..].find(&open_tag_start) {
        let actual_start = pos + start_idx;

        // Find the end of the opening tag
        if let Some(tag_end_idx) = html[actual_start..].find('>') {
            let content_start = actual_start + tag_end_idx + 1;

            // Find closing tag
            if let Some(close_idx) = html[content_start..].find(&close_tag) {
                let content_end = content_start + close_idx;
                let content = &html[content_start..content_end];

                // Try to create heapless String and push to results
                if let Ok(result) = heapless::String::try_from(content) {
                    if results.push(result).is_err() {
                        break; // Vector is full
                    }
                }
            }
        }

        // Move past current position
        pos = actual_start + 1;
    }

    results
}