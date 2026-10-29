# DisplayID 2.x

DisplayID 2.x (version byte `0x20`) is the successor to DisplayID 1.x. It uses the same
extension tag (`0x70`), the same 4-byte section header, and the same multi-fragment
reassembly model — see [DisplayID Extension Handler](./displayid-handler.md) for the
section-level mechanics — but defines a **disjoint data block tag space** at `0x20`–`0x29`
(metadata + timings), `0x7E` (vendor-specific), and `0x81` (CTA DisplayID).

A single `DisplayIdHandler` decodes both 1.x and 2.x sections; dispatch is selected by the
section header's version byte. 1.x and 2.x blocks never appear in the same section: a
section is wholly one or the other, and the handler ignores tags from the wrong tag space.

## Tag assignments

| Tag           | Block                                            | Pipeline       |
|---------------|--------------------------------------------------|----------------|
| `0x20`        | Product Identification Block                     | dynamic        |
| `0x21`        | Display Parameters Block                         | dynamic        |
| `0x22`        | Type VII Detailed Timing Block                   | dynamic+static |
| `0x23`        | Type VIII Enumerated Timing Code Block           | dynamic+static |
| `0x24`        | Type IX Formula-Based Timing Block               | dynamic+static |
| `0x25`        | Dynamic Video Timing Range Limits Block          | dynamic        |
| `0x26`        | Display Interface Features Block                 | dynamic        |
| `0x27`        | Stereo Display Interface Block                   | dynamic        |
| `0x28`        | Tiled Display Topology Block                     | dynamic        |
| `0x29`        | ContainerID Block                                | dynamic        |
| `0x2A`–`0x7D` | Reserved by DisplayID 2.x                        | —              |
| `0x7E`        | Vendor-Specific Block                            | dynamic        |
| `0x7F`–`0x80` | Reserved by DisplayID 2.x                        | —              |
| `0x81`        | CTA DisplayID Block (wraps a CTA-861 collection) | dynamic+static |
| `0x82`–`0xFF` | Reserved by DisplayID 2.x                        | —              |

The "Pipeline" column distinguishes blocks decoded only in the dynamic
(`alloc`/`std`) pipeline from those that also produce video modes through the static
pipeline. Metadata-only blocks are dynamic-only.

## Where decoded data lands

DisplayID 2.x writes into three places:

- **`DisplayCapabilities`** — version-agnostic scalar fields (`product_code`,
  `manufacture_date`, `display_name`, `native_pixels`, `color_bit_depth`,
  `max_pixel_clock_mhz`, `min_v_rate`, `max_v_rate`, etc.).
  These are the same fields the EDID base block, CEA-861, and DisplayID 1.x write to;
  later sources overwrite earlier values for the same field.
- **`DisplayIdCapabilities`** (extension tag `0x70`) — DisplayID-specific records
  including the Display Parameters image size (`image_size`) and 2.x-only structures
  (`manufacturer_oui`, `display_params_v2`,
  `dynamic_timing_range`, `interface_features`, `stereo_interface_v2`, `container_id`,
  `vendor_specific`).
- **`Cea861Capabilities`** (extension tag `0x02`) — only via the `0x81` CTA DisplayID
  Block. See [§ 0x81 CTA DisplayID Block](#cta-displayid-block-0x81-merge-with-cea-861-data).

`caps.supported_modes` accumulates video modes from timing blocks (`0x22`, `0x23`,
`0x24`) and from any CTA-861 timing data carried inside `0x81`.

Note that `caps.manufacturer` (PNP-derived) is **not** populated by DisplayID 2.x.
The 2.x Product Identification Block uses an IEEE OUI, which has no defined mapping
to the 3-letter PNP namespace; the OUI is exposed separately as
`did.manufacturer_oui`.

## Extracted identification data

### Product Identification Block (`0x20`)

Fields decoded into `DisplayCapabilities` and `DisplayIdCapabilities`:

| Field                   | Source                                                                 |
|-------------------------|------------------------------------------------------------------------|
| `did.manufacturer_oui`  | Bytes 0–2 — IEEE OUI, 3 raw bytes (high byte first)                    |
| `caps.product_code`     | LE uint16 at bytes 3–4                                                 |
| `caps.serial_number`    | LE uint32 at bytes 5–8; `0` is treated as unspecified                  |
| `caps.manufacture_date` | Bytes 9–10 (`year = byte + 2000`; week `0xFF` = model year)            |
| `caps.display_name`     | Length byte at 11; ASCII / ISO 8859-1 starting at 12 (up to 13 stored) |

Each field is written only when the payload is long enough to contain it. Names longer
than 13 bytes are truncated to fit `MonitorString`.

### Display Parameters Block (`0x21`)

Fields decoded into `DisplayCapabilities` and `DisplayIdCapabilities`:

| Field                          | Source                                                                                                                                                                    |
|--------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `did.image_size`               | Bytes 0–3, LE uint16 pairs, stored with their unit: 0.1 mm or 1 mm depending on revision bit 7                                                                            |
| `caps.native_pixels`           | Bytes 4–7, LE uint16 pairs; `None` when either axis is `0`                                                                                                                |
| `caps.color_bit_depth`         | Byte 27 bits 2:0 (`1=6 bpc … 5=16 bpc`); 14 bpc is not representable in this encoding                                                                                     |
| `did.display_params_v2`        | 12-bit chromaticities (3 primaries + white), max/10%/min luminance (binary16),         display technology, gamma, scan orientation, audio routing, CIE coordinate variant |

The block is fixed at 29 bytes; shorter payloads are silently skipped.

Luminance fields use the IEEE 754 binary16 encoding. The spec reserves `−0` (`0x8000`)
as the "not used" sentinel; this decoder also treats `+0` (`0x0000`) as `None` because
0 cd/m² is degenerate for any of the three luminance fields and is most likely an EDID
writer that confused the sign bit. NaN and infinity decode to `None` as well.

### Dynamic Video Timing Range Limits Block (`0x25`)

Fields decoded into `DisplayCapabilities` and `DisplayIdCapabilities`:

| Field                      | Source                                                                                                                          |
|----------------------------|---------------------------------------------------------------------------------------------------------------------------------|
| `did.dynamic_timing_range` | Min/max pixel clock (kHz, 24-bit LE), min/max vertical refresh (Hz; max widened to 10 bits on revision ≥ 1), VRR-supported flag |
| `caps.max_pixel_clock_mhz` | `did.dynamic_timing_range.max_pixel_clock_khz / 1000`, capped to `u16::MAX`                                                     |
| `caps.min_v_rate`          | `did.dynamic_timing_range.min_v_rate_hz` (skipped when `0`)                                                                     |
| `caps.max_v_rate`          | `did.dynamic_timing_range.max_v_rate_hz` (skipped when `0`)                                                                     |

Pixel clock is downconverted to MHz for `caps.max_pixel_clock_mhz`; sub-MHz precision
is preserved on `did.dynamic_timing_range.max_pixel_clock_khz`.

### Display Interface Features Block (`0x26`)

| Field                    | Source                                                                                                                                                                  |
|--------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `did.interface_features` | RGB / YCbCr 4:4:4 / 4:2:2 / 4:2:0 color depth bitmasks, minimum 4:2:0 pixel rate (in 74.25 MP/s units), audio capability flags, color space + EOTF combinations bitmask |

The trailing custom-combination bytes (bytes 7–8) and any "additional bytes" extension
are not currently decoded.

### Stereo Display Interface Block (`0x27`)

| Field                     | Source                                                                                                                                                                                                               |
|---------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `did.stereo_interface_v2` | Method byte → `StereoViewingMethodV2` (Field Sequential, Side-by-Side, Pixel Interleaved, Dual Interface, Multi-View, Stacked Frame, Proprietary, or `Reserved(byte)`); timing scope from revision byte upper 2 bits |

The optional inline timing-code list (when timing scope indicates per-method codes)
is not currently decoded.

### Tiled Display Topology Block (`0x28`)

Wire format is identical to the DisplayID 1.x `0x12` block, and decoding goes through
the same path. Fields land on `caps.tiled_topology` — see
[DisplayID Extension Handler § Tiled Display Topology](./displayid-handler.md#extracted-identification-data)
for the field map.

### ContainerID Block (`0x29`)

| Field                       | Source                                                                                |
|-----------------------------|---------------------------------------------------------------------------------------|
| `did.container_id`          | Bytes 0–15, raw 16-byte UUID (typically a Microsoft-style ContainerID GUID)           |

Endianness is preserved as-is; consumers decide between mixed-endian (classic GUID
layout) and big-endian (RFC 4122) interpretation. Payloads shorter than 16 bytes are
skipped.

### Vendor-Specific Block (`0x7E`)

| Field                       | Source                                                                                |
|-----------------------------|---------------------------------------------------------------------------------------|
| `did.vendor_specific`       | Each block appends one record: bytes 0–2 = OUI, bytes 3+ = opaque vendor payload      |

Multiple `0x7E` blocks are allowed in a single section; each is appended in payload
order. Payloads shorter than 3 bytes (no full OUI) are skipped.

## CTA DisplayID Block (`0x81`): merge with CEA-861 data

The `0x81` block wraps a CTA-861 data block collection — the same byte sequence that
appears between byte 4 and the DTD region of a CEA-861 (`0x02`) extension block, but
without the section header flags or the trailing DTDs. Each entry has a 1-byte CTA
header (`tag << 5 | length`) followed by `length` payload bytes; a zero header byte
ends scanning.

Decoding is delegated to the same `parse_cea861_data_block_collection` helper used by
the CEA-861 handler. Decoded entries are merged into the existing
`Cea861Capabilities` extension data (extension tag `0x02`) via a take-mutate-restore
pattern, so `0x81`-derived data combines with any data parsed from a real CEA-861
extension block on the same EDID — **regardless of which extension is processed
first**. The behaviour:

- VICs and timing-bearing extended-tag blocks contribute video modes to
  `caps.supported_modes` with the same `(width, height, refresh_rate, interlaced)`
  dedup the CEA-861 path uses; modes already present (from a base-block DTD,
  DisplayID timing block, or CEA-861 SVD) are not duplicated.
- All CTA-specific state — audio descriptors, vendor-specific blocks, HDR static and
  dynamic metadata, colorimetry, video capability, infoframe descriptors, room
  configuration, speaker locations, T7/T8/T10VTDB entries, Y420 VDB / capability map,
  HDMI Forum SCDB / EEODB, HDMI audio block — appends to the same
  `Cea861Capabilities` instance.
- "First-only" extended-tag fields (`video_capability`, `colorimetry`,
  `hdr_static_metadata`, `video_format_preferences`, `y420_capability_map`,
  `room_configuration`, `hf_eeodb_extension_count`, `hf_scdb`, `hdmi_audio`,
  `speaker_allocation`, `vesa_transfer_characteristic`, `vesa_display_device`,
  `hdmi_vsdb`, `hf_vsdb`) follow the existing intra-CEA "first wins" rule: whichever
  source populates the field first owns it.
- `Cea861Capabilities::flags` is `Cea861Flags::empty()` for `0x81`-derived caps;
  when a real CEA-861 extension also exists, its flags byte is OR-ed in.

Static pipeline: only mode-producing entries reach `StaticDisplayCapabilities`. Audio,
HDR, and other CTA metadata require the dynamic pipeline.

## Extracted timing data

### Type VII Detailed Timing Block (`0x22`)

Each 20-byte descriptor maps to a `VideoMode` with full timing detail. The encoding
matches DisplayID 1.x Type I except the pixel clock is widened to 24 bits (1 kHz
units, not 10 kHz), allowing rates beyond 655 MHz.

| `VideoMode` field               | Source                                                  |
|---------------------------------|---------------------------------------------------------|
| `width`, `height`               | Horizontal/Vertical Active (LE uint16)                  |
| `refresh_rate`                  | Derived: `pixel_clock_hz / (h_total × v_total)`         |
| `pixel_clock_khz`               | Bytes 0–2, 24-bit LE (1 kHz steps)                      |
| `interlaced`                    | Byte 3 bit 4                                            |
| `h_front_porch`, `h_sync_width` | Bytes 6–11 (LE uint16 pairs)                            |
| `v_front_porch`, `v_sync_width` | Bytes 14–19 (LE uint16 pairs)                           |
| `sync`                          | `DigitalSeparate`; polarities from byte 3 bits 2–3      |

Null descriptors (pixel clock = 0) are silently skipped.

### Type VIII Enumerated Timing Code Block (`0x23`)

The payload is a list of 1- or 2-byte timing codes resolved via the DMT, CTA-861 VIC,
or HDMI VIC table. Code space and entry width are selected by the data block's
revision byte. Resolved modes carry full timing detail from the lookup tables (modes
sourced from the HDMI VIC table carry only `width`, `height`, `refresh_rate`).

### Type IX Formula-Based Timing Block (`0x24`)

Each 6-byte descriptor encodes width, height, refresh rate, and byte 0 option flags.
Decoded into `VideoMode` as:

| `VideoMode` field          | Source                                                                        |
|----------------------------|-------------------------------------------------------------------------------|
| `width`, `height`          | Bytes 1–2 / 3–4 (LE uint16, stored as `value − 1`)                           |
| `refresh_rate`             | Byte 5: `byte + 1` Hz (range 1–256 Hz)                                       |
| `cvt_algorithm`            | Byte 0 bits 2:0 → `CvtAlgorithm` (`Cvt`, `CvtRb`, `CvtR2`, or `Reserved(b)`) |
| `ntsc_fractional_refresh`  | Byte 0 bit 3                                                                  |
| `type_ix_stereo`           | Byte 0 bits 6:5 → `TypeIxStereoMode` (Mono / Stereo / MonoOrStereoByUser / Reserved) |

When the algorithm is **CVT-RB v1** (`CvtRb`, VESA CVT 1.1 §3.4) or **CVT-R2**
(`CvtR2`, VESA CVT 1.2 §4), the descriptor is expanded to a full timing via
`display_types::compute_type_ix_timing`: `pixel_clock_khz`, `h_front_porch`,
`h_sync_width`, `v_front_porch`, and `v_sync_width` are populated. Standard CVT (`Cvt`)
and `Reserved(_)` produce a minimal `VideoMode` with only the algorithm metadata set.

## Coexistence with DisplayID 1.x

A single physical EDID never carries both 1.x and 2.x sections in the same `0x70`
extension block (the version byte selects exactly one tag space). However, an EDID
may carry multiple `0x70` extension blocks in different versions across separate
sections, and the same `DisplayIdCapabilities` struct accumulates data from all of
them. Field-level conflicts resolve as follows:

- Scalar fields on `DisplayCapabilities` and `DisplayIdCapabilities` follow last-write
  semantics across blocks of either version, in the order extension blocks appear in
  the EDID stream.
- Lists (`did.vendor_specific`, `caps.supported_modes`) accumulate across all sources.

## Warnings

| Variant                                        | Meaning                                                                                                                                                   |
|------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------|
| `DisplayIdVersionUnknown(u8)`                  | Version byte is outside the known ranges (0x10–0x1F, 0x20). The block is skipped.                                                                         |
| `UnsupportedV2BlockRevision { tag, revision }` | A 2.x data block carries a revision byte the spec marks as reserved. The block is parsed anyway with the revision-0 wire format; values may be incorrect. |

The section-level warnings (`DisplayIdExtensionCountMismatch`,
`DisplayIdChecksumMismatch`, `DisplayIdSectionBytesOutOfRange`) are shared with 1.x —
see [DisplayID Extension Handler § Warnings](./displayid-handler.md#warnings).

## Fragment and data block layout

The 4-byte section header and the 3-byte data block header are identical to 1.x — see
[DisplayID Extension Handler § Fragment layout reference](./displayid-handler.md#fragment-layout-reference).
The version byte at fragment offset 1 is the only thing that distinguishes a 2.x
section from a 1.x section.
