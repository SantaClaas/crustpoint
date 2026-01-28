#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

mod eink_display;

use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use {esp_backtrace as _, esp_println as _};

use crate::eink_display::EinkDisplay;

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[embassy_executor::task]
async fn hello_world() {
    loop {
        info!("Hello world!");
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    // generator version: 1.2.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 66320);
    // COEX needs more RAM - so we've added some more
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timer_group_0 = TimerGroup::new(peripherals.TIMG0);
    let software_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timer_group_0.timer0, software_interrupt.software_interrupt0);

    info!("Embassy initialized!");

    // Set up epaper display
    // Custom pins for XteinkX4, not hardware SPI defaults
    // SPI Clock (SCLK = serial clock)
    let serial_clock = peripherals.GPIO8;
    // SPI Master Out Slave In (MOSI)
    let master_out_slave_in = peripherals.GPIO10;
    // Chip Select (CS)
    let chip_select = peripherals.GPIO21;
    // Data/Command (DC)
    let data_command = peripherals.GPIO4;
    // Reset (RST)
    let reset = peripherals.GPIO5;
    // Busy
    let busy = peripherals.GPIO6;

    let direct_memory_access_channel = peripherals.DMA_CH0;
    let mut display = EinkDisplay::initialize(
        peripherals.SPI2,
        serial_clock,
        master_out_slave_in,
        chip_select,
        direct_memory_access_channel,
        reset,
        data_command,
        busy,
    )
    .await
    .inspect_err(|_error| error!("Error initializing display"))
    .expect("Failed to initialize display");

    display
        .display(
            eink_display::RefreshMode::Full,
            &[0xFF; EinkDisplay::BUFFER_SIZE],
        )
        .await
        .inspect_err(|error| error!("Error displaying {:?}", defmt::Debug2Format(&error)))
        .expect("Failed to display");

    display
        .enter_deep_sleep()
        .await
        .inspect_err(|error| {
            error!(
                "Error starting deep sleep {:?}",
                defmt::Debug2Format(&error)
            )
        })
        .expect("Failed to start deep sleep");

    // Task test

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0/examples
    info!("COMPLETED");
}
