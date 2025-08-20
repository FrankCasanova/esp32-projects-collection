#![no_std]
#![no_main]

use alloc::format;
use alloc::string::String;
use reqwless::response::Status;
use core::sync::atomic::{AtomicBool, Ordering, AtomicI8};
use defmt::{error, info};
use esp_hal::tsens::TemperatureSensor;
use esp_hal::Blocking;

use embassy_executor::Spawner;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::{DhcpConfig, Runner, Stack, StackResources};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer, WithTimeout};
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

static STATE: AtomicI8 = AtomicI8::new(1);
static OPERATIONAL: AtomicBool = AtomicBool::new(true);
static WIFI_ENABLED: AtomicBool = AtomicBool::new(true);
static CONTROLLER: Mutex<CriticalSectionRawMutex, Option<WifiController<'static>>> =
    Mutex::new(None);

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
    println!("Time parsed: {}", hours * 60 + minutes);

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
    {
        *(CONTROLLER.lock().await) = Some(controller);
    }
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

    spawner.spawn(connection()).ok();
    spawner.spawn(net_task(runner)).ok();
    STATE.store(1, Ordering::Relaxed);
    Timer::after(Duration::from_secs(5)).await;
    wait_for_connection(stack).await;
    

    // Fetch time once at startup
    let initial_time = match access_website(stack, tls_seed).await {
        Some(time_str) => parse_time_to_minutes(&time_str),
        None => {
            println!("Failed to get initial time");
            None
        }
    };
    // --- Transition to Operational/Non-Operational State ---
    // Determine the initial state based on the fetched time
    // let is_initially_operational = match initial_time {
    //     Some(minutes) => minutes >= 570 && minutes <= 1380, // 9:30 AM to 11:30 PM
    //     None => true, // Default to operational if time is unknown?
    // };

    // if is_initially_operational {
    //     STATE.store(0, Ordering::Relaxed); // Start in Operational state
    // } else {
    //     STATE.store(-1, Ordering::Relaxed); // Start in Non-Operational state
    //     // Give the connection task a chance to run and disconnect
    //     Timer::after(Duration::from_secs(1)).await;
    // }
    // // Note: Wi-Fi is now managed by the connection task based on the STATE.

        loop {
        // Determine the next operational window based on initial time
        let (start_delay, operational_duration) = match initial_time {
            Some(minutes) => {
                if minutes < 570 {
                    // Before 9:30am - Wait until 9:30
                    ((570 - minutes) * 60, 810) // Run until 11:30 PM (810 mins)
                } else if minutes <= 1380 {
                    // 9:30am-11:30pm - Start immediately
                    (0, 1380 - minutes) // Run until 11:30 PM
                } else {
                    // After 11:30pm - Wait until next day 9:30am
                    ((1440 - minutes + 570) * 60, 810) // Run until next 11:30 PM
                }
            }
            None => {
                // Default behavior if time is unknown - Assume start now, run for a standard duration or until next check
                // Or, you could wait for a fixed time like next 9:30 AM logic
                 println!("Time unknown, using default operational window.");
                 (0, 810) // Example: Run for 13.5 hours
                 // Alternatively, calculate delay to next 9:30 AM from now (requires knowing current time relative to midnight)
                 // This is trickier without a persistent clock. Assuming 0 delay and 810 duration for now.
            }
        };

        // --- Transition to Non-Operational State Before Waiting ---
        STATE.store(-1, Ordering::Relaxed);
        println!("Entering Non-Operational state. Wi-Fi should disconnect.");
        // Give the connection task a chance to execute disconnection
        Timer::after(Duration::from_secs(2)).await;

        // Wait until the start of the next operational window
        if start_delay > 0 {
            println!("Delaying {} seconds until operational window starts", start_delay);
            Timer::after(Duration::from_secs(start_delay as u64)).await;
        }

        // --- Transition to Operational State ---
        // STATE.store(0, Ordering::Relaxed);
        // println!("Entering Operational state. Wi-Fi should connect.");
        // Give the connection task a chance to connect
        // It might connect immediately if it was waiting, or after the next 5s cycle.
        Timer::after(Duration::from_secs(5)).await; // Allow time for connection attempt

        // --- Main Operational Loop ---
        let start_instant = embassy_time::Instant::now();
        let operational_seconds = operational_duration * 60;

        // Stay in Operational state loop
        while embassy_time::Instant::now() - start_instant
            < Duration::from_secs(operational_seconds as u64)
        {
            println!("Within operational window - Running main tasks...");
            // ****************************************************
            // MAIN APPLICATION LOGIC GOES HERE
            // Run your sensors and other tasks here
            // ****************************************************

            Timer::after_secs(10).await; // Example delay
        }

        println!("Operational window ended");
        // STATE will be set to -1 at the start of the next loop iteration
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
async fn connection() {
    println!("start connection task");
    let mut controller_guard = CONTROLLER.lock().await;
    let controller = controller_guard.as_mut().unwrap();

    while WIFI_ENABLED.load(Ordering::Relaxed) {
        let current_state = STATE.load(Ordering::Relaxed);

        match current_state {
            1 => {
                // Initial Setup State: Ensure connected
                match esp_wifi::wifi::wifi_state() {
                    WifiState::StaConnected => {
                        // Good, stay connected. Wait for potential disconnection or state change.
                         println!("Setup: Wi-Fi connected.");
                    }
                    _ => {
                        // Not connected, try to connect
                        if !matches!(controller.is_started(), Ok(true)) {
                            let client_config =
                                wifi::Configuration::Client(wifi::ClientConfiguration {
                                    ssid: SSID.try_into().unwrap(),
                                    password: PASSWORD.try_into().unwrap(),
                                    ..Default::default()
                                });
                            controller.set_configuration(&client_config).unwrap();
                            println!("Setup: Starting wifi");
                            if let Err(e) = controller.start_async().await {
                                 println!("Setup: Failed to start wifi: {:?}", e);
                                 // Maybe retry or wait longer?
                            } else {
                                println!("Setup: Wifi started!");
                            }
                        }
                        if !matches!(esp_wifi::wifi::wifi_state(), WifiState::StaConnected) {
                            println!("Setup: About to connect...");
                            match controller.connect_async().await {
                                Ok(_) => println!("Setup: Wifi connected!"),
                                Err(e) => println!("Setup: Failed to connect to wifi: {:?}", e),
                            }
                        }
                    }
                }
                // Check for state change frequently during setup?
                Timer::after(Duration::from_secs(2)).await;
            }
            0 => {
                // Operational State: Ensure connected
                match esp_wifi::wifi::wifi_state() {
                    WifiState::StaConnected => {
                        // Already connected, good. Wait for potential disconnection or state change.
                        println!("Operational: Wi-Fi connected.");
                        // Use a longer wait, but wake up if disconnected
                         match controller.wait_for_event(WifiEvent::StaDisconnected).with_timeout(Duration::from_secs(10)).await {
                             Ok(_) => println!("Operational: Detected disconnection event."),
                             Err(_) => println!("Operational: Timeout waiting for disconnect event, still connected."),
                         }
                         // Or just wait a bit: Timer::after(Duration::from_secs(5)).await;
                    }
                    _ => {
                        // Not connected, try to connect
                         println!("Operational: Not connected, attempting to connect...");
                        if !matches!(controller.is_started(), Ok(true)) {
                            let client_config =
                                wifi::Configuration::Client(wifi::ClientConfiguration {
                                    ssid: SSID.try_into().unwrap(),
                                    password: PASSWORD.try_into().unwrap(),
                                    ..Default::default()
                                });
                            controller.set_configuration(&client_config).unwrap();
                            println!("Operational: Starting wifi");
                            if let Err(e) = controller.start_async().await {
                                 println!("Operational: Failed to start wifi: {:?}", e);
                            } else {
                                println!("Operational: Wifi started!");
                            }
                        }
                        // Attempt connection
                        match controller.connect_async().await {
                            Ok(_) => println!("Operational: Wifi connected!"),
                            Err(e) => println!("Operational: Failed to connect to wifi: {:?}", e),
                        }
                        // Wait a bit before checking again
                        Timer::after(Duration::from_secs(5)).await;
                    }
                }
            }
            -1 => {
                // Not Operational State: Ensure disconnected
                match esp_wifi::wifi::wifi_state() {
                    WifiState::StaConnected => {
                        println!("Non-Operational: Disconnecting WiFi for energy savings");
                        // Attempt to disconnect and stop
                        if let Err(e) = controller.disconnect_async().await {
                            println!("Non-Operational: Disconnect error: {:?}", e);
                        }
                        println!("Non-Operational: Stoping WiFi");
                        if let Err(e) = controller.stop_async().await {
                            println!("Non-Operational: Stoping error: {:?}", e);
                        }
                        WIFI_ENABLED.store(false, Ordering::Relaxed);
                        
                        // Stopping the controller might be necessary, but check docs.
                        // controller.stop_async().await might be needed or sufficient.
                        // Let's try disconnect first. Stop might be implicit or needed after.
                        // A delay might be needed after disconnect before checking state or stopping.
                        Timer::after(Duration::from_secs(1)).await; // Give time for disconnect
                         if matches!(esp_wifi::wifi::wifi_state(), WifiState::StaConnected) {
                              println!("Non-Operational: Still connected after disconnect attempt, trying stop.");
                              // If disconnect didn't work, try stop
                               if let Err(e) = controller.stop_async().await {
                                    println!("Non-Operational: Stop error: {:?}", e);
                               }
                               Timer::after(Duration::from_secs(1)).await; // Give time for stop
                         }
                         break;
                    }
                    _ => {
                        // Already disconnected or in the process. Ensure stopped?
                        // Potentially call stop if not started?
                        if matches!(controller.is_started(), Ok(true)) {
                             println!("Non-Operational: Wi-Fi started but not connected, stopping.");
                             if let Err(e) = controller.stop_async().await {
                                println!("Non-Operational: Stop error (while not connected): {:?}", e);
                             }
                        }
                        //println!("Non-Operational: Wi-Fi already disconnected.");
                        // Wait before checking state again
                        Timer::after(Duration::from_secs(5)).await;
                    }
                }
            }
            _ => {
                // Unknown state, treat as non-operational?
                println!("Unknown STATE value: {}, treating as Non-Operational", current_state);
                STATE.store(-1, Ordering::Relaxed); // Correct state?
                 Timer::after(Duration::from_secs(5)).await;
            }
        }
        // The waits inside the match arms control the loop timing now
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}

async fn access_website(stack: Stack<'_>, tls_seed: u64) -> Option<heapless::String<32>> {
    const MAX_RETRIES: u32 = 10; // Adjust the number of retries as needed
    const RETRY_DELAY_MS: u64 = 10_000; // 10 seconds in milliseconds

    let dns = DnsSocket::new(stack);
    let tcp_state = TcpClientState::<1, 8192, 8192>::new();
    let tcp = TcpClient::new(stack, &tcp_state);

    let mut rx_buffer = [0; 16384]; // These buffers need to be available for the duration
    let mut tx_buffer = [0; 16384]; // of the retry loop for TLS.

    let mut attempt = 0;
    let mut last_status: Option<Status> = None;

    loop {
        attempt += 1;
        println!("HTTP Request attempt {}", attempt);

        // Re-initialize TLS buffers for each attempt if necessary,
        // though often reusing them is fine depending on reqwless internals.
        // For safety with potential retries and buffer states, re-init might be preferred.
        // However, let's try reusing first. If issues arise, move buffer creation inside the loop.
        let tls = TlsConfig::new(
            tls_seed, // Consider if tls_seed should change per attempt (probably not strictly necessary for retry)
            &mut rx_buffer,
            &mut tx_buffer,
            reqwless::client::TlsVerify::None,
        );

        // Create a new client for each attempt. This ensures clean state.
        let mut client = HttpClient::new_with_tls(&tcp, &dns, tls);
        let mut buffer = [0u8; 8096]; // Buffer for the response headers/body

        // Build the request
        let mut http_req = match client
            .request(
                reqwless::request::Method::GET,
                "https://www.timeanddate.com", // Removed extra spaces
            )
            .await
        {
            Ok(req) => req.headers(&[
                ("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36"),
                ("Connection", "close"),
            ]),
            Err(e) => {
                println!("Request build error on attempt {}: {:?}", attempt, e);
                // Decide whether to retry on request build errors
                if attempt >= MAX_RETRIES {
                    println!("Max retries reached after request build error.");
                     break; // Exit loop if max retries exceeded
                }
                println!("Retrying in {} ms...", RETRY_DELAY_MS);
                embassy_time::Timer::after(embassy_time::Duration::from_millis(RETRY_DELAY_MS)).await;
                continue; // Retry the loop
            }
        };

        // Send the request
        let response_result = http_req.send(&mut buffer).await; // Use the buffer here

        match response_result {
            Ok(response) => {
                let status = response.status;
                last_status = Some(status.into());
                println!("Attempt {}: Status {:?}", attempt, status);

                if status == Status::Ok { // Check for 200 OK
                    println!("Success! Received status 200.");

                    // --- Process the successful response body ---
                    let mut body_reader = response.body().reader();
                    let mut result_string: Option<heapless::String<32>> = None;
                    // Consider if you still need a deadline for reading the body of a successful response
                    // For now, let's keep it simple and try to read for a short while
                    let body_deadline = embassy_time::Instant::now() + Duration::from_secs(10);

                    while embassy_time::Instant::now() < body_deadline && result_string.is_none() {
                        let mut chunk_buffer = [0u8; 512];
                        match body_reader.read(&mut chunk_buffer).await {
                            Ok(0) => {
                                // End of stream
                                println!("End of response body reached.");
                                break;
                            },
                            Ok(len) => match core::str::from_utf8(&chunk_buffer[..len]) {
                                Ok(html_text) => {
                                    //println!("Received chunk: {}", &html_text[..len.min(100)]); // Debug print first 100 chars
                                    if let Some(title) = extract_by_id(&html_text, "clk_hm") {
                                        if let Some(secs) = extract_by_id(&html_text, "ij0") {
                                            let mut result = heapless::String::<32>::new();
                                            if result.push_str(&title).is_ok() &&
                                               result.push_str(":").is_ok() &&
                                               result.push_str(&secs).is_ok() {
                                                println!("Access Website hour: {result}");
                                                result_string = Some(result);
                                                // Break the body reading loop as we found what we need
                                                break;
                                            } else {
                                                 println!("Error constructing time string, buffer might be full.");
                                                 // Could break or continue reading if buffer was too small for this part
                                                 break; // Let's assume failure to construct means we're done trying
                                            }
                                        } else {
                                             //println!("'ij0' not found in this chunk.");
                                        }
                                    } else {
                                        //println!("'clk_hm' not found in this chunk.");
                                    }
                                }
                                Err(e) => {
                                    println!("Error decoding chunk as UTF-8: {:?}", e);
                                    // Might indicate binary data or encoding issue, break
                                    break;
                                }
                            },
                            Err(e) => {
                                println!("Error reading response body chunk: {:?}", e);
                                // Error reading, break
                                break;
                            }
                        }
                        // Optional small delay between chunk reads if needed, usually not
                        // Timer::after(Duration::from_millis(10)).await;
                    }

                    // Return the result, whether we found the data or not after status 200
                    // If result_string is still None here, it means we got 200 but couldn't parse the time.
                    // Depending on requirements, you might want to retry or return None.
                    // For now, returning the result (None if not found) seems appropriate after a 200.
                    return result_string; // Exit the function with the result

                } else {
                    // Status was not OK (e.g., 404, 500, etc.)
                    println!("Received non-OK status: {:?}", status);
                    // Consume the body to free up the connection/resources if necessary
                    // Although Connection: close is used, it's good practice.
                     let mut body_reader = response.body().reader();
                     let mut discard_buffer = [0u8; 1024];
                     while let Ok(n) = body_reader.read(&mut discard_buffer).await {
                         if n == 0 { break; } // EOF
                         //println!("Discarded {} bytes", n); // Optional debug
                     }

                    // Check retry limit
                    if attempt >= MAX_RETRIES {
                        println!("Max retries ({}) reached. Last status: {:?}", MAX_RETRIES, last_status);
                        break; // Exit loop
                    }
                }
            }
            Err(e) => {
                println!("Request send error on attempt {}: {:?}", attempt, e);
                last_status = None; // Indicate request couldn't even be sent
                // Check retry limit
                if attempt >= MAX_RETRIES {
                     println!("Max retries reached after send error.");
                     break; // Exit loop
                }
                // For connection errors, it might be useful to check Wi-Fi state or wait longer,
                // but for now, standard retry delay.
            }
        }

        // If we reach here, it means the attempt failed (non-200 status or error) and we are retrying.
        if attempt < MAX_RETRIES { // No need to print/delay after the last failed attempt
             println!("Retrying in {} ms...", RETRY_DELAY_MS);
             embassy_time::Timer::after(embassy_time::Duration::from_millis(RETRY_DELAY_MS)).await;
        }
    }

    // If the loop completes without returning, it means retries were exhausted or failed
    println!("Failed to get a successful response (status 200) after {} attempts. Last status: {:?}", MAX_RETRIES, last_status);
    None // Return None if unsuccessful
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
