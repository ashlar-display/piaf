# Testing strategy

Binary parsers benefit from a testing approach that combines small deterministic tests with larger corpus-based validation.

## Test categories

### Unit tests

Unit tests cover narrow pieces of logic and live next to the code they test. Handler tests
call `process` directly on a handcrafted `[u8; 128]`, without going through the parser or
building an `ExtensionLibrary`. Parser tests construct minimal valid EDID byte arrays and
assert on specific error and warning conditions.

This keeps failures localized: a failing test in `base.rs` can only mean `BaseBlockHandler`
is broken.

### Integration tests

A single integration test in `capabilities/mod.rs` verifies that the full pipeline wires
together correctly — that `with_standard_handlers()` registers the handlers and that
`capabilities_from_edid` invokes them. It does not duplicate the field-level assertions
that belong in handler unit tests.

### Fixture tests

Fixtures are real EDID captures in `testdata/`. Tests in `tests/fixtures.rs` (and a few in
`tests/serde.rs` and `tests/static_pipeline.rs`) load them by path and assert on the
decoded fields. These fixtures make it easier to improve the parser without unintentionally
changing behavior.

```text
testdata/
 ├── valid/     truthful EDIDs
 ├── edge/      parse, but lie or are odd: placeholder sizes, a driver board's own EDID,
 │              0×0 cm in the base block
 └── invalid/   rejected by the parser today: truncated or corrupted captures
```

A fixture is the raw capture, unmodified, named as `examples/capture_fixture.rs` names it
(`<display name or manufacturer ID + product code>_<manufacturer ID>.bin`, lowercase,
words joined with `_`). [`CONTRIBUTING.md`](../CONTRIBUTING.md#contributing-edid-fixtures)
describes how to capture and contribute one.

Each fixture added from now on has a context file next to it, with the same name and a
`.toml` extension. It records what is true about the display, as opposed to what its EDID
claims, so a test can check a decoded value or a warning against reality, not only against
piaf's previous output. The fixtures that existed before this convention have none.

| Table     | Key                 | Meaning                                                                         |
|-----------|---------------------|---------------------------------------------------------------------------------|
| `capture` | `contributor`       | Who captured it                                                                 |
|           | `date`              | Capture date                                                                    |
|           | `method`            | How: `capture_fixture`, Linux sysfs, Windows registry, …                        |
|           | `os`                | Operating system of the capture                                                 |
|           | `connection`        | Direct, via a driver board, via adapters, …                                     |
|           | `serials`           | `original` or `anonymised`                                                      |
| `display` | `make_model`        | The display, as sold                                                            |
|           | `panel`             | The panel; for a driver board, the panel behind it                              |
|           | `native_resolution` | `[width, height]` in pixels                                                     |
|           | `measured_size_mm`  | `[width, height]` of the active area                                            |
|           | `size_source`       | `measured` or `manufacturer spec`                                               |
|           | `reference`         | URL of the source for spec values, e.g. a spec sheet or a display database page |
|           | `running_mode`      | What the OS actually drives                                                     |
| `notes`   | `known_issues`      | List of what the EDID gets wrong or does oddly                                  |

Every key is optional: a gap is better than a guess. New keys may be added as tests need
them.

### Fuzzing

Fuzzing is strongly recommended for the parser.

Important expectations:

- no panics,
- no uncontrolled memory growth,
- invalid input results in controlled errors or warnings,
- unknown structures do not break parsing invariants.

Two fuzz targets live in `fuzz/fuzz_targets/`:

- `parse_edid` — exercises the full dynamic pipeline: raw bytes → `parse_edid` → `capabilities_from_edid`.
- `capabilities_static` — exercises the static (no-alloc) pipeline: raw bytes → `parse_edid` → `capabilities_from_edid_static`.

Both are set up using `cargo-fuzz` with libFuzzer. `cargo-fuzz` requires nightly; the library itself stays on stable.

#### Quick smoke run

Runs until Ctrl+C. Useful for interactive testing:

```
cargo +nightly fuzz run parse_edid
cargo +nightly fuzz run capabilities_static
```

Any crashes are written to `fuzz/artifacts/<target>/`.

#### Long campaign

Run for a fixed duration (e.g. one hour), then minimise and commit the resulting corpus:

```
cargo +nightly fuzz run parse_edid -- -max_total_time=3600
cargo +nightly fuzz run capabilities_static -- -max_total_time=3600
```

After the run completes, deduplicate the corpus down to a minimal covering set:

```
cargo +nightly fuzz cmin parse_edid
cargo +nightly fuzz cmin capabilities_static
```

The minimised corpus lives in `fuzz/corpus/<target>/` and should be committed so that subsequent runs — locally and in CI — start from a richer base.

#### CI

The fuzz workflow (`.github/workflows/fuzz.yml`) runs a 60-second smoke test on every push and pull request, and a 1-hour deep run on a weekly schedule. The corpus is cached between runs using GitHub Actions cache. Crash artifacts are uploaded automatically on failure.

## Test philosophy

PIAF should be strict about structural integrity, but practical about diagnostics.

The test suite should reflect that balance by checking both:

- outright rejection of invalid core structure,
- graceful handling of unusual or partially malformed optional content.

## Long-term goal

As the fixture corpus grows, it should become a source of confidence for refactoring, extension support, and improvements to the normalization layer.
