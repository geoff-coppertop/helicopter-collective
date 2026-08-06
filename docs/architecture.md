# Architecture

## Project Structure

```
helicopter-collective/
├── src/
│   ├── main.rs          # Firmware entry point — blinks LED on PIN_25
│   ├── lib.rs           # `#![cfg_attr(not(test), no_std)]` — exposes the modules below to
│   │                    # both the ARM firmware and host-side `cargo test-host`
│   ├── units.rs         # Raw ADC counts → engineering units (mT, °C)
│   ├── filter.rs        # EMA / One Euro Filter smoothing for the magnetic axes
│   ├── mdns.rs          # mDNS name encoding / responder logic
│   └── status.rs        # Fixed-buffer status string formatting
├── examples/
│   ├── blinky.rs         # GPIO blink example (mirrors main.rs)
│   ├── mag_read.rs       # I2C TMAG5273 read → convert → filter → log pipeline
│   └── usb_web_server.rs # USB CDC-NCM web server: live status page, mDNS, LED control
├── tmag5273/              # Local TMAG5273 driver crate (path dependency)
│   ├── src/lib.rs
│   ├── src/types.rs
│   └── tests/driver.rs   # Host-run tests against a mocked I2C bus
├── build.rs               # Copies memory.x to OUT_DIR for the linker
├── memory.x               # Linker script — RP2350 memory layout
├── Cargo.toml             # Dependencies (embedded deps gated to the `arm` target)
├── rust-toolchain.toml    # Pinned nightly toolchain
├── .cargo/config.toml     # Default build target, probe-rs runner, `test-host` alias
├── .github/workflows/     # CI: devcontainer build + host-side unit tests
├── .devcontainer/         # VS Code devcontainer for embedded Rust development
└── .vscode/
    ├── launch.json          # probe-rs debug launch configuration
    └── rp2350.svd            # SVD peripheral definitions for debugger register view
```

## Key Dependencies

Embedded-only dependencies are gated to `[target.'cfg(target_arch = "arm")'.dependencies]` in `Cargo.toml` so the plain `[dependencies]` section — and therefore the library — stays buildable on the host for `cargo test-host`.

| Crate | Purpose |
|---|---|
| `embassy-executor` | Async task executor for embedded (nightly features required) |
| `embassy-rp` | RP2350 HAL — GPIO, I2C, USB, peripherals |
| `embassy-time` | Async timers and delays |
| `embassy-net` | TCP/UDP/IPv4 networking stack over the USB CDC-NCM interface |
| `embassy-usb` | USB device stack (CDC-NCM class) |
| `embassy-sync` | `Mutex`/`Signal` primitives for sharing state between tasks |
| `embedded-hal` / `embedded-hal-async` | Hardware abstraction traits |
| `heapless` | Fixed-capacity collections (no heap available) |
| `leasehund` | DHCP server for the USB Ethernet interface |
| `picoserve` | Embedded HTTP server for the status web page |
| `static_cell` | `'static` allocation of embassy resources without a heap |
| `defmt` + `defmt-rtt` | Structured logging over RTT (Real-Time Transfer via SWD) |
| `panic-probe` | Panic handler that prints via defmt |
| `cortex-m-rt` | Cortex-M runtime (startup, interrupt vectors) |
| `tmag5273` | Local path dependency (`./tmag5273`) — TMAG5273 magnetometer driver |

`tmag5273` was originally a Git dependency; it's now vendored as a local crate in this repo (see `tmag5273/`) with its own host-run test suite.

## Coding Conventions

### Embedded Rust patterns used throughout

- Firmware entry points (`src/main.rs`, `examples/*.rs`) start with `#![no_std]`, `#![no_main]`, `#![feature(impl_trait_in_assoc_type)]`
- `src/lib.rs` instead uses `#![cfg_attr(not(test), no_std)]` — `no_std` on the ARM target, plain `std` under `cargo test-host`
- Entry point is `async fn main(_spawner: Spawner)` decorated with `#[embassy_executor::main]`
- Peripherals are claimed once at startup via `embassy_rp::init(Default::default())`
- GPIO is controlled via `embassy_rp::gpio::Output`
- Delays use `Timer::after(Duration::from_millis(...)).await`
- I2C uses `embassy_rp::i2c::I2c::new_async` with interrupt binding via `bind_interrupts!`
- Logging uses `defmt` macros (`info!`, `warn!`, `error!`) — not `println!` (no std)
- Panic handler is `panic_probe` (prints defmt over RTT, then halts)

### Import style

```rust
use {defmt_rtt as _, panic_probe as _};  // side-effect-only imports last
```

### Error handling

- `unwrap()` is acceptable in examples and early-stage firmware where panicking on error is intentional
- Production code should use proper error propagation with `?` and `Result<T, E>` returns

### Testable logic goes in `src/`

Peripheral-free logic (unit conversion, filtering, string formatting, protocol encoding) belongs in `src/`, not inlined in `examples/`, so it can be unit-tested on the host. See [Testing](development.md#testing).
