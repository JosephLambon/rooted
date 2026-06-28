#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::analog::adc;
use esp_hal::gpio::{Input, InputConfig};
use esp_hal::time::{Duration, Instant};
use esp_hal::delay::Delay;

use embedded_hal::delay::DelayNs;

use esp_println::logger::init_logger;

use log::{info};

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

    // Initialise esp-println / log logger
    init_logger(log::LevelFilter::Info);

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // ESP32 pin GPIO36 connects to AOUT pin of moisture sensor
    let mut adc1

    let pin36 = Input::new(peripherals.GPIO36, InputConfig::default());

    loop {
        let timer = Instant::now();
        info!("===PROGRAM START===");
        info!("500ms delay start...");
        while timer.elapsed() < Duration::from_millis(500) { }   
        
        let mut delay = Delay::new();
        
        loop {
            let analog = peripherals.SENS.split();

            let moisture_level = pin36.level();
            info!("level: {:?}", moisture_level);

            // PAUSE FOR 5s
            delay.delay_ms(5000 as u32);
        }        
    }
    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.1.0/examples
}

#[unsafe(no_mangle)]
pub extern "Rust" fn _esp_println_timestamp() -> u64 {
    esp_hal::time::Instant::now()
        .duration_since_epoch()
        .as_millis()
}