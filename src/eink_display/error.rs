use embassy_time::TimeoutError;
use esp_hal::{dma::DmaBufError, spi};

#[derive(Debug, thiserror::Error, defmt::Format)]
pub(crate) enum CreateError {
    #[error("Failed to create direct memory access (DMA) receive channel buffer")]
    DmaReceiveBuffer(DmaBufError),
    #[error("Failed to create direct memory access (DMA) transmit channel buffer")]
    DmaTransmitBuffer(DmaBufError),
    #[error("Failed to create SPI bus")]
    SpiBus(#[from] spi::master::ConfigError),
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to send command")]
pub(crate) struct SendCommandError(pub(super) spi::Error);

#[derive(Debug, thiserror::Error)]
#[error("Failed to send data")]
pub(crate) struct SendDataError(pub(super) spi::Error);

#[derive(Debug, thiserror::Error)]
pub(crate) enum SetRamAreaError {
    #[error("Failed to send command")]
    SendCommand(#[from] SendCommandError),
    #[error("Failed to send data")]
    SendData(#[from] SendDataError),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum InitializeControllerError {
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
pub(crate) struct WaitForBusyTimeoutError(pub(super) TimeoutError);

#[derive(Debug, thiserror::Error)]
pub(crate) enum InitializationError {
    #[error("Failed to create e-ink display driver instance")]
    Create(#[from] CreateError),
    #[error("Failed to initialize e-ink display controller")]
    InitializeController(#[from] InitializeControllerError),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RefreshError {
    #[error("Failed to send command")]
    SendCommand(#[from] SendCommandError),
    #[error("Failed to send data")]
    SendData(#[from] SendDataError),
    #[error("Failed to wait for busy")]
    WaitForBusy(#[from] WaitForBusyTimeoutError),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DisplayError {
    #[error("Failed to set RAM area")]
    SetRamArea(#[from] SetRamAreaError),
    #[error("Failed to send command")]
    SendCommand(#[from] SendCommandError),
    #[error("Failed to send data")]
    SendData(#[from] SendDataError),
    #[error("Failed to refresh display")]
    Refresh(#[from] RefreshError),
}
