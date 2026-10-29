# Development Setup

**Requirements:** Rust 1.85+ (stable). Install via [rustup](https://rustup.rs/).

## Clone and build

```sh
git clone https://github.com/ashlar-display/piaf.git
cd piaf
cargo build
```

## Running tests

```sh
cargo test                            # std (default)
cargo test --features serde           # std + serde
cargo build --no-default-features --features alloc  # alloc-only build check
cargo build --no-default-features                   # bare no_std build check
```

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
