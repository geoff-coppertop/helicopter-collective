# CLAUDE.md

## Project Overview

`helicopter-collective` is embedded Rust firmware for a helicopter collective controller built on the Raspberry Pi RP2350. It uses a TMAG5273 Hall-effect magnetometer over I2C to track position.

See the [README](README.md) and [docs/](docs/) for full documentation:

- [Hardware & Memory Layout](docs/hardware.md)
- [Development — build, debug, test, dev container](docs/development.md)
- [Architecture — structure, dependencies, conventions](docs/architecture.md)

## Constraints for AI Assistants

- **No host execution for the firmware binary:** the ARM-target code (`src/main.rs`, `examples/`) can't run with plain `cargo test` or on the host — it requires physical RP2350 hardware. Peripheral-free logic in `src/` (filters, unit conversion, status formatting, mDNS encoding) is target-gated so it *can* run on the host via `cargo test-host` — see [docs/development.md](docs/development.md#testing).
- **No std on the ARM target:** don't introduce `std` types or `println!` into firmware/example code. Use `defmt` for logging and `heapless` or stack allocation for data structures.
- **No heap:** there is no allocator. Avoid `Vec`, `String`, `Box`. Use fixed-size arrays, `heapless` collections, or stack-allocated types.
- **Nightly required:** the codebase uses nightly-only features (`impl_trait_in_assoc_type`). Do not attempt to downgrade to stable.
- **Peripheral ownership:** Embassy and Rust's ownership model ensure each peripheral is owned by exactly one task. Do not share peripherals without proper synchronization primitives (`Mutex`, channels).
- **Prefer async delays:** `Timer::after().await` is preferred over blocking `cortex_m::delay`. `embassy_executor` runs cooperative async tasks.

# Working conventions for this repo

## Git

- Before rebasing, always `git fetch origin` first. Never rebase onto a
  possibly-stale local `main`/base branch.
- On a branch that hasn't been merged/released, never leave a commit whose
  purpose is to fix a defect introduced earlier in that same branch. Fold
  the fix back into the commit that introduced the problem instead — every
  commit in an unreleased branch should be correct as introduced. This
  applies even after the branch has already been pushed; rewrite history
  (`git reset --soft` / cherry-pick reconstruction, not `rebase -i`) rather
  than appending a trailing "Fix ..." commit.
- Once a branch's PR has been merged, treat any follow-up work as fresh:
  restart the branch from the latest default branch rather than stacking
  new commits on merged history.
- Split commits by logical subtopic, not by when the user happened to ask
  for them. If a single request (or a single conversation turn) covers
  several genuinely separable concerns, they still get their own commits —
  check whether each piece is independently buildable/testable on its own
  and, if so, commit it that way. Conversely, don't split a single
  indivisible change into multiple commits just because it was described
  in separate sentences.

## Pull requests

- Keep the PR description up to date with the branch: when new commits land,
  update the summary and commit list so the description matches what's
  actually in the branch, not just what was there when the PR was opened.
- Always include a test plan written as literal markdown checkboxes
  (`- [ ] ...`), covering both automated checks (build, unit tests) and
  manual verification steps for anything not covered by tests. Update the
  checklist as functionality is added, not just when the PR is first created.

## Tests

- Add tests in the same commit as the code they exercise, not as a
  follow-up "add tests" commit.
- Only write tests for logic this project owns. Don't write tests that
  exercise third-party crate internals or behavior.
- Every test needs a clear justification: what real failure mode does it
  catch? Don't add tests purely for coverage padding.
- Host-side unit tests run via `cargo test-host` (aliased in
  `.cargo/config.toml`), targeting `x86_64-unknown-linux-gnu` since the
  embedded target has no std. Pure logic that doesn't need embedded-only
  dependencies (`heapless`, `embassy-*`, etc.) should be extracted into
  `src/` (see `src/mdns.rs`, `src/status.rs`) so it's testable this way,
  rather than left inline in `examples/`.
