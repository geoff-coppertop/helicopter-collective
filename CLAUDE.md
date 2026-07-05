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
