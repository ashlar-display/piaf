# Data model

PIAF keeps a clear separation between parsed source data and normalized capability output.

## Parsed representation

Two types represent the parsed EDID. Both implement `EdidSource` and are accepted by the
capability pipelines.

**`ParsedEdidRef<'a>`** — the zero-copy output of `parse_edid`. Borrows block data directly
from the input slice; no bytes are copied. Extension blocks are accessible at all build tiers
including bare `no_std`, because they are borrowed rather than allocated.

```rust
pub struct ParsedEdidRef<'a> {
    pub base_block: &'a [u8; 128],
    pub num_extensions: usize,
    // alloc/std: pub warnings: Vec<ParseWarning>,
    // no_std:    pub warnings: [Option<EdidWarning>; 8],
}
```

**`ParsedEdid`** — the owned output of `parse_edid_owned`. Copies block bytes out of the
input so the result can outlive the input buffer.

```rust
pub struct ParsedEdid {
    pub base_block: [u8; 128],
    // alloc/std only:
    pub extensions: Vec<[u8; 128]>,
    pub warnings: Vec<ParseWarning>,
}
```

In bare `no_std` builds (no `alloc`), `ParsedEdid.extensions` and `ParsedEdid.warnings` are
absent. Prefer `ParsedEdidRef` in bare `no_std` when extension block access is needed — it
retains the borrowed extension bytes without requiring an allocator.

Both structures are useful for:

- debugging,
- inspecting exact decoded content,
- preserving information that may not fit neatly into a simplified model,
- supporting future extensions.

## Capability representation

PIAF provides two output types depending on the pipeline used.

### `DisplayCapabilities` (dynamic pipeline, `alloc`/`std`)

The consumer-facing output for the dynamic pipeline. Fields are `Option` where the source
data may be absent or undecodable. The `extension_data` field allows handlers to attach
typed custom data without modifying the struct.

```rust
pub struct DisplayCapabilities {
    // Identity
    pub manufacturer: Option<ManufacturerId>,
    pub manufacture_date: Option<ManufactureDate>,
    pub edid_version: Option<EdidVersion>,
    pub product_code: Option<u16>,
    pub serial_number: Option<u32>,
    pub serial_number_string: Option<MonitorString>,
    pub display_name: Option<MonitorString>,
    pub unspecified_text: [Option<MonitorString>; 4],
    // Input
    pub digital: bool,
    pub color_bit_depth: Option<ColorBitDepth>,
    pub video_interface: Option<VideoInterface>,
    pub analog_sync_level: Option<AnalogSyncLevel>,
    // Color
    pub chromaticity: Chromaticity,
    pub gamma: Option<DisplayGamma>,
    pub display_features: Option<DisplayFeatureFlags>,
    pub digital_color_encoding: Option<DigitalColorEncoding>,
    pub analog_color_type: Option<AnalogColorType>,
    pub color_management: Option<ColorManagementData>,
    pub white_points: [Option<WhitePoint>; 2],
    // Physical
    pub screen_size: Option<ScreenSize>,
    pub preferred_image_size_mm: Option<(u16, u16)>,
    // Timing
    pub min_v_rate: Option<u16>,
    pub max_v_rate: Option<u16>,
    pub min_h_rate_khz: Option<u16>,
    pub max_h_rate_khz: Option<u16>,
    pub max_pixel_clock_mhz: Option<u16>,
    pub timing_formula: Option<TimingFormula>,
    // alloc/std only:
    pub supported_modes: Vec<VideoMode>,
    // Panel (from DisplayID, alloc/std only):
    pub display_technology: Option<DisplayTechnology>,
    pub operating_mode: Option<OperatingMode>,
    pub backlight_type: Option<BacklightType>,
    pub native_pixels: Option<(u16, u16)>,
    pub physical_orientation: Option<PhysicalOrientation>,
    pub rotation_capability: Option<RotationCapability>,
    pub zero_pixel_location: Option<ZeroPixelLocation>,
    pub scan_direction: Option<ScanDirection>,
    pub subpixel_layout: Option<SubpixelLayout>,
    pub pixel_pitch_hundredths_mm: Option<u16>,
    pub pixel_response_time_ms: Option<u8>,
    pub data_enable_used: Option<bool>,
    pub panel_aspect_ratio_100: Option<u32>,
    pub power_sequencing: Option<PowerSequencing>,
    pub transfer_characteristic: Option<DisplayIdTransferCharacteristic>, // alloc/std only
    pub display_id_interface: Option<DisplayIdInterface>,
    pub stereo_interface: Option<DisplayIdStereoInterface>,
    pub tiled_topology: Option<DisplayIdTiledTopology>,
    // Diagnostics and extension data (alloc/std only):
    pub warnings: Vec<ParseWarning>,
    pub extension_data: Vec<(u8, Arc<dyn ExtensionData>)>,
}
```

Rate fields (`min_v_rate`, `max_v_rate`, `min_h_rate_khz`, `max_h_rate_khz`) are `u16`
rather than `u8` because the `0xFD` range limits descriptor can add a 255-unit offset to
extend beyond the 8-bit range.

### `StaticDisplayCapabilities<const MAX_MODES: usize>` (static pipeline, all tiers)

The output type for `capabilities_from_edid_static`. Contains all the same scalar fields as
`DisplayCapabilities` (same names, same types), plus a fixed-capacity array for modes and
private, fixed-capacity warning storage:

```rust
pub struct StaticDisplayCapabilities<const MAX_MODES: usize> {
    // All scalar fields identical to DisplayCapabilities
    pub manufacturer: Option<ManufacturerId>,
    // ...

    // Mode storage
    pub supported_modes: [Option<VideoMode>; MAX_MODES],
    pub num_modes: usize,

    // Warning storage (private; see the accessors below)
}
```

Access modes through the iterator rather than indexing directly; warnings are only
reachable through accessors, which are the same in every build:

```rust
for mode in caps.iter_modes() { /* &VideoMode */ }
for warn in caps.iter_warnings() { /* &EdidWarning */ }
let all: &[EdidWarning] = caps.warnings();
let lost: u16 = caps.warnings_dropped(); // beyond MAX_STATIC_WARNINGS (8)
```

### `VideoMode` construction

`VideoMode` is `#[non_exhaustive]`. Use the constructors provided by `display-types` rather
than struct literal syntax:

```rust
// Simple mode (established timings, standard timings, SVDs):
let mode = VideoMode::new(1920, 1080, 60, false);

// Full DTD mode with pixel clock, blanking-interval, and sync fields:
let mode = VideoMode::new(1920, 1080, 60, false)
    .with_detailed_timing(148500, 88, 44, 4, 5, 0, 0, StereoMode::None,
                          Some(SyncDefinition::DigitalSeparate {
                              h_sync_positive: true,
                              v_sync_positive: true,
                          }));
```

All other fields (`pixel_clock_khz`, `h_front_porch`, `h_sync_width`, etc.) default to
`None` / `0` when not set via `with_detailed_timing`. This matches the sparse data available from non-DTD sources
such as SVDs and standard timing entries.

Modes beyond capacity are silently dropped. Warnings beyond `MAX_STATIC_WARNINGS` are not
stored but counted in `warnings_dropped()`. 64 is a reasonable default for `MAX_MODES`.

`StaticDisplayCapabilities` does not have an `extension_data` field. Rich extension metadata
(audio, VSDB, colorimetry, HDR) is only available through the dynamic pipeline.

## Why separate them

A parser-oriented structure and a consumer-oriented structure serve different purposes.

`ParsedEdid` prioritizes fidelity to the source data.

`DisplayCapabilities` prioritizes:

- ease of use,
- semantic clarity,
- stability across parser improvements.

Trying to use one structure for both usually produces an API that is awkward for everyone.

## Fixed-capacity fields

When a field has a fixed maximum size defined by the spec, it is represented with a
fixed-capacity type rather than a heap-allocated one. This makes the field available in
all build configurations, including bare `no_std` without `alloc`.

The preferred approach for a bounded string is a newtype over a fixed-size byte array with
a `Display` impl:

```rust
pub struct ManufacturerId(pub [u8; 3]);

impl ManufacturerId {
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.0).unwrap_or("???")
    }
}

impl core::fmt::Display for ManufacturerId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}
```

This gives consumers the same ergonomics as a `String` field — `format!("{}", id)`,
`id.as_str()`, `id.to_string()` — without requiring heap allocation.

Fields with a small fixed bound (like `white_points`, which the EDID `0xFB` descriptor
limits to two entries) use `[Option<T>; N]` directly.

Fields that are genuinely variable in length (display name strings, warnings) remain
`#[cfg(any(feature = "alloc", feature = "std"))]` gated in `DisplayCapabilities`. Mode
lists are the exception: `StaticDisplayCapabilities<N>` provides a fixed-capacity
`[Option<VideoMode>; N]` available at all build tiers.

New fields should follow the fixed-capacity pattern where the bound is derivable from
the spec.

## Error and warning model

Errors and warnings are distinct.

```rust
pub enum EdidError {
    InvalidLength,
    InvalidHeader,
    ChecksumMismatch,
}

pub enum EdidWarning {
    /// Extension block tag not in the registered set.
    UnknownExtension(u8),
    /// An 18-byte descriptor slot could not be decoded.
    DescriptorParseFailed,
    /// Manufacturer ID bytes outside the valid PNP range (1–26 per 5-bit field).
    /// `DisplayCapabilities::manufacturer` is left as `None`.
    InvalidManufacturerId,
    /// A data block inside an extension block declared a length that extends past the
    /// end of the data block collection. Remaining data blocks are skipped.
    MalformedDataBlock,
    /// A DTD slot was skipped because the slice was shorter than the required 18 bytes.
    DtdSlotTooShort,
    /// A DTD slot was skipped because the pixel clock value would overflow during
    /// refresh rate calculation. Indicates a malformed or corrupted EDID.
    DtdPixelClockOverflow,
    /// The extension block count declared in the base block exceeds the parser's
    /// safety limit; only the first `limit` blocks were parsed.
    ExtensionBlockLimitReached { declared: usize, limit: usize },
    /// A DisplayID extension block carries an unrecognised version byte.
    DisplayIdVersionUnknown(u8),
    /// The extension count in a DisplayID section header does not match the number
    /// of `0x70`-tagged blocks present in the EDID stream.
    DisplayIdExtensionCountMismatch { declared: u8, found: u8 },
    /// The DisplayID section checksum does not make the section sum to zero.
    DisplayIdChecksumMismatch,
    /// `section_byte_count` in the DisplayID section header is too large to fit
    /// within the 128-byte extension block.
    DisplayIdSectionBytesOutOfRange(u8),
    /// A DisplayID Transfer Characteristics block carries a reserved encoding byte
    /// (bits 7:6 = `0b11`). The block is skipped.
    UnknownTransferEncoding(u8),
    /// Byte slice length differs from `(1 + extension_count) × 128`.
    /// Extra bytes are ignored; too few is a hard `EdidError::InvalidLength`.
    SizeMismatch { expected: usize, actual: usize },
}
```

This separation allows callers to decide how strict they want to be without losing useful
diagnostic detail. Warnings from the parser (including `UnknownExtension` and `SizeMismatch`)
are propagated into `DisplayCapabilities::warnings` alongside handler warnings, so consumers
have a single place to inspect all diagnostics.

### Extensible warnings (`alloc`/`std` builds)

In `alloc`/`std` builds, warnings are type-erased behind a `ParseWarning` alias:

```rust
pub type ParseWarning = Arc<dyn core::error::Error + Send + Sync + 'static>;
```

This means custom extension handlers can push their own error types into the warning list
without wrapping them in `EdidWarning`. The built-in library always emits `EdidWarning`
variants, but a third-party handler that detects a protocol-specific anomaly can emit its
own type directly.

Using `Arc` (rather than `Box`) keeps `ParseWarning` cloneable, which lets warnings be
copied from `ParsedEdid` into `DisplayCapabilities` without consuming the parsed result.

To inspect a specific variant, use `downcast_ref` on the inner error:

```rust
for w in caps.iter_warnings() {
    if let Some(ew) = (**w).downcast_ref::<EdidWarning>() {
        // handle known library warning
    }
}
```

In bare `no_std` builds (without `alloc`) the warning list holds `EdidWarning` values
directly (no type erasure). `StaticDisplayCapabilities` stores up to
`MAX_STATIC_WARNINGS` (8) and counts the rest in `warnings_dropped()`. The `iter_warnings()`
method provides uniform access across both configurations.

