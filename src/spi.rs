use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use esp_hal::{
    Async,
    dma::{DmaBufError, DmaChannelFor, DmaRxBuf, DmaTxBuf},
    dma_buffers,
    gpio::{
        Level, Output, OutputConfig, OutputPin,
        interconnect::{PeripheralInput, PeripheralOutput},
    },
    spi::master::{AnySpi, Config, ConfigError, Instance, Spi, SpiDmaBus},
    time::Rate,
};
use static_cell::StaticCell;

#[derive(Debug, thiserror::Error)]
pub(crate) enum SetUpError {
    #[error("Failed to create direct memory access (DMA) receive channel buffer")]
    DmaReceiveBuffer(DmaBufError),
    #[error("Failed to create direct memory access (DMA) transmit channel buffer")]
    DmaTransmitBuffer(DmaBufError),
    #[error("Failed to create SPI bus")]
    SpiBus(#[from] ConfigError),
}
pub(crate) type Device<'a> = SpiDevice<'a, NoopRawMutex, SpiDmaBus<'a, Async>, Output<'a>>;

pub(crate) fn set_up_devices(
    spi: impl Instance + 'static,
    serial_clock: impl PeripheralOutput<'static>,
    master_out_slave_in: impl PeripheralOutput<'static>,
    master_in_slave_out: impl PeripheralInput<'static>,
    direct_memory_access_channel: impl DmaChannelFor<AnySpi<'static>>,
    display_chip_select: impl OutputPin + 'static,
    sd_card_chip_select: impl OutputPin + 'static,
) -> Result<(Device<'static>, Device<'static>), SetUpError> {
    let configuration = Config::default()
        .with_frequency(Rate::from_mhz(40))
        .with_mode(esp_hal::spi::Mode::_0)
        .with_read_bit_order(esp_hal::spi::BitOrder::MsbFirst);

    // DMA = Direct Memory Access
    let (receive_buffer, receive_descriptor, transmit_buffer, transmit_descriptors) =
        dma_buffers!(32_000);
    let direct_memory_access_receive_buffer =
        DmaRxBuf::new(receive_descriptor, receive_buffer).map_err(SetUpError::DmaReceiveBuffer)?;
    let direct_memory_access_transmit_buffer = DmaTxBuf::new(transmit_descriptors, transmit_buffer)
        .map_err(SetUpError::DmaTransmitBuffer)?;

    // Not sure if the embassy wrapper for sharing calls duplicates work the esp_hal SPI is already doing. Hopefully it uses the DMA too but I think it should.
    let spi = Spi::new(spi, configuration)?
        .with_sck(serial_clock)
        .with_mosi(master_out_slave_in)
        .with_miso(master_in_slave_out)
        // Do not configure chip select (cs) pin as that is different depending on the device
        .with_dma(direct_memory_access_channel)
        .with_buffers(
            direct_memory_access_receive_buffer,
            direct_memory_access_transmit_buffer,
        )
        .into_async();

    // Choosing to share the bus using embassy_embedded_hal over embedded_hal_bus to allow for async operations
    // Set up SPI bus sharing between devices with embassy
    static SPI_BUS: StaticCell<Mutex<NoopRawMutex, SpiDmaBus<'static, Async>>> = StaticCell::new();
    let spi_bus = Mutex::new(spi);
    let spi_bus = SPI_BUS.init(spi_bus);

    let display_chip_select =
        Output::new(display_chip_select, Level::High, OutputConfig::default());

    let display_spi = SpiDevice::new(spi_bus, display_chip_select);

    let sd_card_chip_select =
        Output::new(sd_card_chip_select, Level::High, OutputConfig::default());
    let sd_card_spi = SpiDevice::new(spi_bus, sd_card_chip_select);

    Ok((display_spi, sd_card_spi))
}
