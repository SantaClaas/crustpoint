pub(crate) use crate::eink_display::error::*;

use defmt::info;
use embassy_time::{Duration, Timer, with_timeout};
use embedded_hal_async::spi::SpiDevice;
use esp_hal::gpio::{Input, InputConfig, InputPin, Level, Output, OutputConfig, OutputPin};
mod error;

#[derive(Debug, defmt::Format)]
#[repr(u8)]
enum Command {
    // Initialization and reset
    SoftReset = 0x12,
    TemperatureSensorControl = 0x18,
    BoosterSoftStart = 0x0C,
    DriverOutputControl = 0x01,
    BorderWaveformControl = 0x3C,

    // RAM and buffer management
    DataEntryMode = 0x11,
    SetRamXRange = 0x44,
    SetRamYRange = 0x45,
    SetRamXCounter = 0x4E,
    SetRamYCounter = 0x4F,
    AutoWriteBwRam = 0x46,
    AutoWriteRedRam = 0x47,
    WriteBwRam = 0x24,
    WriteRedRam = 0x26,

    // Display update and refresh
    DisplayUpdateControl1 = 0x21,
    DisplayUpdateControl2 = 0x22,
    MasterActivation = 0x20,

    // LUT and voltage settings
    /// Write temperature
    WriteTemperature = 0x1A,

    // Power management
    DeepSleep = 0x10,
}

#[derive(Debug, defmt::Format)]
#[repr(u8)]
enum ControlMode {
    /// Normal mode - compare RED vs BW for partial
    Normal = 0x00,
    /// Bypass RED RAM (treat as 0) - for full refresh
    BypassRed = 0x40,
}

pub(super) struct EinkDisplay<'d, SPI: SpiDevice> {
    spi: SPI,
    reset: Output<'d>,
    /// Based on usage this pin is used to select between data and command mode.
    /// When set to low, the pin is in command mode to send commands.
    /// When set to high, the pin is in data mode to send data.
    data_command: Output<'d>,
    busy: Input<'d>,
    is_screen_on: bool,
    is_custom_lut_active: bool,
}

pub(super) enum RefreshMode {
    Fast,
    Full,
    HalfRefresh,
}

const DISPLAY_WIDTH: u16 = 800;
const DISPLAY_HEIGHT: u16 = 480;
const DISPLAY_WIDTH_BYTES: usize = {
    // There is no div_exact yet
    assert!(
        DISPLAY_WIDTH % 8 == 0,
        "Display width must be a multiple of 8"
    );

    DISPLAY_WIDTH.strict_div(8) as usize
};

pub(crate) const BUFFER_SIZE: usize = DISPLAY_WIDTH_BYTES.strict_mul(DISPLAY_HEIGHT as usize);

impl<'d, SPI: SpiDevice> EinkDisplay<'d, SPI> {
    fn new(
        spi: SPI,
        reset: impl OutputPin + 'd,
        data_command: impl OutputPin + 'd,
        busy: impl InputPin + 'd,
    ) -> Result<Self, CreateError> {
        // Set up GPIO pins
        let reset = Output::new(reset, Level::Low, OutputConfig::default());
        let data_command = Output::new(data_command, Level::High, OutputConfig::default());
        let busy = Input::new(
            busy,
            InputConfig::default().with_pull(esp_hal::gpio::Pull::Down),
        );

        Ok(Self {
            spi,
            reset,
            data_command,
            busy,
            is_screen_on: false,
            is_custom_lut_active: false,
        })
    }

    async fn reset(&mut self) {
        info!("Resetting display");
        self.reset.set_high();
        // Might need to be blocking if it needs to be the exact time
        Timer::after_millis(20).await;
        self.reset.set_low();
        Timer::after_millis(2).await;
        self.reset.set_high();
        Timer::after_millis(20).await;
        info!("Display reset completed");
    }

    async fn send_command(&mut self, command: Command) -> Result<(), SendCommandError<SPI::Error>> {
        info!("Sending command: {:?}", command);
        // Set into command mode
        self.data_command.set_low();
        self.spi.write(&[command as u8]).await?;
        info!("Command sent");
        Ok(())
    }

    async fn send_data(&mut self, data: &[u8]) -> Result<(), SendDataError<SPI::Error>> {
        info!("Sending data: {:?}", data.as_ref().len());
        // Set into data mode
        self.data_command.set_high();
        self.spi.write(data).await?;
        info!("Data sent");
        Ok(())
    }

    async fn wait_for_idle(&mut self) -> Result<(), WaitForBusyTimeoutError> {
        with_timeout(Duration::from_secs(10), self.busy.wait_for_low())
            .await
            .map_err(WaitForBusyTimeoutError)
    }

    async fn set_ram_area(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> Result<(), SetRamAreaError<SPI::Error>> {
        // Data entry x increment y decrement???
        const DATA_ENTRY_X_INC_Y_DEC: u8 = 0x01;

        //TODO overflow safety
        // Reverse Y coordinate (gates are reversed on this display)
        let y = DISPLAY_HEIGHT - y - height;

        self.send_command(Command::DataEntryMode).await?;
        self.send_data(&[DATA_ENTRY_X_INC_Y_DEC]).await?;

        // Set RAM X address range (start, end) - X is in PIXELS
        self.send_command(Command::SetRamXRange).await?;
        //TODO safe arithmetic and casting
        // Start low byte
        self.send_data(&[(x % 256) as u8]).await?;
        // Start high byte
        self.send_data(&[(x / 256) as u8]).await?;
        // End low byte
        self.send_data(&[((x + width - 1) % 256) as u8]).await?;
        // End high byte
        self.send_data(&[((x + width - 1) / 256) as u8]).await?;

        // Set RAM Y address range (start, end) - Y is in PIXELS
        self.send_command(Command::SetRamYRange).await?;
        // Start low byte
        self.send_data(&[((y + height - 1) % 256) as u8]).await?;
        // Start high byte
        self.send_data(&[((y + height - 1) / 256) as u8]).await?;
        // End low byte
        self.send_data(&[(y % 256) as u8]).await?;
        // End high byte
        self.send_data(&[(y / 256) as u8]).await?;

        // Set RAM X address counter - X is in PIXELS
        self.send_command(Command::SetRamXCounter).await?;
        // Low byte
        self.send_data(&[(x % 256) as u8]).await?;
        // High byte
        self.send_data(&[(x / 256) as u8]).await?;

        // Set RAM Y address counter - Y is in PIXELS
        self.send_command(Command::SetRamYCounter).await?;
        // Low byte
        self.send_data(&[((y + height - 1) % 256) as u8]).await?;
        // High byte
        self.send_data(&[((y + height - 1) / 256) as u8]).await?;
        Ok(())
    }

    async fn initialize_controller(&mut self) -> Result<(), InitializeControllerError<SPI::Error>> {
        info!("Initializing SSD1677 controller");

        // Soft reset
        self.send_command(Command::SoftReset).await?;
        self.wait_for_idle().await?;

        // Temperature sensor control (internal)
        const TEMPERATURE_SENSOR_INTERNAL: u8 = 0x80;
        self.send_command(Command::TemperatureSensorControl).await?;
        self.send_data(&[TEMPERATURE_SENSOR_INTERNAL]).await?;

        // Booster soft-start control (GDEQ0426T82 specific values)
        self.send_command(Command::BoosterSoftStart).await?;
        //TODO combine to one slice
        self.send_data(&[0xAE]).await?;
        self.send_data(&[0xC7]).await?;
        self.send_data(&[0xC3]).await?;
        self.send_data(&[0xC0]).await?;
        self.send_data(&[0xC0]).await?;
        self.send_data(&[0x40]).await?;

        // Driver output control: set display height (480) and scan direction
        self.send_command(Command::DriverOutputControl).await?;
        //TODO safer casting
        self.send_data(&[((DISPLAY_HEIGHT - 1) % 256) as u8])
            .await?;
        self.send_data(&[((DISPLAY_HEIGHT - 1) / 256) as u8])
            .await?;
        self.send_data(&[0x02]).await?;

        // Border waveform control
        self.send_command(Command::BorderWaveformControl).await?;
        self.send_data(&[0x01]).await?;

        // Set up full screen RAM area
        self.set_ram_area(0, 0, DISPLAY_WIDTH, DISPLAY_HEIGHT)
            .await?;

        info!("Clearing RAM buffers");
        // Auto write BW RAM
        self.send_command(Command::AutoWriteBwRam).await?;
        self.send_data(&[0xF7]).await?;
        self.wait_for_idle().await?;

        // Auto write Red RAM
        self.send_command(Command::AutoWriteRedRam).await?;
        self.send_data(&[0xF7]).await?;
        self.wait_for_idle().await?;

        info!("SSD1677 controller initialized");
        Ok(())
    }

    pub(super) async fn initialize(
        spi: SPI,
        reset: impl OutputPin + 'd,
        data_command: impl OutputPin + 'd,
        busy: impl InputPin + 'd,
    ) -> Result<Self, InitializationError<SPI::Error>> {
        info!("Initializing e-ink display driver");
        let mut this = Self::new(spi, reset, data_command, busy)?;

        this.reset().await;

        this.initialize_controller().await?;

        info!("E-ink display driver initialized");

        Ok(this)
    }

    async fn refresh(
        &mut self,
        mode: RefreshMode,
        turn_screen_off: bool,
    ) -> Result<(), RefreshError<SPI::Error>> {
        // Configure Display Update Control 1
        self.send_command(Command::DisplayUpdateControl1).await?;
        // Configure buffer comparison mode
        self.send_data(&[
            match mode {
                RefreshMode::Fast => ControlMode::Normal,
                RefreshMode::Full | RefreshMode::HalfRefresh => ControlMode::BypassRed,
            } as u8,
            0x00,
        ])
        .await?;

        // (From crosspoint/open xteink community sdk)
        // best guess at display mode bits:
        // bit | hex | name                    | effect
        // ----+-----+--------------------------+-------------------------------------------
        // 7   | 80  | CLOCK_ON                | Start internal oscillator
        // 6   | 40  | ANALOG_ON               | Enable analog power rails (VGH/VGL drivers)
        // 5   | 20  | TEMP_LOAD               | Load temperature (internal or I2C)
        // 4   | 10  | LUT_LOAD                | Load waveform LUT
        // 3   | 08  | MODE_SELECT             | Mode 1/2
        // 2   | 04  | DISPLAY_START           | Run display
        // 1   | 02  | ANALOG_OFF_PHASE        | Shutdown step 1 (undocumented)
        // 0   | 01  | CLOCK_OFF               | Disable internal oscillato

        // Select appropriate display mode based on refresh type
        // let mut display_mode = 0b0000_0000u8;
        let mut display_mode = 0x00;

        if !self.is_screen_on {
            info!("Turning screen on");
            // Set CLOCK_ON and ANALOG_ON bits
            self.is_screen_on = true;
            // display_mode |= 0b1100_0000
            display_mode |= 0xC0;
        }

        if turn_screen_off {
            info!("Turning screen off");
            self.is_screen_on = false;
            // Set ANALOG_OFF_PHASE and CLOCK_OFF bits
            // 0x03;
            display_mode |= 0b000_00011;
        }

        match mode {
            RefreshMode::Fast => {
                display_mode |= if self.is_custom_lut_active {
                    // 0x0C
                    0b0000_1100
                } else {
                    // 0x1C
                    0b0001_1100
                };
            }
            RefreshMode::Full => {
                // 0x34;
                display_mode |= 0b0011_0100;
            }
            RefreshMode::HalfRefresh => {
                // Write high temp to the register for a faster refresh
                self.send_command(Command::WriteTemperature).await?;
                self.send_data(&[0x5A]).await?;
                display_mode |= 0b1101_0100;
            }
        }

        // Power on and refresh display
        self.send_command(Command::DisplayUpdateControl2).await?;
        self.send_data(&[display_mode]).await?;

        info!("Is busy? {}", self.busy.level());
        self.send_command(Command::MasterActivation).await?;

        // Wait for display to finish updating
        self.wait_for_idle().await?;

        Ok(())
    }

    pub(crate) async fn display(
        &mut self,
        mut refresh_mode: RefreshMode,
        frame_buffer: &[u8],
    ) -> Result<(), DisplayError<SPI::Error>> {
        if !self.is_screen_on {
            // Force half refresh if screen is off
            refresh_mode = RefreshMode::HalfRefresh;
        }

        // Set up full screen RAM area
        self.set_ram_area(0, 0, DISPLAY_WIDTH, DISPLAY_HEIGHT)
            .await?;

        match refresh_mode {
            RefreshMode::Fast => {
                // For fast refresh, write to BW buffer only
                self.send_command(Command::WriteBwRam).await?;
                self.data_command.set_high();

                self.send_data(frame_buffer).await?;
            }
            RefreshMode::HalfRefresh | RefreshMode::Full => {
                // For full refresh, write to both buffers before refresh
                self.send_command(Command::WriteBwRam).await?;
                self.send_data(frame_buffer).await?;

                self.send_command(Command::WriteRedRam).await?;
                self.send_data(frame_buffer).await?;
            }
        }

        self.refresh(refresh_mode, false).await?;

        Ok(())
    }

    pub(crate) async fn enter_deep_sleep(&mut self) -> Result<(), EnterDeepSleepError<SPI::Error>> {
        info!("Preparing display to enter deep sleep");
        // First, power down the display properly
        // This shuts down the analog power rails and clock
        if self.is_screen_on {
            self.send_command(Command::DisplayUpdateControl1).await?;
            self.send_data(&[ControlMode::BypassRed as u8]).await?;

            self.send_command(Command::DisplayUpdateControl2).await?;
            // Set ANALOG_OFF_PHASE (bit 1) and CLOCK_OFF (bit 0)
            // 0x03
            self.send_data(&[0b0000_0011]).await?;

            // Wait for the power-down sequence to complete
            self.wait_for_idle().await?;

            self.is_screen_on = false;
        }

        info!("Entering deep sleep");
        // Now enter deep sleep mode
        self.send_command(Command::DeepSleep).await?;
        // Enter deep sleep
        self.send_data(&[0x01]).await?;
        Ok(())
    }
}
