use embassy_time::TimeoutError;
use embedded_hal::spi::Error;
use esp_hal::spi;

#[derive(Debug, thiserror::Error, defmt::Format)]
pub(crate) enum CreateError {
    #[error("Failed to create SPI bus")]
    SpiBus(#[from] spi::master::ConfigError),
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to send command")]
pub(crate) struct SendCommandError<E: Error>(#[from] pub(super) E);

#[derive(Debug, thiserror::Error)]
#[error("Failed to send data")]
pub(crate) struct SendDataError<E: Error>(#[from] pub(super) E);

#[derive(Debug, thiserror::Error)]
pub(crate) enum SetRamAreaError<E: Error> {
    #[error("Failed to send command")]
    SendCommand(#[from] SendCommandError<E>),
    #[error("Failed to send data")]
    SendData(#[from] SendDataError<E>),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum InitializeControllerError<E: Error> {
    #[error("Failed to send command")]
    SendCommand(#[from] SendCommandError<E>),
    #[error("Failed to send data")]
    SendData(#[from] SendDataError<E>),
    #[error("Timed out waiting for busy")]
    WaitForBusy(#[from] WaitForBusyTimeoutError),
    #[error("Failed to set RAM area")]
    SetRamArea(#[from] SetRamAreaError<E>),
}

#[derive(Debug, thiserror::Error, defmt::Format)]
#[error("Timeout waiting for busy")]
pub(crate) struct WaitForBusyTimeoutError(pub(super) TimeoutError);

#[derive(Debug, thiserror::Error)]
pub(crate) enum InitializationError<E: Error> {
    #[error("Failed to create e-ink display driver instance")]
    Create(#[from] CreateError),
    #[error("Failed to initialize e-ink display controller")]
    InitializeController(#[from] InitializeControllerError<E>),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RefreshError<E: Error> {
    #[error("Failed to send command")]
    SendCommand(#[from] SendCommandError<E>),
    #[error("Failed to send data")]
    SendData(#[from] SendDataError<E>),
    #[error("Failed to wait for busy")]
    WaitForBusy(#[from] WaitForBusyTimeoutError),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DisplayError<E: Error> {
    #[error("Failed to set RAM area")]
    SetRamArea(#[from] SetRamAreaError<E>),
    #[error("Failed to send command")]
    SendCommand(#[from] SendCommandError<E>),
    #[error("Failed to send data")]
    SendData(#[from] SendDataError<E>),
    #[error("Failed to refresh display")]
    Refresh(#[from] RefreshError<E>),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum EnterDeepSleepError<E: Error> {
    #[error("Failed to send command")]
    SendCommand(#[from] SendCommandError<E>),
    #[error("Failed to send data")]
    SendData(#[from] SendDataError<E>),
    #[error("Failed to wait for busy")]
    WaitForBusy(#[from] WaitForBusyTimeoutError),
}
