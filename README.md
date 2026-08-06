# helicopter-collective

I started this project by pieceing together,

- the [example for the RP2040](https://archive.hannobraun.com/embedded-rust/getting-started/)
- this [embassy quickstart template](https://github.com/9names/embassy-rp-quickstart/tree/embassy-rp-0.3-rp235x)

I've lifted individual hardware examples from,

- the [embassy rp235x examples](examples/rp235x)
- the [sparkfun tmag5273 arduino library](https://github.com/sparkfun/SparkFun_TMAG5273_Arduino_Library)

This project uses similar hardware to implement a [3D mouse](https://github.com/sb-ocr/diy-spacemouse).

## Documentation

- [Hardware & Memory Layout](docs/hardware.md)
- [Development — build, debug, test, dev container](docs/development.md)
- [Architecture — structure, dependencies, conventions](docs/architecture.md)

## Development Setup

### USB access to the Raspberry Pi Debug Probe

Flashing and debugging (`probe-rs run`, `probe-rs-debugger`) go through a
[Raspberry Pi Debug Probe](https://www.raspberrypi.com/products/debug-probe/)
connected over USB. The devcontainer doesn't create its own USB device nodes —
`.devcontainer/devcontainer.json` bind-mounts the host's `/dev/bus/usb` into
the container and runs with `--userns=keep-id`, so the container process ends
up checked against the exact same device node, under the same UID, as the
host user. That means **USB permissions have to be granted on the host**, not
inside the container image — there's no way for the Dockerfile or devcontainer
features to grant this from the inside.

`.devcontainer/69-probe-rs.rules` is the udev rules file (from the
probe-rs/OpenOCD project) that grants non-root access to the probe and other
common debug adapters. It needs to be installed wherever the devcontainer
actually runs:

- **On a NixOS host managed by
  [`nixos-config`](https://github.com/geoff-coppertop/nixos-config)**: already
  handled — see that repo's `custom.debugProbes.enable` option
  (`modules/debug-probes.nix`) and its README § USB Debug Probes (udev).
- **On any other Linux host**, install it manually:

  ```bash
  sudo cp .devcontainer/69-probe-rs.rules /etc/udev/rules.d/
  sudo udevadm control --reload
  ```

  Then replug the probe (or run `sudo udevadm trigger`) so the new
  permissions apply to an already-connected device.

  **Keep the filename exactly `69-probe-rs.rules`** — don't rename it or
  merge its contents into some other rules file. The `69-` prefix is
  load-bearing: it makes this file sort *before* systemd's own
  `70-uaccess.rules`/`73-seat-late.rules`, which only grant USB access via
  ACL if a device is already tagged `uaccess` by the time they're evaluated.
  A renamed or merged copy that sorts *after* those files will silently fail
  to grant access on a device's first-ever plug-in — it can look like it
  "sometimes" works, because re-triggering an already-enumerated device
  masks the problem by reusing a tag that persisted from an earlier pass.
