use defmt::info;
use embassy_time::{Duration, TimeoutError, Timer, with_timeout};
use esp_hal::{
    Async,
    dma::{DmaBufError, DmaChannelFor, DmaRxBuf, DmaTxBuf},
    dma_buffers,
    gpio::{
        Input, InputConfig, InputPin, Level, Output, OutputConfig, OutputPin,
        interconnect::PeripheralOutput,
    },
    spi::{
        self,
        master::{AnySpi, Config, Instance, Spi, SpiDmaBus},
    },
    time::Rate,
};

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

const BUFFER_SIZE: usize = DISPLAY_WIDTH_BYTES.strict_mul(DISPLAY_HEIGHT as usize);

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
}

#[derive(Debug, thiserror::Error, defmt::Format)]
pub(super) enum CreateError {
    #[error("Failed to create direct memory access (DMA) receive channel buffer")]
    DmaReceiveBuffer(DmaBufError),
    #[error("Failed to create direct memory access (DMA) transmit channel buffer")]
    DmaTransmitBuffer(DmaBufError),
    #[error("Failed to create SPI bus")]
    SpiBus(#[from] spi::master::ConfigError),
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to send command")]
pub(super) struct SendCommandError(spi::Error);

#[derive(Debug, thiserror::Error)]
#[error("Failed to send data")]
pub(super) struct SendDataError(spi::Error);

#[derive(Debug, thiserror::Error)]
pub(super) enum SetRamAreaError {
    #[error("Failed to send command")]
    SendCommand(#[from] SendCommandError),
    #[error("Failed to send data")]
    SendData(#[from] SendDataError),
}

#[derive(Debug, thiserror::Error)]
pub(super) enum InitializeControllerError {
    #[error("Failed to send command")]
    SendCommand(#[from] SendCommandError),
    #[error("Failed to send data")]
    SendData(#[from] SendDataError),
    #[error("Timed out waiting for busy")]
    WaitForBusy(#[from] WaitForBusyTimeoutError),
    #[error("Failed to set RAM area")]
    SetRamArea(#[from] SetRamAreaError),
}

#[derive(Debug, thiserror::Error, defmt::Format)]
#[error("Timeout waiting for busy")]
pub(super) struct WaitForBusyTimeoutError(TimeoutError);

#[derive(Debug, thiserror::Error)]
pub(super) enum InitializationError {
    #[error("Failed to create e-ink display driver instance")]
    Create(#[from] CreateError),
    #[error("Failed to initialize e-ink display controller")]
    InitializeController(#[from] InitializeControllerError),
}

pub(super) struct EinkDisplay<'d> {
    spi: SpiDmaBus<'d, Async>,
    reset: Output<'d>,
    data_command: Output<'d>,
    busy: Input<'d>,
}

impl<'d> EinkDisplay<'d> {
    fn new(
        spi: impl Instance + 'd,
        serial_clock: impl PeripheralOutput<'d>,
        master_in_slave_out: impl PeripheralOutput<'d>,
        chip_select: impl PeripheralOutput<'d>,
        direct_memory_access_channel: impl DmaChannelFor<AnySpi<'d>>,
        reset: impl OutputPin + 'd,
        data_command: impl OutputPin + 'd,
        busy: impl InputPin + 'd,
    ) -> Result<Self, CreateError> {
        // DMA = Direct Memory Access
        let (receive_buffer, receive_descriptor, transmit_buffer, transmit_descriptors) =
            dma_buffers!(BUFFER_SIZE);
        let direct_memory_access_receive_buffer = DmaRxBuf::new(receive_descriptor, receive_buffer)
            .map_err(CreateError::DmaReceiveBuffer)?;
        let direct_memory_access_transmit_buffer =
            DmaTxBuf::new(transmit_descriptors, transmit_buffer)
                .map_err(CreateError::DmaTransmitBuffer)?;

        // Initialize SPI with custom pins
        let spi = Spi::new(
            spi,
            Config::default()
                .with_frequency(Rate::from_mhz(40))
                .with_mode(esp_hal::spi::Mode::_0)
                .with_read_bit_order(esp_hal::spi::BitOrder::MsbFirst), // .with_write_bit_order(esp_hal::spi::BitOrder::MsbFirst)
        )?
        .with_sck(serial_clock)
        .with_mosi(master_in_slave_out)
        // .with_miso(todo!("Not defined in XteinkX4 screen spec"))
        .with_cs(chip_select)
        .with_dma(direct_memory_access_channel)
        .with_buffers(
            direct_memory_access_receive_buffer,
            direct_memory_access_transmit_buffer,
        )
        .into_async();

        // Set up GPIO pins
        let reset = Output::new(reset, Level::Low, OutputConfig::default());
        let data_command = Output::new(data_command, Level::High, OutputConfig::default());
        let busy = Input::new(busy, InputConfig::default());

        Ok(Self {
            spi,
            reset,
            data_command,
            busy,
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

    async fn send_command(&mut self, command: Command) -> Result<(), SendCommandError> {
        info!("Sending command: {:?}", command);
        // Set into command mode
        self.data_command.set_low();
        self.spi
            .write_async(&[command as u8])
            .await
            .map_err(SendCommandError)?;
        info!("Command sent");
        Ok(())
    }

    async fn send_data(&mut self, data: &[u8]) -> Result<(), SendDataError> {
        info!("Sending data: {:?}", data);
        // Set into data mode
        self.data_command.set_high();
        self.spi.write_async(data).await.map_err(SendDataError)?;
        info!("Data sent");
        Ok(())
    }

    async fn wait_for_busy(&mut self) -> Result<(), WaitForBusyTimeoutError> {
        with_timeout(Duration::from_millis(10_000), self.busy.wait_for_low())
            .await
            .map_err(WaitForBusyTimeoutError)
    }

    async fn set_ram_area(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> Result<(), SetRamAreaError> {
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

    pub(super) async fn initialize_controller(&mut self) -> Result<(), InitializeControllerError> {
        info!("Initializing SSD1677 controller");

        // Soft reset
        self.send_command(Command::SoftReset).await?;
        self.wait_for_busy().await?;

        // Temperature sensor control (internal)
        const TEMPERATURE_SENSOR_INTERNAL: u8 = 0x80;
        self.send_command(Command::TemperatureSensorControl).await?;
        self.send_data(&[TEMPERATURE_SENSOR_INTERNAL]).await?;

        // Booster soft-start control (GDEQ0426T82 specific values)
        self.send_command(Command::BoosterSoftStart).await?;
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
        self.wait_for_busy().await?;

        info!("SSD1677 controller initialized");
        Ok(())
    }

    pub(super) async fn initialize(
        spi: impl Instance + 'd,
        serial_clock: impl PeripheralOutput<'d>,
        master_in_slave_out: impl PeripheralOutput<'d>,
        chip_select: impl PeripheralOutput<'d>,
        direct_memory_access_channel: impl DmaChannelFor<AnySpi<'d>>,
        reset: impl OutputPin + 'd,
        data_command: impl OutputPin + 'd,
        busy: impl InputPin + 'd,
    ) -> Result<Self, InitializationError> {
        info!("Initializing e-ink display driver");
        let mut this = Self::new(
            spi,
            serial_clock,
            master_in_slave_out,
            chip_select,
            direct_memory_access_channel,
            reset,
            data_command,
            busy,
        )?;

        this.reset().await;

        this.initialize_controller().await?;

        info!("E-ink display driver initialized");

        Ok(this)
    }
}
