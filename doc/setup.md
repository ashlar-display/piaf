# Development Setup

**Requirements:** Rust 1.85+ (stable). Install via [rustup](https://rustup.rs/).

## Clone and build

```sh
git clone https://github.com/ashlar-display/piaf.git
cd piaf
cargo build
```

## Running checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo clippy --all-targets --no-default-features --features alloc -- -D warnings
cargo clippy --all-targets --features serde -- -D warnings
cargo rustdoc --all-features -- -D warnings
```

## Running tests

```sh
cargo test                                         # std (default)
cargo test --features serde                        # std + serde
cargo test --no-default-features --features alloc  # alloc only
cargo test --no-default-features                   # bare no_std
```

The examples and integration tests need `alloc`, so Cargo skips them without it; the
README's examples run as doctests with the others. See [testing.md](testing.md) for what
the tests cover.

## `no_std` builds

Without `std` the crate is `no_std`, with or without `alloc`. CI checks both on a bare
target, which has no `std` for a dependency to pull in:

```sh
rustup target add thumbv7em-none-eabi
cargo build --no-default-features --features alloc --target thumbv7em-none-eabi
cargo build --no-default-features --target thumbv7em-none-eabi
```

## Checking the MSRV

The MSRV is `rust-version` in `Cargo.toml`. CI checks it with
[`cargo-msrv`](https://github.com/foresterre/cargo-msrv), which runs the checks on that
toolchain:

```sh
cargo install cargo-msrv --version 0.19.6 --locked
cargo msrv verify -- cargo test --locked
cargo msrv verify -- cargo test --locked --features serde
cargo msrv verify -- cargo test --locked --no-default-features --features alloc
cargo msrv verify -- cargo test --locked --no-default-features
cargo msrv verify --target thumbv7em-none-eabi \
  -- cargo build --locked --no-default-features --features alloc --target thumbv7em-none-eabi
cargo msrv verify --target thumbv7em-none-eabi \
  -- cargo build --locked --no-default-features --target thumbv7em-none-eabi
```

## Measuring coverage

Install [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) and the LLVM tools
component, then measure the default and `serde` feature sets as CI does:

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report --locked
cargo llvm-cov --no-report --locked --features serde
cargo llvm-cov report --summary-only
```

CI fails when line coverage drops more than 0.1 points below `.coverage-baseline`, and
opens a PR raising the baseline when a push improves it.

## Fuzzing

Fuzzing requires nightly:

```sh
cargo +nightly fuzz run parse_edid
cargo +nightly fuzz run capabilities_static
```

See [`doc/testing.md`](testing.md) for long-campaign workflows and corpus management.

## Capturing EDID fixtures

To capture EDID binaries from connected displays (Linux only; reads `/sys/class/drm`):

```sh
cargo run --example capture_fixture
```

It saves each display's EDID to `testdata/valid/` and prints test stubs for
`tests/fixtures.rs`. It skips EDIDs that fail to parse; those belong in `testdata/invalid/`
and are copied by hand. A display whose EDID lies (placeholder size, a driver board's own
EDID) belongs in `testdata/edge/`: move it there after capturing.

Fixtures live in `testdata/`; see [`doc/testing.md`](testing.md#fixture-tests) for the layout
and the context file that goes with each one. Adding real-world captures from unusual
hardware is welcome: see
[`CONTRIBUTING.md`](../CONTRIBUTING.md#contributing-edid-fixtures).
