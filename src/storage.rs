use alloc::string::ToString;
use defmt::info;
use embedded_sdmmc::{Continue, LfnBuffer, TimeSource, Timestamp, VolumeIdx, VolumeManager};

use crate::spi;

/// A time source that always returns a timestamp of 0.
/// Could think about implementing real time with the RTC and trying to keep it accurate using NTP
struct NoopTimeSource;

impl TimeSource for NoopTimeSource {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp::from_fat(0, 0)
    }
}

type SdCard<'a> = embedded_sdmmc::SdCard<spi::Device<'a>, embassy_time::Delay>;
type SdCardError = embedded_sdmmc::Error<embedded_sdmmc::SdCardError>;
type Volume<'a> = embedded_sdmmc::Volume<'a, SdCard<'a>, NoopTimeSource, 4, 4, 1>;
type Directory<'a> = embedded_sdmmc::Directory<'a, SdCard<'a>, NoopTimeSource, 4, 4, 1>;

#[derive(defmt::Format)]
pub(crate) enum DebugError {
    OpenVolume(SdCardError),
    OpenRootDirectory(SdCardError),
}

pub(crate) struct Storage<'a> {
    volume_manager: VolumeManager<SdCard<'a>, NoopTimeSource>,
}

impl<'a> Storage<'a> {
    pub(crate) fn new(device: spi::Device<'a>) -> Self {
        let card = SdCard::new(device, embassy_time::Delay);
        let volume_manager = VolumeManager::new(card, NoopTimeSource);
        Self { volume_manager }
    }

    pub(crate) async fn debug_files(&self) -> Result<(), SdCardError> {
        let volume = self.volume_manager.open_volume(VolumeIdx(0)).await?;
        let root_directory = volume.open_root_dir()?;

        let mut count = 0;
        let mut buffer = [0u8; 256];
        let mut buffer = LfnBuffer::new(&mut buffer);
        root_directory
            .iterate_dir_lfn(&mut buffer, |entry, i_dont_know| {
                count += 1;

                let file_name = core::str::from_utf8(entry.name.base_name());
                info!(
                    "Entry: {:?} {}",
                    defmt::Debug2Format(&file_name),
                    i_dont_know
                );
                Continue::Yes
            })
            .await?;

        info!("Total entries: {}", count);
        Ok(())
    }

    pub(crate) async fn write_log(&self, time: u64, battery_level: u16) -> Result<(), SdCardError> {
        let volume = self.volume_manager.open_volume(VolumeIdx(0)).await?;
        let root_directory = volume.open_root_dir()?;
        let file = root_directory
            .open_file_in_dir("log.csv", embedded_sdmmc::Mode::ReadWriteCreateOrAppend)
            .await?;

        let time = time.to_string();
        let time = time.as_bytes();
        file.write(time).await?;
        file.write(b",").await?;
        let level = battery_level.to_string();
        let level = level.as_bytes();
        file.write(level).await?;
        file.write(b"\n").await?;

        Ok(())
    }
}
