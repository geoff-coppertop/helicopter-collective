# Development

## Toolchain

- **Rust channel:** pinned in `rust-toolchain.toml` (currently `nightly-2025-11-01`)
- **Components:** `rust-src`, `rustfmt`, `llvm-tools`
- **Required nightly feature:** `#![feature(impl_trait_in_assoc_type)]` — needed by embassy's async executor pattern

The toolchain is automatically selected by `rust-toolchain.toml`. Do not change the channel without verifying embassy compatibility.

## Build Commands

```bash
# Build firmware (default target already set in .cargo/config.toml)
cargo build

# Build a specific example
cargo build --example blinky
cargo build --example mag_read
cargo build --example usb_web_server

# Flash and run on connected hardware (requires probe-rs and RP2350 connected via SWD)
cargo run
cargo run --example mag_read
cargo run --example usb_web_server

# Check for compilation errors without producing an artifact
cargo check
cargo clippy
```

The default build target `thumbv8m.main-none-eabihf` is set in `.cargo/config.toml` — no `--target` flag needed.

## Testing

The firmware binary itself can't run on the host — it targets `thumbv8m.main-none-eabihf` and needs real RP2350 hardware. Two things make most of the logic testable anyway:

1. **Peripheral-free logic lives in `src/` and is target-gated.** Embedded-only dependencies (`embassy-*`, `cortex-m-rt`, etc.) are scoped to `[target.'cfg(target_arch = "arm")'.dependencies]` in `Cargo.toml`, and `src/lib.rs` uses `#![cfg_attr(not(test), no_std)]` rather than an unconditional `no_std`. That means modules with no hardware dependency — `src/units.rs` (ADC-count → engineering-unit conversion), `src/filter.rs` (EMA / One Euro Filter smoothing), `src/mdns.rs` (mDNS name encoding), `src/status.rs` (fixed-buffer status formatting) — compile and run under plain `std` on the host.

   ```bash
   # Run host-side unit tests (aliased in .cargo/config.toml)
   cargo test-host
   ```

   This runs `cargo test --lib --target x86_64-unknown-linux-gnu`, exercising the `#[cfg(test)] mod tests` blocks in each of those modules directly — no probe, no hardware.

2. **The `tmag5273` driver crate has its own host-run test suite.** `tmag5273/tests/driver.rs` uses `embedded-hal-mock` to fake the I2C bus and asserts on register read/write order and multi-byte value decoding, run with plain `cargo test` from `tmag5273/`.

When adding new logic, prefer extracting it into `src/` (or `tmag5273/`) over writing it inline in `examples/`, specifically so it's reachable by `cargo test-host`.

CI (`.github/workflows/ci.yml`) runs both the devcontainer build and the `tmag5273` test suite on every push/PR to `main`.

What's still untestable off-hardware: anything that actually talks to a peripheral (I2C transfers to a real sensor, USB enumeration, GPIO timing). Validate those by:

1. Flashing and observing RTT log output on real hardware
2. Examples in `examples/` serving as integration/manual-test entry points for individual hardware features

## Logging and Debugging

Logs are output via RTT (Real-Time Transfer over SWD debugger):

- **Via probe-rs runner:** `cargo run` streams defmt output to the terminal automatically
- **Via VS Code:** Use the "helicopter-collective launch" debug config in `.vscode/launch.json`
- **Log level:** Controlled by `DEFMT_LOG` env var (set to `"debug"` in `.cargo/config.toml`)

defmt log levels: `trace` < `debug` < `info` < `warn` < `error`

## Dev Container

The repository includes a fully configured VS Code devcontainer (`.devcontainer/`):

- Debian bookworm base with embedded Rust toolchain pre-installed
- `probe-rs` for flashing and debugging
- `flip-link` for stack overflow detection
- VS Code extensions: `rust-analyzer`, `probe-rs-debugger`, `cortex-debug`
- USB passthrough (`/dev/bus/usb`) for hardware access from the container — see the [README](../README.md#development-setup) for the host-side udev setup this requires
- Format-on-save, trim-trailing-whitespace, autofetch enabled

To use: open the repo in VS Code and select "Reopen in Container".
