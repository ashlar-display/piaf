# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking changes

- **The HDMI Audio Data Block was misparsed, and `HdmiAudioBlock` changes with
  display-types 0.5.** piaf read a multi-stream flag from an undefined bit and parsed
  the rest as Short Audio Descriptors; the block holds a stream count, MS NonMixed
  support, and 4-byte HDMI 3D Audio Descriptors followed by a 3D speaker allocation. It
  is now parsed that way (as edid-decode and libdisplay-info do) into the new
  `HdmiAudioBlock` fields (`max_streams`, `ms_non_mixed`, `audio_3d_descriptors`,
  `speaker_allocation_3d`); `Audio3dChannels` and `Audio3dSpeakerAllocation` are
  re-exported. New warnings: `EdidWarning::HdmiAudioNonMixedWithoutMultiStream`,
  `HdmiAudio3dDescriptorsExceedBlock` (none taken), `HdmiAudio3dUnsupportedFormat` (that
  descriptor skipped).

### Added

- **Horizontal blanking on decoded modes.** `VideoMode::h_blank` (display-types 0.5) is
  set from EDID and CTA Detailed Timing Descriptors, DisplayID detailed timings (Types I,
  II, VI, VII), DisplayID Type IX (from the CVT formula), and VIC and DMT lookups. It is
  left `None` for modes known only by resolution and refresh rate: established and
  standard timings, CVT 3-byte codes, DisplayID Types III and V, and CTA T10VTDB entries
  (whose CVT formula piaf does not apply yet).
- **DisplayID HDMI VICs 1–4 carry their full timing**, taken from the CTA VICs they are
  defined as (95, 94, 93, 98): pixel clock, porches, sync and `h_blank`. Their `source`
  stays `None`.
- **`EdidWarning::DtdBordersAmbiguous`** for a Detailed Timing Descriptor with non-zero
  borders. EDID 1.3 counts the borders in the blanking, EDID 1.4 does not; piaf reads
  them as 1.3 (as EDIDs in practice do, and as its refresh rate already did), so
  `h_blank` is the blanking minus both borders, and the warning says so.
- **`EdidWarning::DtdBlankingInconsistent`** when that reading leaves less than the front
  porch and sync pulse (or, without borders, the blanking is too short for them): the
  mode is kept with `h_blank` left `None`, and the warning carries the raw values for a
  caller that applies the EDID 1.4 reading itself.

### Changed

- **Part of Ashlar** — the README now opens with a block naming the
  [Ashlar](https://ashlar-display.org) stack and linking the other crates in it. Keywords
  are aligned across the stack, with `ashlar` and `hdmi` on every crate; `ashlar` is added,
  `displayid` replaces `display` and `cea861`. The crate's `homepage` is now the Ashlar
  website.

- `doc/roadmap.md` drops its "Shipped" section, a stale copy of what this changelog
  records; the roadmap lists only what is planned.
- **Moved to the `ashlar-display` organization** — the repository is now
  `ashlar-display/piaf` (the old URLs redirect), and `repository` points there. Releases
  are attested and published from the new repository; 0.4.2, attested before the move,
  verifies with `--owner DracoWhitefire --signer-repo DracoWhitefire/piaf` (see "Verifying
  releases" in the README). The code of conduct is now the organization's, shared by every
  Ashlar repository; the repository's own copy is removed.

### Fixed

- **`capabilities_from_edid_static` dropped the base block's warnings in alloc builds**
  (bare `no_std` builds kept them): `DtdPixelClockOverflow`, the DTD border warnings and
  the rest now reach the static output in every build.
- **Detailed Timing Blocks in a VESA Video Timing Block Extension** (CTA extended tag
  0x03) were decoded by a separate decoder that kept only the resolution, refresh rate
  and interlace flag. They now go through the same decoder as every other DTD, with the
  pixel clock, porches, sync, borders, stereo and `h_blank`, and its warnings.

## [0.4.2] - 2026-10-18

### Internal

- **Automated publish can be triggered by `release-tag`** — `publish.yml` gains a
  `workflow_dispatch` trigger. Tags pushed with `GITHUB_TOKEN` do not start push-triggered
  workflows, so `release-tag`'s "Trigger publish workflow" step
  (`gh workflow run publish.yml`) could not start a publish run. Dispatches against a
  non-tag ref (e.g. `main`) are skipped, so they cannot publish or create a release.
- **Publishing uses crates.io trusted publishing** — `publish.yml` obtains a short-lived
  token through OIDC (`rust-lang/crates-io-auth-action`, revoked at the end of the job, and
  short-lived regardless) instead of the long-lived `CARGO_REGISTRY_TOKEN` secret. The
  upload job runs in the `release` environment, which only `v*` tags can deploy to and
  which requires a maintainer's approval. Quality gates moved to a separate `verify` job,
  so no dependency build scripts run and no cache is restored where publishing credentials
  exist. The attested `.crate` is kept as a separate copy; after upload the job fails if
  crates.io's checksum differs from it, and that same copy is attached to the GitHub
  release. Re-running a partially failed publish reuses an existing GitHub release.
  `publish.yml` refuses tags that do not match the `Cargo.toml` version.
- **Release summary before approval** — `publish.yml` gains a `summary` job that shows the
  crate, version and commit, changes since the previous published release (ignoring
  pre-release or malformed tags, and flagging a baseline that is not on crates.io), changes
  to workflow, toolchain and build configuration, manifest changes, the changelog section,
  and the `.crate` sha256 with a command to reproduce it, on the run page while the publish
  job waits for approval. The publish job refuses to attest a package whose sha256 differs
  from the reviewed one.
- **`release-tag` no longer tags after a failed promotion** — `release-tag` now promotes
  `develop` to `main` through a `release-promote/v*` branch and PR. A failed merge of the
  promote PR used to go unnoticed, after which the previous `main` commit could be tagged
  with the new version. The step now fails, and tagging requires `main` to hold exactly the
  promoted tree. It dispatches `publish.yml` with `actions: write` (it had only `read`, so
  the dispatch could not start a run), and it also accepts a squash-merged release commit
  (`Release vX.Y.Z (#N)`).
- **`release-tag` requires the fuzz smoke run too** — it now requires both CI and the fuzz
  smoke run to have passed on `develop`'s head (it checked CI only), and reports a run that
  has not finished yet as such rather than as `not-found`.
- **Release workflows only run from `develop`** — `release-prep` and `release-tag` fail when
  started from another branch, so an outdated copy of the workflow is not run against
  `develop` by mistake.
- **`release-prep` leaves the fuzz dependencies alone** — refreshing `fuzz/Cargo.lock` ran
  `cargo generate-lockfile`, which re-resolved every fuzz dependency inside the release
  commit. It now updates only `piaf`'s own entry; Dependabot proposes the rest.
- **No CI or fuzz run on promotion PRs** — CI and the fuzz workflow no longer run on pull
  requests into `main`. The only ones are `release-tag`'s `release-promote/*` PRs, whose
  content already passed both on `develop`; opened by `github-actions[bot]`, their runs
  would be held for approval and listed as `action_required` or failed.
- **Supply-chain hardening for CI** — every third-party action is pinned to a commit SHA
  of a release tag, with `dependabot.yml` keeping the pins (and both `Cargo.lock`s, the
  crate's and the fuzz crate's) current. The Rust toolchains themselves still follow
  `stable` and `nightly`. `cargo-semver-checks` is installed as a checksum-verified
  prebuilt binary (the version listed by the pinned `install-action`) instead of being
  restored from a cached `~/.cargo/bin`; `cargo-fuzz` is installed from crates.io at a fixed
  version instead of from its Git repository's latest commit. The security audit also runs
  weekly.
- **README documents release provenance** — a badge shows whether crates.io requires trusted
  publishing for this crate, and "Verifying releases" now states that versions up to 0.4.1
  carry no SLSA provenance and have no GitHub release (it claimed every release was
  attested), that they were uploaded with an API token (0.1.0 and 0.4.1 by hand, 0.2.0 to
  0.4.0 from CI), that later releases are attested and uploaded through trusted publishing,
  and how to look up a version's publishing details on crates.io.

## [0.4.1] - 2026-05-07

### Breaking changes

- **`VideoMode::refresh_rate` is now `Option<RefreshRate>`** (was `u16`). `VideoMode::new`
  accepts `impl Into<RefreshRate>` and stores `Some(...)`; integer literals require a `u32`
  suffix (e.g. `60u32`) or explicit `RefreshRate::integral(60)`. Reading `refresh_rate`
  must handle the `None` case (default-constructed `VideoMode` and any code that previously
  treated `0` as "unset"). Equality and ordering of refresh rates now go through
  `RefreshRate`'s exact rational representation, so DMT 0x58 (4096×2160) is preserved as
  `60000/1001` instead of the truncated `60`. Supersedes the prior `u8 → u16` widening
  noted in earlier unreleased notes.
- **`EdidWarning` new variant**: `UnsupportedV2BlockRevision { tag: u8, revision: u8 }`.
  Exhaustive `match` arms on `EdidWarning` must be updated.

### Added

- **DisplayID 2.x extension support** — full coverage of the 2.x tag space, decoded by the
  same `DisplayIdHandler` as 1.x; dispatch is selected by the section header version byte:
  - **`0x20`** Product Identification Block — `manufacturer_oui` on `DisplayIdCapabilities`;
    `product_code`, `serial_number`, `manufacture_date`, `display_name` on
    `DisplayCapabilities`. PNP-derived `manufacturer` is intentionally not populated.
  - **`0x21`** Display Parameters Block — `display_params_v2` (chromaticity, luminance,
    gamma, display technology, scan orientation, audio routing, CIE coordinate variant);
    `preferred_image_size_mm`, `native_pixels`, `color_bit_depth` mirrored on
    `DisplayCapabilities`.
  - **`0x22`** Type VII Detailed Timing — `supported_modes` (20-byte descriptors with
    24-bit pixel clock); decoder shared with the CTA T7VTDB path.
  - **`0x23`** Type VIII Enumerated Timing Code — `supported_modes` via DMT/VIC/HDMI VIC
    lookup, supporting both 1- and 2-byte code modes.
  - **`0x24`** Type IX Formula-Based Timing — `supported_modes` with `cvt_algorithm`
    (CVT-RB1/RB2/RB3, RB-with-CVT-RB1/RB2) and `y420` flag from byte 0. **CVT-RB v1**
    (VESA CVT 1.1 §3.4), **CVT-RB v2** (VESA CVT 1.2 §4), and **CVT-RB v3** (VESA CVT
    2.0 §4.5; baseline identical to v2 for fixed-rate descriptors) are fully expanded:
    `pixel_clock_khz`, `h_front_porch`, `h_sync_width`, `v_front_porch`, and
    `v_sync_width` are derived via `display_types::compute_type_ix_timing`. The
    RB-with-CVT-RB1/RB2 variants currently emit only the metadata; consumers can apply
    the formula themselves until full evaluation lands (see `doc/roadmap.md`).
  - **`0x25`** Dynamic Video Timing Range Limits — `dynamic_timing_range` (kHz precision
    pixel clock, 9-bit max v rate on rev ≥ 1, VRR flag); `max_pixel_clock_mhz`,
    `min_v_rate`, `max_v_rate` mirrored.
  - **`0x26`** Display Interface Features — `interface_features` (per-encoding color depth
    bitmasks, audio flags, color space + EOTF combinations bitmask).
  - **`0x27`** Stereo Display Interface — `stereo_interface_v2` (Field Sequential,
    Side-by-Side, Pixel Interleaved, Dual Interface, Multi-View, Stacked Frame,
    Proprietary, Reserved); timing scope decoded from revision bits 7:6.
  - **`0x28`** Tiled Display Topology — `tiled_topology` (same wire format as 1.x `0x12`).
  - **`0x29`** ContainerID — `container_id` (raw 16-byte UUID; mixed-endian vs RFC 4122
    interpretation deferred to consumers).
  - **`0x7E`** Vendor-Specific — `vendor_specific` (OUI + opaque payload, multiple records
    per section in payload order).
  - **`0x81`** CTA DisplayID — wraps a CTA-861 data block collection; merges into the
    existing `Cea861Capabilities` (extension tag `0x02`) via take-mutate-restore so
    `0x81`-derived data combines with a real CEA-861 extension regardless of processing
    order. Mode-producing entries also reach the static pipeline.
  - **2.x tag-space exhaustiveness test** mirrors the 1.x tag-coverage check.
- **`RefreshRate`** re-exported from `piaf` (originating in `display-types`). Constructors
  `integral(u32)`, `fractional(numer, denom)`, and `from_ratio(numer: u64, denom: u64)`
  for computing rates from large intermediate products such as
  `pixel_clock_hz / (h_total × v_total)`.
- **`CvtAlgorithm`** re-exported from `piaf` — CVT formula selector for Type IX
  descriptors. `Reserved(u8)` for spec-reserved encodings.
- **Type VII Detailed Timing decoder shared between DisplayID 2.x (`0x22`) and the CTA
  T7VTDB path** via `decode_type_vii_descriptor_to_mode`. The CTA wrapper remains parsed
  inline; the 20-byte body decodes through the shared function.
- **`parse_cea861_data_block_collection`** — `pub(crate)` helper extracted from the CEA-861
  handler so the DisplayID 2.x `0x81` block reuses the same data-block collection parser.
  Mode-producing entries still flow through `caps.supported_modes` with dedup; CTA-specific
  state merges into `Cea861Capabilities`.
- **SLSA Build Level 2 provenance** — release artifacts are attested via
  `actions/attest-build-provenance` and verified with
  `gh attestation verify <file> --repo DracoWhitefire/piaf`.
- **`ModeSource` now populated for DTDs**: `VideoMode::source` is set to
  `ModeSource::DtdIndex(n)` (zero-based) for all Detailed Timing Descriptors decoded from
  the EDID base block and CEA-861 extension blocks. VIC- and DMT-keyed modes already had their
  source set automatically by `display-types`; this fills in the remaining gap.
- **Dependency audit pipeline** - dependencies get checked on cargo manifest changes.

### Changed

- Updated `display-types` dependency to track the V2 type set (new `RefreshRate`,
  `CvtAlgorithm`, `DisplayParamsV2`, `DynamicTimingRange`, `DisplayInterfaceFeatures`,
  `DisplayIdStereoInterfaceV2`, `DisplayIdVendorSpecific`, `ChromaticityPoint12`,
  `Chromaticity12`, `ColorDepthsFull`, `ColorDepthsSubsampled`, `ScanOrientation`,
  `StereoTimingScopeV2`, `StereoViewingMethodV2`, `StereoEye`, `DualInterfaceMirroring`,
  and the `tag::V2_*` constants); `VideoMode` gains `cvt_algorithm` and `y420` fields.

### Fixed

- **DisplayID Type I (`0x03`) descriptor decoder was structurally wrong** — every
  field was misread. The wire layout (per VESA DisplayID 1.x §4.4.2 and the
  `edid-decode` `parse_displayid_type_1_7_timing` reference) is: bytes 0–2 carry a
  24-bit pixel clock in 10 kHz units stored as `value − 1`; byte 3 carries options
  (interlaced bit 4, aspect/stereo/preferred elsewhere); bytes 4–19 carry H/V
  active/blank/front-porch/sync-width fields all stored as `value − 1`, with sync
  polarity in bit 15 of the H/V Front Porch words. The previous decoder treated
  byte 0 as options, bytes 1–2 as a 16-bit pixel clock, all subsequent fields
  shifted by one byte, and byte 19 as a flags field — none of which matches the
  spec. Type I has been rewritten to delegate to a shared
  `decode_type_1_7_descriptor_body(d, pixel_clock_unit_khz)` helper that Type VII
  also uses.
- **DisplayID Type V (`0x11`), Type VII (`0x22`), Type IX (`0x24`) off-by-one** —
  every multi-byte timing field (pixel clock, H/V active, H/V blank, H/V front
  porch, H/V sync width) is encoded as `value − 1` on the wire (the all-zero
  descriptor is reserved as "null"). The previous decoders read `value` raw,
  producing widths/heights one short and pixel clocks one short. Type VII now
  delegates to the shared `decode_type_1_7_descriptor_body(d, 1)` helper; Type V
  and Type IX `h_active`/`v_active` read via `checked_add(1)?` so raw `0xFFFF` is
  silently skipped instead of overflowing.

  No fixture EDIDs in `testdata/` carry DisplayID extension blocks, so the prior
  bug was masked by self-consistent synthetic tests; real-world EDIDs with Type
  I/V/VII/IX descriptors would have produced wrong `width`/`height`/`pixel_clock_khz`.

  Test helpers (`make_type_i_descriptor`, `make_type_v_descriptor`,
  `make_type_vii_descriptor`, `make_type_ix_descriptor`) now accept human-readable
  values and apply `value − 1` internally, so test assertions stay readable.
  "Zero width/height skipped" tests were repurposed as "raw 0xFFFF overflow
  skipped" since the wire format cannot represent value 0.

  All items identified in the cross-check against `edid-decode` have since been
  resolved: `CvtAlgorithm` enum reshaped to spec-correct variants (`Cvt`/`CvtRb`/`CvtR2`);
  Type IX/V byte 0 bit 3 decoded as `ntsc_fractional_refresh` (not bit 4, and not `y420`);
  Type IX/V bits 6:5 decoded as `TypeIxStereoMode`; Type VII byte 3 bit 7 decoded as
  `y420` (block revision ≥ 2).

### Internal

- Updated `display-types` dependency from local path to published `0.4.0`.
- Fixed two Clippy lints in tests (`same_item_push`, `identity_op`): repeated
  `p.push(0x11)` in a loop replaced with `p.extend(core::iter::repeat_n(...))`;
  `(0b00 << 6) | 1` simplified to `1`.
- Added `release-prep` and `release-tag` GitHub Actions workflows. `release-prep`
  (workflow dispatch, bump input) runs `cargo semver-checks` to validate or auto-detect
  the required version bump, stamps `CHANGELOG.md`, regenerates `Cargo.lock` and
  `fuzz/Cargo.lock`, and opens a release PR against `develop`. `release-tag` verifies
  the release commit is on `develop` and CI is green, then merges to `main`, pushes the
  version tag, and triggers the publish workflow.
- Fixed coverage ratchet CI: added `LC_NUMERIC=C` to the baseline `printf` to prevent
  locale-dependent decimal separators from corrupting `.coverage-baseline` on non-C locales.

## [0.4.0] - 2026-03-25

### Breaking changes

- **`VideoMode::with_detailed_timing` new first parameter**: `display-types` 0.2.0 adds
  `pixel_clock_khz: u32` as the first argument. Any caller that constructs a `VideoMode` via
  `with_detailed_timing` must insert the pixel clock value (in kHz) before the existing
  porch/sync arguments.

### Added

- **Pixel clock now populated for detailed-timing modes**: `VideoMode::pixel_clock_khz` is now
  set for modes decoded from Detailed Timing Descriptors. Previously it was only set for
  VIC-keyed and DMT-keyed modes. Affected sources:
  - EDID 18-byte Detailed Timing Descriptor — pixel clock read in 10 kHz units and stored as kHz
  - DisplayID Type I Short Descriptor Video Timings — same 10 kHz → kHz conversion
  - DisplayID Type II Detailed Video Timing — same 10 kHz → kHz conversion
  - DisplayID Type VI Detailed Video Timing — pixel clock already in kHz, stored directly

### Changed

- Updated `display-types` dependency from `0.1.3` to `0.2.0`.

### Internal

- **Coverage ratchet**: CI now measures line coverage across the `std` and `std + serde`
  feature sets using `cargo-llvm-cov`. The baseline is stored in `.coverage-baseline`
  (currently 93.83%); CI fails if coverage drops more than 0.1% below it. On pushes to
  `main` or `develop`, coverage improvements are committed automatically via a
  `ci/coverage-ratchet` PR.

## [0.3.2] - 2026-03-22

### Changed

- Updated `display-types` dependency from `0.1.2` to `0.1.3`. `DisplayIdCapabilities`
  is now provided by the shared crate and re-exported from `piaf` as before. DisplayID
  block tag constants (`displayid::tag`) and product type constants (`displayid::product_type`)
  are also now available directly from `display-types`.

## [0.3.1] - 2026-03-22

### Changed

- Updated `display-types` dependency from `0.1.1` to `0.1.2`. All CEA-861 output types,
  lookup tables, and OUI constants are now provided by the shared crate and re-exported
  from `piaf` as before. Downstream crates can access these directly via `display-types`
  without depending on `piaf`.

## [0.3.0] - 2026-03-22

### Breaking changes

**Shared type library**

All display capability types have been extracted into the
[`display-types`](https://crates.io/crates/display-types) crate (version 0.1.1), which is now a
required dependency. Types that were previously defined in `piaf` are now re-exported from
`display-types`; existing `use piaf::…` imports continue to work, but the types themselves now
originate from `display-types`.

The following types are affected:
`DisplayCapabilities`, `ExtensionData`, `ParseWarning`, `EdidVersion`,
`VideoMode`, `StereoMode`, `SyncDefinition`,
`Chromaticity`, `ChromaticityPoint`, `WhitePoint`, `ColorManagementData`, `DcmChannel`,
`ColorBitDepth`, `DigitalColorEncoding`, `AnalogColorType`, `DisplayGamma`,
`VideoInputFlags`, `VideoInterface`, `AnalogSyncLevel`,
`DisplayFeatureFlags`,
`ManufacturerId`, `ManufactureDate`, `MonitorString`,
`ScreenSize`,
`TimingFormula`, `GtfSecondaryParams`, `CvtSupportParams`, `CvtAspectRatios`, `CvtAspectRatio`, `CvtScaling`,
`BacklightType`, `DisplayIdInterface`, `DisplayIdStereoInterface`, `DisplayIdTiledTopology`,
`DisplayInterfaceType`, `DisplayTechnology`, `InterfaceContentProtection`, `OperatingMode`,
`PhysicalOrientation`, `PowerSequencing`, `RotationCapability`, `ScanDirection`,
`StereoSyncInterface`, `StereoViewingMode`, `SubpixelLayout`, `TileBezelInfo`,
`TileTopologyBehavior`, `ZeroPixelLocation`,
`TransferPointEncoding`, `TransferCurve`, `DisplayIdTransferCharacteristic`.

**`#[non_exhaustive]` on all output structs**

All public output structs are now marked `#[non_exhaustive]`, including `VideoMode`. Code that
constructs these structs with struct literal syntax must switch to the provided `::new(…)`
constructors. `VideoMode` specifically provides:

- `VideoMode::new(width, height, refresh_rate, interlaced)` — for modes from established
  timings, standard timings, and SVDs
- `VideoMode::with_detailed_timing(h_front_porch, h_sync_width, v_front_porch, v_sync_width,
  h_border, v_border, stereo, sync) -> Self` — builder for DTD-sourced modes, chained after
  `new`

**`serde` feature now requires `display-types/serde`**

Enabling the `serde` feature now also enables `display-types/serde`. Projects that previously
activated serde serialization by enabling only `piaf/serde` will automatically get serialization
for all re-exported types as before, with no action required.

### Fixed

- **Bare `no_std` builds**: Both `capabilities_from_edid` and `capabilities_from_edid_static`
  failed to compile in bare `no_std` mode after the type extraction. The root cause was that
  `decode_base_block` gained a `warn: &mut dyn ModeSink` parameter to route warnings in the
  absence of heap allocation, but the call sites had not been updated.
  `capabilities_from_edid_static` now initialises `StaticDisplayCapabilities` before the
  base-block decode and passes it directly as the warning sink, so base-block warnings (e.g.
  `InvalidManufacturerId`) are preserved in `StaticDisplayCapabilities::warnings` exactly as
  they were before the type extraction. `capabilities_from_edid` uses a local `NullSink`
  because `DisplayCapabilities` carries no warning storage in bare `no_std` builds; use
  `capabilities_from_edid_static` if warnings are needed in that build configuration.
- **Transfer Characteristics block with reserved encoding**: a DisplayID Transfer
  Characteristics block whose encoding byte carried the reserved value `0b11` was silently
  ignored. It now emits `EdidWarning::UnknownTransferEncoding(bits)` before skipping the
  block, consistent with how other undecodable fields are handled.

### Added

- **`display-types` dependency**: `display-types = "0.1.1"` is now a required dependency,
  providing the stable shared vocabulary between piaf and downstream consumers such as
  [concordance](https://crates.io/crates/concordance).
- **`EdidWarning::UnknownTransferEncoding(u8)`**: new warning variant emitted when a DisplayID
  Transfer Characteristics block carries a reserved encoding byte.
- **Code of conduct**: `CODE_OF_CONDUCT.md` added (Contributor Covenant 3.0).

### Internal

- `decode_base_block` gained a `warn: &mut dyn ModeSink` parameter so warnings can be routed
  to the appropriate sink in bare `no_std` builds where `DisplayCapabilities` has no warning
  storage. `NullSink` unified into a single module-level definition.
- `decode_color_bit_depth` and `decode_manufacture_date` re-exported from `capabilities::base`
  so `displayid::metadata` can import them without reaching into the private `header` submodule.
- Decoder methods that were `pub(crate)` on shared types in display-types have been moved to
  free functions inside piaf, keeping the public API of display-types free of parser internals.
- All `VideoMode` construction sites migrated from struct literal syntax to `VideoMode::new` /
  `VideoMode::with_detailed_timing`.
- Unused imports removed following the type extraction.

## [0.2.1] - 2026-03-21

### Fixed

- **alloc-only build** (`--no-default-features --features alloc`): `Vec` was not in scope inside
  the nested `unpack8`/`unpack10`/`unpack12` helpers in the DisplayID transfer characteristics
  decoder. The alloc build silently failed to compile; now covered by CI.

### Added

- **Serde round-trip tests** (`tests/serde.rs`, requires `--features serde`): 31 tests covering
  all public types that derive `Serialize`/`Deserialize`, including panel enums, panel structs
  (constructed via `serde_json::from_str` to work around `#[non_exhaustive]`), transfer types,
  and a fixture-based smoke test of `DisplayCapabilities`.
- **Fuzz targets**: a second target `capabilities_static` exercises the no-alloc pipeline
  (`capabilities_from_edid_static<64>`). The `parse_edid` target is updated to use
  `ExtensionLibrary::with_standard_handlers()` consistently for both parse and capabilities calls.
  The empty `fuzz_target_1` placeholder is removed.
- **Fuzz corpus**: real EDID fixture files seeded into both `fuzz/corpus/parse_edid/` and
  `fuzz/corpus/capabilities_static/`, providing 256-byte inputs with CEA-861 extension blocks
  previously absent from the all-128-byte generated corpus.
- **Fuzz CI workflow** (`.github/workflows/fuzz.yml`): 60-second smoke run on every push and pull
  request; 1-hour deep run on a weekly schedule (Saturdays 02:00 UTC) and on manual dispatch.
  Corpus is cached between runs; crash artifacts are uploaded automatically on failure.

### Documentation

- README: added **DisplayID 1.x coverage** table listing all 20 decoded block types (tags
  `0x00`–`0x13`) with their output fields.
- `doc/testing.md`: updated fuzzing section to cover both targets, the long-campaign workflow
  (`-max_total_time=3600` followed by `fuzz cmin`), and the CI setup.

### Internal

- `#[non_exhaustive]` added to all public enums and output structs. Exhaustive matches in
  `examples/capture_fixture.rs` and `examples/inspect_displays.rs` updated accordingly.
- `examples/inspect_displays.rs` updated to print all new DisplayID panel fields.
- alloc-only build step added to CI (`cargo build --no-default-features --features alloc`).

## [0.2.0] - 2026-03-21

### Breaking changes

**Handler trait signatures**
- `ExtensionHandler::process` now receives `blocks: &[&[u8; 128]]` (all blocks matching the
  handler's registered tag, in stream order) instead of a single `block: &[u8; 128]`. Update
  implementations to iterate or index into the slice; single-block handlers can use `blocks[0]`.
- `StaticExtensionHandler::process` now receives `blocks: &[&[u8; 128]]` and
  `ctx: &mut StaticContext<'_>` instead of `block: &[u8; 128]` and `sink: &mut dyn ModeSink`.
  Replace `sink` with `ctx` at call sites; `StaticContext` implements `ModeSink` and exposes the
  same `push_mode` / `push_warning` methods.

**`EdidWarning` new variants**
- Four new variants were added: `DisplayIdVersionUnknown`, `DisplayIdExtensionCountMismatch`,
  `DisplayIdChecksumMismatch`, `DisplayIdSectionBytesOutOfRange`. Exhaustive `match` arms on
  `EdidWarning` must be updated.

**`DisplayCapabilities` and `StaticDisplayCapabilities` new fields**
- Both structs gained many new `Option` fields for DisplayID data. Code that constructs these
  structs with a struct literal (rather than through the library) must add the new fields or
  use `..Default::default()`.

### Added

**DisplayID 1.x extension**
- `DisplayIdHandler` and `DISPLAYID_HANDLER` — built-in handler for EDID extension tag `0x70`
- `STANDARD_HANDLERS` now includes `DisplayIdHandler` alongside `Cea861Handler`
- `DisplayIdCapabilities` — decoded view of a full DisplayID section (alloc / std only)
- Fragment reassembly: consecutive `0x70` extension blocks are collected and processed as a
  single unit; the `section_byte_count` continuation mechanism is fully implemented
- Checksum verification per section with `DisplayIdChecksumMismatch` warning on failure
- Full coverage of DisplayID 1.x data blocks:
  - **0x01** Display Parameters Block — native resolution, range limits, aspect ratio, audio,
    interlacing, deinterlacing, contiguous frequency flag
  - **0x02** Color Characteristics Block — primary / white point CIE xy coordinates and bit depth
  - **0x03** Product Identification Block — manufacturer ID, product code, serial number, name
  - **0x04** Type I Short Descriptor Video Timings
  - **0x05** CTA Video Timing Block — VIC-keyed timings with native / preferred flags
  - **0x06** VESA Video Timing Block — DMT-ID keyed timings
  - **0x07** Type II Detailed Video Timing
  - **0x08** Type III Short Video Timing
  - **0x09** Type IV Short Video Timing
  - **0x0A** ASCII String Block
  - **0x0B** Product Serial Number Block
  - **0x0C** Display Device Data Block — technology, operating mode, backlight, native pixels,
    aspect ratio, orientation, rotation, scan direction, sub-pixel layout, pixel pitch,
    pixel response time, DE signal polarity
  - **0x0D** Interface Power Sequencing Block — T1–T5 timing parameters
  - **0x0E** Transfer Characteristics Block — luminance curve as an ordered list of encoded
    points (`DisplayIdTransferCharacteristic`, alloc / std only)
  - **0x0F** Display Interface Data Block — interface type, number of lanes/links, content
    protection, color depth per channel, spread-spectrum support
  - **0x10** Stereo Display Interface Data Block — viewing mode, sync interface, frame rate,
    polarity, pattern, eye-separation
  - **0x11** Video Timing Range Descriptor Block — minimum and maximum H/V rates, pixel clock range
  - **0x12** Tiled Display Topology Data Block — tile position/size, total tiled display size,
    pixel overlap, bezel width, topology ID, single-enclosure flag

**New public types (all in `piaf::` unless noted)**
- Panel types: `BacklightType`, `DisplayIdInterface`, `DisplayIdStereoInterface`,
  `DisplayIdTiledTopology`, `DisplayInterfaceType`, `DisplayTechnology`,
  `InterfaceContentProtection`, `OperatingMode`, `PhysicalOrientation`, `PowerSequencing`,
  `RotationCapability`, `ScanDirection`, `StereoSyncInterface`, `StereoViewingMode`,
  `SubpixelLayout`, `TileBezelInfo`, `TileTopologyBehavior`, `ZeroPixelLocation`
- Transfer types: `TransferPointEncoding`; `DisplayIdTransferCharacteristic`, `TransferCurve`
  (alloc / std only)
- `StaticContext` — output context passed to `StaticExtensionHandler::process`; wraps a
  `ModeSink` and is extensible without changing the trait signature

**New `DisplayCapabilities` fields** (all `Option`, default `None`)
- `display_technology`, `display_subtype`, `operating_mode`, `backlight_type`,
  `data_enable_used`, `data_enable_positive`, `native_pixels`, `panel_aspect_ratio_100`,
  `physical_orientation`, `rotation_capability`, `zero_pixel_location`, `scan_direction`,
  `subpixel_layout`, `pixel_pitch_hundredths_mm`, `pixel_response_time_ms`,
  `power_sequencing`, `transfer_characteristic` (alloc / std only),
  `display_id_interface`, `stereo_interface`, `tiled_topology`

**Test fixtures and developer experience**
- `capture_fixture` example — captures EDID binaries from connected displays
- Two new real-world test fixtures: `philips_ftv_phl.bin`, `phl_275e1_phl.bin`
- CI now publishes to crates.io on tag push

---

## [0.1.0] - 2026-03-21

### Added

**Core EDID parsing**
- `ParsedEdid` struct with `DisplayCapabilities`, `EdidWarnings`, and extension data
- `parse_edid` entry point with extensible handler and warning infrastructure
- Manufacturer ID, product code, serial number, manufacture week and year
- Video input definition with bit flags, color bit depth, color type (analog/digital)
- Physical display dimensions and gamma
- Established timings bitmap, standard timings, and 18-byte descriptor block parsing
- Chromaticity coordinates, white point descriptors, and color management data
- Preferred image size, interlacing, CVT, and range limits in `VideoMode`
- Horizontal/vertical front porch and sync width in `VideoMode`
- DTD parsing with out-of-range refresh rate handling

**CEA-861 / CTA-861 extension**
- Full VIC lookup table covering VICs 1–255 (including extended VICs 128–255)
- CEA Audio Descriptors, HDMI SPA, Max TMDS, Deep Color
- Video capability, colorimetry standards, HDR EOTFs/luminance metadata
- HDMI 2.0 Vendor-Specific Data Block (VSDB) and HDMI Forum Sink Capability Data Block
- HDMI Forum EDID Extension Override
- VESA Display Data Block (DDDB) and VTB-EXT
- Type VII (T7VTDB), Type VIII (T8VTDB with full VESA DMT 1.13 table), and Type X (T10VTDB) Video Timing Data Blocks
- VESA Vendor-Specific Audio/Video Data Blocks (VSADB, VSVDB)
- VESA Transfer Characteristic Data Block
- Spatial audio and info frame data decoding
- CEA-861 flags exposed as `bitflags` with `serde` support

**Static / zero-copy pipeline**
- `StaticDisplayCapabilities<const MAX_MODES: usize>` for fixed-size environments
- `ModeSink` trait and `EdidSource` for zero-copy implementation
- `StaticExtensionHandler` trait with `KnownExtensions` for slice-backed dispatch
- `StaticContext` and `capabilities_from_edid_static` entry point

**`no_std` support**
- Full `no_std` + `alloc` build via `std` and `alloc` feature flags
- Serial number string, display name, and unspecified text in `no_std` builds via newtype pattern
- Warnings emitted in `no_std` builds
- `Vec` replaced with slice-compatible patterns for `no_std` compatibility

**Robustness and safety**
- `EdidError` enum via `thiserror`; extensible warnings infrastructure
- Handlers can emit warnings
- Hardened manufacturer ID parsing and early return on malformed DTDs
- Overflow-safe extension block counting with `checked_mul`
- `#![deny(unsafe_code)]`
- Fuzz testing corpus and `cargo fuzz` integration

**Developer experience**
- Example script demonstrating full field extraction
- Test fixtures for real-world EDID binaries with fixture capture script
- Full rustdoc coverage
- CI pipeline with `cargo test`, `cargo rustdoc`, and `cargo fmt`
- Published to crates.io with docs.rs integration
