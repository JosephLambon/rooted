#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_hal::{
    delay::Delay,
    clock::CpuClock,
    time::{Duration, Instant},
    rng::Rng,
    timer::{timg::TimerGroup},
    analog::adc::{Adc, AdcConfig, Attenuation}, 
    interrupt::software::SoftwareInterruptControl,
    ram,
};

use reqwless::client::HttpClient;
use reqwless::headers::ContentType;
use reqwless::request::{Method,RequestBuilder};

use esp_radio::wifi::{
    ControllerConfig,
    WifiController,
    Interface,
    sta::StationConfig,
    scan::ScanConfig,
};

use embassy_net::{
    StackResources,
    Runner,
    tcp::client::{TcpClient, TcpClientState},
    dns::DnsSocket,
};

use core::fmt::Write;

use embassy_time::Timer;

use embedded_hal::delay::DelayNs;

use esp_println::{println, logger::init_logger};

use log::{error, info};

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    esp_println::println!("PANIC: {}", info);
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

const SSID: &str = "VM2157716";
const PASSWORD: &str = "ysKdaa3ra2jzxxks";
// Local mock receiver (moisture-api-mock). Plain HTTP — ESP32 TLS too heavy for testing.
// Update IP if the laptop's DHCP lease changes (`ipconfig getifaddr en0`).
const RECEIVER_URL: &str = "http://192.168.0.52:8080/moisture";

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) -> ! {
    // generator version: 1.3.0
    // generator parameters: --chip esp32
    
    // Setup CPU clock & watchdog timer
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    info!("Loading peripherals...");
    let peripherals = esp_hal::init(config);
    
    // Initialise esp-println / log logger
    init_logger(log::LevelFilter::Info);
    
    // Configure Real-Time Operating System (RTOS)
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let software_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    // Start the task scheduler
    info!("Starting task scheduler...");
    esp_rtos::start(timg0.timer0, software_interrupt.software_interrupt0);

    // Heap MUST be initialised before Wi-Fi — esp-radio allocates buffers on init.
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 32 * 1024);

    // Configure wifi radio
    info!("Initialising wifi controller...");
    let sta_config = esp_radio::wifi::Config::Station(
        StationConfig::default()
            .with_ssid(SSID)
            .with_password(PASSWORD.into()),
    );
    // Start wifi radio
    info!("Starting Wi-Fi");
    let (mut controller, interfaces) = esp_radio::wifi::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(sta_config),
    )
    .unwrap();
    info!("Wi-Fi configured and started");

    // Configure this is a station instance, so esp32 connects to router
    let wifi_interface = interfaces.station;
    // Configure how esp32 gets its address on the network.
    // DHCP is the system where router hands out IP addresses automatically
    let config = embassy_net::Config::dhcpv4(Default::default());

    // Rng produces u32. Need u64, hence bit manipulation here
    /*  e.g:
        a  as u8        0000 1011
        b  as u8        0000 0110      <- widened: upper half is zeros
        b << 4          0110 0000      <- bits slide left, zeros fill in behind
        a | (b << 4)    0110 1011      <- OR merges them, no overlap
    */
    let rng = Rng::new();
    let net_seed = rng.random() as u64 | ((rng.random() as u64) << 32);
    
    // Start the network stack
    // stack - used by program to open connections & send data
    // runner - background task that does work of moving data in & out
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        config,
        mk_static!(StackResources<3>, StackResources::<3>::new()), // reserves memory required by stack
        net_seed,  // random starting no. for stack. Security purposes
    );

    info!("Scanning for access points");
    // Configure scanning. Return max 10 results
    let scan_config = ScanConfig::default().with_max(10);
    let result = controller.scan_async(&scan_config).await.unwrap();
    for ap in result {
        info!("{:?}", ap);
    }
    // start background task, keeps wifi link alive
    spawner.spawn(connection(controller).unwrap());
    // start background task, keeps runner alive
    spawner.spawn(net_task(runner).unwrap());

    // pauses until networks is ready & device supplied an IP address
    stack.wait_config_up().await;
    // Fetches IP address & prints
    if let Some(config) = stack.config_v4() {
        info!("Got IP: {}", config.address);
    }
    //  Setup TCP client, 1 connection at a time, send & receive buffers of 1500bytes each
    let tcp_client = TcpClient::new(
        stack,
        mk_static!(
            TcpClientState<1, 1500, 1500>,
            TcpClientState::<1, 1500, 1500>::new()
        ),
    );
    let dns_client = DnsSocket::new(stack);

    // Start timer
    info!("Starting timer...");
    let timer = Instant::now();
    info!("500ms delay start...\n");
    while timer.elapsed() < Duration::from_millis(500) {}
    info!("===PROGRAM START===");

    // ESP32 pin GPIO36 connects to AOUT pin of moisture sensor
    let mut adc1_config = AdcConfig::new();
    let mut pin36 = adc1_config.enable_pin(peripherals.GPIO32, Attenuation::_11dB);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);

    info!("adc1 configured.");
    let mut delay = Delay::new();
    info!("GPIO32 moisture readings:");
    loop {
        let mut http_client = HttpClient::new(&tcp_client, &dns_client);
        let mut rx_buf = [0u8; 4096];

        
        match nb::block!(adc1.read_oneshot(&mut pin36)) {
            Ok(moisture_level) => {
                let mut body: heapless::String<64> = heapless::String::new();
                write!(body, r#"{{"moisture":{}}}"#, moisture_level).unwrap();

                info!("level: {:?}", moisture_level);
                info!("Attempting request...");
                let mut request = http_client
                    .request(Method::POST, RECEIVER_URL)
                    .await.unwrap()
                    .body(body.as_bytes())
                    .content_type(ContentType::ApplicationJson);
                    
                let response = request.send(&mut rx_buf)
                    .await.unwrap();

                info!("Response code: {:?}", response.status);
                // Handle request success or error
                if !response.status.is_successful() {
                    error!("Request failed: {}", response.status.0);
                    continue;
                }
                info!("Success: {}", response.status.0)

            }
            Err(e) => {
                error!("{:?}", e);
                error!("Error reading moisture level");
            }
        };



        // PAUSE FOR 5s
        delay.delay_ms(2000_u32);
    }
}

#[unsafe(no_mangle)]
pub extern "Rust" fn _esp_println_timestamp() -> u64 {
    esp_hal::time::Instant::now()
        .duration_since_epoch()
        .as_millis()
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    loop {
        println!("Connecting to Wi-Fi...");

        match controller.connect_async().await {
            Ok(info) => {
                println!("Wi-Fi connected to {:?}", info);
                let info = controller.wait_for_disconnect_async().await.ok();
                println!("Disconnected: {:?}", info);
            }
            Err(err) => println!("Failed to connect to Wi-Fi: {:?}", err),
        }

        Timer::after(embassy_time::Duration::from_secs(5)).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await
}

