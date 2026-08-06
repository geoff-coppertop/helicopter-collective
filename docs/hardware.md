# Hardware

## Target

| Component | Detail |
|---|---|
| MCU | Raspberry Pi RP2350 (ARM Cortex-M33, dual-core) |
| Chip variant | `rp235xa` (used in embassy-rp feature flags) |
| Build target | `thumbv8m.main-none-eabihf` |
| Sensor | TMAG5273B1 Hall-effect magnetometer on I2C0 |
| I2C pins | SDA = PIN_20, SCL = PIN_21 |
| LED | Onboard LED on PIN_25 |
| USB | CDC-NCM (USB Ethernet) — see `examples/usb_web_server.rs` |
| Debugging | SWD via a Raspberry Pi Debug Probe, using probe-rs with RTT logging |

This is `no_std` / `no_main` firmware — there is no operating system, no heap allocator, and no standard library on the ARM target. Pure logic that doesn't touch peripherals lives in `src/` and is target-gated so it can also compile and run under `std` on the host — see [Testing](development.md#testing).

## Sensor Data

The TMAG5273 reports raw ADC counts for X/Y/Z magnetic field and temperature. `src/units.rs` converts these into engineering units (mT, °C); `src/filter.rs` smooths the magnetic axes with a One Euro Filter before use. See `examples/mag_read.rs` for the full read → convert → filter → log pipeline.

## Memory Layout

Defined in `memory.x`:

| Region | Start | Size | Notes |
|---|---|---|---|
| FLASH | `0x10000000` | 2048K | |
| RAM | `0x20000000` | 512K | Striped across 8 banks for performance |
| SRAM4 | `0x20080000` | 4K | |
| SRAM5 | `0x20081000` | 4K | |

`build.rs` copies `memory.x` into `OUT_DIR` so the linker can find it.

## USB Debug Probe Access

Flashing and debugging go through a [Raspberry Pi Debug Probe](https://www.raspberrypi.com/products/debug-probe/) connected over USB. See the [README](../README.md#development-setup) for host udev setup — permissions have to be granted on the host, not inside the devcontainer.
