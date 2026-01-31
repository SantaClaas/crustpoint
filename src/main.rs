#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

mod eink_display;
mod input;
mod spi;

use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_hal::analog::adc::AdcChannel;
use esp_hal::gpio::{self, Input, InputConfig};
use esp_hal::peripherals::{ADC2, GPIO0, GPIO3, LPWR};
use esp_hal::rtc_cntl::sleep::{RtcioWakeupSource, WakeupLevel};
use esp_hal::rtc_cntl::{reset_reason, wakeup_cause};
use esp_hal::system::Cpu;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{clock::CpuClock, rtc_cntl::Rtc};
use {esp_backtrace as _, esp_println as _};

use crate::eink_display::EinkDisplay;
use crate::input::Analog;

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[derive(Debug, thiserror::Error)]
enum ApplicationError {
    #[error("Error setting up SPI")]
    SetUpSpi(#[from] spi::SetUpError),
    #[error("Error setting up e-ink display")]
    SetUpEinkDisplay(
        eink_display::InitializationError<
            <spi::Device<'static> as embedded_hal_async::spi::ErrorType>::Error,
        >,
    ),
    #[error("Error displaying on e-ink display")]
    Display(
        eink_display::DisplayError<
            <spi::Device<'static> as embedded_hal_async::spi::ErrorType>::Error,
        >,
    ),
    #[error("Error spawning task")]
    Spawn(#[from] embassy_executor::SpawnError),
}

#[embassy_executor::task]
async fn handle_power_button(
    mut pin: GPIO3<'static>,
    lpwr: LPWR<'static>,
    mut eink_display: EinkDisplay<'static, spi::Device<'static>>,
) {
    loop {
        let borrowed = pin.reborrow();

        let mut power_button = Input::new(borrowed, InputConfig::default());
        // Low = pressed, High = released
        power_button.wait_for_low().await;

        info!("Power button pressed. Turning off");

        if let Err(error) = eink_display
            .display(
                eink_display::RefreshMode::Full,
                &[0x00; eink_display::BUFFER_SIZE],
            )
            .await
        {
            error!(
                "Failed to update display before entering deep sleep: {:?}",
                defmt::Debug2Format(&error)
            );
            continue;
        }

        let Err(error) = eink_display.enter_deep_sleep().await else {
            break;
        };

        error!(
            "Failed to enter deep sleep: {:?}",
            defmt::Debug2Format(&error)
        );
    }

    // Just to be safe and avoid bricking the device when we accidentally run the deep sleep after reboot
    Timer::after_secs(5).await;
    info!("Entering deep sleep");

    let wakeup_pins: &mut [(&mut dyn gpio::RtcPinWithResistors, WakeupLevel)] =
        &mut [(&mut pin, WakeupLevel::Low)];

    let rtcio = RtcioWakeupSource::new(wakeup_pins);

    // LPWR = Low Power Watchdog and Reset? Low Power Wrapper? LowPoWeR? Laser Power?
    let mut real_time_control = Rtc::new(lpwr);
    real_time_control.sleep_deep(&[&rtcio]);
}

/// Just a convenience replacement for main to be able to return errors
async fn run(spawner: Spawner) -> Result<(), ApplicationError> {
    let reset_reason = reset_reason(Cpu::ProCpu);
    let wake_reason = wakeup_cause();

    info!(
        "Reset reason: {:?}; Wakeup reason: {:?}",
        defmt::Debug2Format(&reset_reason),
        wake_reason
    );

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 66320);
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
    let master_in_slave_out = peripherals.GPIO7;
    // Display Chip Select (CS)
    let display_chip_select = peripherals.GPIO21;
    // Data/Command (DC)
    let data_command = peripherals.GPIO4;
    // Reset (RST)
    let reset = peripherals.GPIO5;
    // Busy
    let busy = peripherals.GPIO6;

    let mut analog = Analog::new(
        peripherals.ADC1,
        peripherals.GPIO0,
        peripherals.GPIO1,
        peripherals.GPIO2,
    );

    let direct_memory_access_channel = peripherals.DMA_CH0;
    let sd_card_chip_select = peripherals.GPIO12;

    let (display_spi, _sd_card_spi) = spi::set_up_devices(
        peripherals.SPI2,
        serial_clock,
        master_out_slave_in,
        master_in_slave_out,
        direct_memory_access_channel,
        display_chip_select,
        sd_card_chip_select,
    )?;

    info!("Initializing display");

    let mut display = EinkDisplay::initialize(display_spi, reset, data_command, busy)
        .await
        .map_err(ApplicationError::SetUpEinkDisplay)?;

    let mut frame = [0x00u8; eink_display::BUFFER_SIZE];
    frame[0..eink_display::BUFFER_SIZE / 2].fill(0x33);
    // frame[eink_display::BUFFER_SIZE / 2..].fill(0x00);

    display
        .display(eink_display::RefreshMode::Full, &frame)
        .await
        .map_err(ApplicationError::Display)?;

    spawner.spawn(handle_power_button(
        peripherals.GPIO3,
        peripherals.LPWR,
        display,
    ))?;

    loop {
        analog.poll().await;
        Timer::after_secs(1).await;
    }

    Ok(())
}

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) {
    let result = run(spawner).await;
    if let Err(error) = result {
        error!("Main failed: {:?}", defmt::Debug2Format(&error));
    }
    info!("Main completed");
}
