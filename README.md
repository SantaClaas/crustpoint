# Crustpoint

Experimenting with the xteink 4 ereader and rust

## Resources

I did not build this from scratch. This is based on the previous and future work of others including:

- [ESP32 embedded Rust setup explained (Video)](https://youtu.be/dxgufYRcNDg)
- [The Rust on ESP Book](https://docs.espressif.com/projects/rust/book/)
- Repositories
  - [CrossPoint Reader](https://github.com/daveallie/crosspoint-reader)
  - [Xteink X4 Sample](https://github.com/CidVonHighwind/xteink-x4-sample)
  - [Ariel-OS fork x4 example](https://github.com/juicecultus/ariel-os/blob/a2816b156b632b2633801df45d69c8fe9dde500c/examples/x4-launcher/src/ssd1677.rs)
  - [Xteink X4 sample rust](https://github.com/HookedBehemoth/TrustyReader/)
- Tools
  - [Xteink Flash Tools](https://xteink.dve.al/)
  - [ESPConnect](https://thelastoutpostworkshop.github.io/ESPConnect/)
- Crates
  - [Embedded graphics](https://crates.io/crates/embedded_graphics)
  - [Embedded SD/mmc](https://crates.io/crates/embedded_sdmmc)
- [Reading/Writing partition tables](https://docs.espressif.com/projects/rust/esp-bootloader-esp-idf/0.3.0/esp32c2/esp_bootloader_esp_idf/partitions/index.html)

## Backup

1. Install espflash through cargo
2. The xteink x4 needs to be on and not in standby
3. Based on https://github.com/CidVonHighwind/xteink-x4-sample run `espflash read-flash 0x0 0x1000000 firmware_backup.bin --chip esp32c3 --port /dev/cu.usbmodem2101`. It will take a while around 30 minutes for me and has no status indicator. If you want a status indicator use the python esptool
4. Repeat for `espflash read-flash --chip esp32c3 --port /dev/cu.usbmodem2101 0x10000 0x640000 app0_backup_rust_espflash.bin`
   1. The port might change depending on your environment

## Restore

`uv run esptool --chip esp32c3 --port /dev/cu.usbmodem2101 write_flash 0x0 firmware_backup.bin`
