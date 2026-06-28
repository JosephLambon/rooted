#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_hal::{
    main,
    delay::Delay,
    clock::CpuClock,
    time::{Duration, Instant},
    timer::{timg::TimerGroup},
    analog::adc::{Adc, AdcConfig, Attenuation}, 
    interrupt::software::SoftwareInterruptControl,
    ram,
};


use embedded_hal::delay::DelayNs;

use esp_println::logger::init_logger;

use log::{error, info};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[main]
fn main() -> ! {
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

    // Initialise wifi radio
    info!("Initialising wifi controller...");
    if let Ok(controller) = esp_radio::wifi::WifiController::new(
        peripherals.WIFI,
        Default::default(),
    ) {}


    // Start timer
    info!("Starting timer...");
    let timer = Instant::now();
    info!("500ms delay start...\n");
    while timer.elapsed() < Duration::from_millis(500) {}
    info!("===PROGRAM START===");

    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 32 * 1024);


    // ESP32 pin GPIO36 connects to AOUT pin of moisture sensor
    let mut adc1_config = AdcConfig::new();
    let mut pin36 = adc1_config.enable_pin(peripherals.GPIO32, Attenuation::_11dB);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config);

    info!("adc1 configured.");
    let mut delay = Delay::new();
    info!("GPIO32 moisture readings:");
    loop {
        match nb::block!(adc1.read_oneshot(&mut pin36)) {
            Ok(moisture_level) => {
                info!("level: {:?}", moisture_level);
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
