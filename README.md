# Crustpoint

Experimenting with the xteink 4 ereader and rust

## Resources

- [ESP32 embedded Rust setup explained (Video)](https://youtu.be/dxgufYRcNDg)
- [The Rust on ESP Book](https://docs.espressif.com/projects/rust/book/)
- Repositories
  - [CrossPoint Reader](https://github.com/daveallie/crosspoint-reader)
  - [Xteink X4 Sample](https://github.com/CidVonHighwind/xteink-x4-sample)
- Tools
  - [Xteink Flash Tools](https://xteink.dve.al/)
  - [ESPConnect](https://thelastoutpostworkshop.github.io/ESPConnect/)

## Backup

1. Install espflash through cargo
2. The xteink x4 needs to be on and not in standby
3. Based on https://github.com/CidVonHighwind/xteink-x4-sample run `espflash read-flash 0x0 0x1000000 firmware_backup.bin --chip esp32c3 --port /dev/cu.usbmodem2101`. It will take a while around 30 minutes for me and has no status indicator. If you want a status indicator use the python esptool
4. Repeat for `espflash read-flash --chip esp32c3 --port /dev/cu.usbmodem2101 0x10000 0x640000 app0_backup_rust_espflash.bin`
   1. The port might change depending on your environment

## Restore

`uv run esptool --chip esp32c3 --port /dev/cu.usbmodem2101 write_flash 0x0 firmware_backup.bin`
