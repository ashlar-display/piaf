# DisplayID Extension Handler

DisplayID (tag `0x70`) is a VESA standard for display identification that carries richer
timing and capability data than the base EDID block. It is most common on DisplayPort
Alt Mode devices, docks, and professional monitors.

## Multi-block sections

Unlike CEA-861, where each 128-byte extension block is self-contained, a single logical
DisplayID section may span several consecutive 128-byte blocks all tagged `0x70`. The
dispatch layer collects all `0x70` blocks in stream order and passes them to
`DisplayIdHandler` as a slice — the handler owns reassembly. No other part of the pipeline
needs to know about DisplayID's multi-block structure.

In `alloc`/`std` builds, full multi-block reassembly is supported. In bare `no_std` builds
(no allocator), extension blocks cannot be stored after parsing, so the static pipeline
receives only base-block data; DisplayID content is unavailable at that tier.

## Dynamic pipeline

`DisplayIdHandler` is registered automatically by `ExtensionLibrary::with_standard_handlers()`.
After calling `capabilities_from_edid`, retrieve the parsed DisplayID section via
`get_extension_data`:

```rust
use piaf::{DisplayIdCapabilities, capabilities_from_edid, parse_edid};

let library = ExtensionLibrary::with_standard_handlers();
let parsed = parse_edid(&bytes, &library)?;
let caps = capabilities_from_edid(&parsed, &library);

if let Some(did) = caps.get_extension_data::<DisplayIdCapabilities>(0x70) {
    println!("DisplayID version: 0x{:02X}", did.version);
    println!("Product type: {}", did.product_type);
}
```

Video modes decoded from DisplayID timing blocks are added to `caps.supported_modes`
alongside modes from the base block and CEA-861.

## Static pipeline

`DisplayIdHandler` is included in `STANDARD_HANDLERS`. It decodes video modes from all
timing blocks (Types I–VI, VESA bitmap, CTA bitmap) and pushes them into the static output:

```rust
use piaf::{STANDARD_HANDLERS, StaticDisplayCapabilities, capabilities_from_edid_static, parse_edid};

let parsed = parse_edid(&bytes, STANDARD_HANDLERS)?;
let caps: StaticDisplayCapabilities<64> = capabilities_from_edid_static(&parsed, STANDARD_HANDLERS);

for mode in caps.iter_modes() {
    println!("{}×{}@{}Hz", mode.width, mode.height, mode.refresh_rate);
}
```

`DisplayIdCapabilities` is not available from the static pipeline — rich metadata requires
the dynamic pipeline.

The block-level decoding documented below covers DisplayID 1.x sections (version
bytes `0x10`–`0x1F`). DisplayID 2.x sections (version byte `0x20`) use the same
section header and reassembly path but a disjoint data block tag space — see
[DisplayID 2.x](./displayid-2x.md) for the 2.x block-by-block field mapping.

## `DisplayIdCapabilities`

Stored under tag `0x70` in `DisplayCapabilities::extension_data`:

| Field          | Type | Description                                                          |
|----------------|------|----------------------------------------------------------------------|
| `version`      | `u8` | Version byte from the section header (0x10–0x1F = v1.x, 0x20 = v2.x) |
| `product_type` | `u8` | Display product primary use case, bits 2:0 of header byte 3          |

## Extracted identification data

**Product Identification Block** (tag `0x00`) is decoded into the following
`DisplayCapabilities` fields (dynamic pipeline only):

| `DisplayCapabilities` field | Source                                                                |
|-----------------------------|-----------------------------------------------------------------------|
| `manufacturer`              | Manufacturer PNP ID, 2-byte packed encoding (same as EDID base block) |
| `product_code`              | LE uint16 at payload bytes 2–3                                        |
| `serial_number`             | LE uint32 at payload bytes 4–7; `0` is treated as unspecified         |
| `manufacture_date`          | Week/year bytes 8–9 (`year = byte + 1990`; week `0xFF` = model year)  |
| `display_name`              | ASCII product name starting at byte 10 (up to 13 bytes stored)        |

**Display Parameters Block** (tag `0x01`) is decoded into the following
`DisplayCapabilities` fields (dynamic pipeline only):

| `DisplayCapabilities` field | Source                                                                        |
|-----------------------------|-------------------------------------------------------------------------------|
| `color_bit_depth`           | Payload byte 5, bits 4:0; same `001=6bpc … 110=16bpc` encoding as EDID `0x14` |

The image size (payload bytes 0–3, LE uint16 pairs, `0` on either axis = not defined) goes to
`DisplayIdCapabilities::image_size` with its unit; `preferred_image_size_mm` belongs to the
base block's preferred timing and is not touched.

**Color Characteristics Block** (tag `0x02`) is decoded into the following
`DisplayCapabilities` field (dynamic pipeline only):

| `DisplayCapabilities` field | Source                                                                                                                |
|-----------------------------|-----------------------------------------------------------------------------------------------------------------------|
| `chromaticity`              | 8 × LE uint16 at payload bytes 0–15: Red x/y, Green x/y, Blue x/y, White x/y; 1/1024 scale, lower 10 bits significant |

Payloads shorter than 16 bytes are silently ignored.

**Video Timing Range Limits Block** (tag `0x09`) is decoded into the following
`DisplayCapabilities` fields (dynamic pipeline only):

| `DisplayCapabilities` field | Source                                            |
|-----------------------------|---------------------------------------------------|
| `max_pixel_clock_mhz`       | Payload bytes 3–5 (LE 24-bit, 10 kHz units ÷ 100) |
| `min_h_rate_khz`            | Payload byte 6                                    |
| `max_h_rate_khz`            | Payload byte 7                                    |
| `min_v_rate`                | Payload byte 10                                   |
| `max_v_rate`                | Payload byte 11                                   |

Fields are written only when the payload is long enough to contain them.

**Product Serial Number Block** (tag `0x0A`) is decoded into the following
`DisplayCapabilities` field (dynamic pipeline only):

| `DisplayCapabilities` field | Source                                                   |
|-----------------------------|----------------------------------------------------------|
| `serial_number_string`      | Payload bytes (ASCII, up to 13 bytes, `0x0A`-terminated) |

**General Purpose ASCII String Blocks** (tag `0x0B`) are decoded into successive
`unspecified_text` slots (dynamic pipeline only). Up to four blocks are stored; extras
are silently dropped.

| `DisplayCapabilities` field | Source                                                        |
|-----------------------------|---------------------------------------------------------------|
| `unspecified_text[0..4]`    | Each `0x0B` block payload (up to 13 bytes, `0x0A`-terminated) |

**Display Device Data Block** (tag `0x0C`) is decoded into the following
`DisplayCapabilities` fields (dynamic pipeline only):

| `DisplayCapabilities` field | Source                                                                    |
|-----------------------------|---------------------------------------------------------------------------|
| `display_technology`        | Byte 0 bits 7:4 — `DisplayTechnology` enum                                |
| `display_subtype`           | Byte 0 bits 3:0 — raw technology-specific sub-type (0–15)                 |
| `operating_mode`            | Byte 1 bits 3:0 — `OperatingMode` enum                                    |
| `backlight_type`            | Byte 1 bits 5:4 — `BacklightType` enum                                    |
| `data_enable_used`          | Byte 1 bit 6 — `true` if DE signal is used                                |
| `data_enable_positive`      | Byte 1 bit 7 — DE polarity (`true` = positive)                            |
| `native_pixels`             | Bytes 2–5, two LE uint16 — `(width_px, height_px)`; `None` if either is 0 |
| `panel_aspect_ratio_100`    | Byte 6 — raw byte; AR = value / 100 + 1                                   |
| `physical_orientation`      | Byte 7 bits 1:0 — `PhysicalOrientation` enum                              |
| `rotation_capability`       | Byte 7 bits 3:2 — `RotationCapability` enum                               |
| `zero_pixel_location`       | Byte 7 bits 5:4 — `ZeroPixelLocation` enum                                |
| `scan_direction`            | Byte 7 bits 7:6 — `ScanDirection` enum                                    |
| `subpixel_layout`           | Byte 8 — `SubpixelLayout` enum                                            |
| `pixel_pitch_hundredths_mm` | Bytes 9–10 — `(h, v)` in 0.01 mm; `None` if either is 0                   |
| `color_bit_depth`           | Byte 11 bits 3:0 — bpc − 1 converted to `ColorBitDepth`                   |
| `pixel_response_time_ms`    | Byte 12 — milliseconds; `None` if 0                                       |

**Interface Power Sequencing Block** (tag `0x0D`) is decoded into the following
`DisplayCapabilities` field (dynamic pipeline only):

| `DisplayCapabilities` field | Source                                                                                                                     |
|-----------------------------|----------------------------------------------------------------------------------------------------------------------------|
| `power_sequencing`          | All 6 timing fields (T1–T6) decoded into a `PowerSequencing` struct; raw counts in 2 ms units; `None` if payload < 6 bytes |

**Transfer Characteristics Block** (tag `0x0E`) is decoded into the following
`DisplayCapabilities` field (dynamic pipeline only):

| `DisplayCapabilities` field | Source                                                                                                                                                                                                               |
|-----------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `transfer_characteristic`   | Sample encoding (8/10/12-bit), channel mode (luminance or RGB), and normalized `[0.0, 1.0]` sample points decoded into a `DisplayIdTransferCharacteristic`; `None` if payload < 2 bytes or encoding byte is reserved |

In single-channel mode the curve is `TransferCurve::Luminance(Vec<f32>)`. In multi-channel
mode (byte 0 bit 5 set) the curve is `TransferCurve::Rgb { red, green, blue }` with the
sample data split into three equal sequential regions. Blocks where the sample bytes cannot
be split evenly are silently skipped.

**Display Interface Data Block** (tag `0x0F`) is decoded into the following
`DisplayCapabilities` field:

| `DisplayCapabilities` field | Source                                                                                                                                                                                       |
|-----------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `display_id_interface`      | Interface type, spread-spectrum flag, lane count, min/max pixel clock (in 10 kHz units), and content protection type decoded into a `DisplayIdInterface` struct; `None` if payload < 7 bytes |

**Stereo Display Interface Data Block** (tag `0x10`) is decoded into the following
`DisplayCapabilities` field:

| `DisplayCapabilities` field | Source                                                                                                                                                 |
|-----------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------|
| `stereo_interface`          | Stereo viewing mode, 3D sync signal polarity, and glasses sync interface decoded into a `DisplayIdStereoInterface` struct; `None` if payload < 2 bytes |

**Tiled Display Topology Data Block** (tag `0x12`) is decoded into the following
`DisplayCapabilities` field:

| `DisplayCapabilities` field | Source                                                                                                                                                                                                       |
|-----------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `tiled_topology`            | Grid dimensions, tile position, tile pixel size, single-enclosure flag, missing-tile behavior, and optional per-edge bezel sizes decoded into a `DisplayIdTiledTopology` struct; `None` if payload < 7 bytes |

The `h_tile_count` and `v_tile_count` fields store actual counts (raw value + 1). The
`bezel` field is `None` when the `has_bezel_info` flag is clear or the payload has fewer
than 11 bytes.

If the DisplayID block appears alongside an EDID base block, DisplayID values overwrite
any base-block values for the same fields.

## Extracted timing data

**Type I Video Timing blocks** (tag `0x03`) are decoded in both the dynamic and static
pipelines. Each 20-byte descriptor maps to a `VideoMode` with full timing detail:

| `VideoMode` field               | Source                                               |
|---------------------------------|------------------------------------------------------|
| `width`, `height`               | Horizontal/Vertical Active (exact pixel/line counts) |
| `refresh_rate`                  | Derived: `pixel_clock_hz / (h_total × v_total)`      |
| `pixel_clock_khz`               | Bytes 1–2 × 10 (10 kHz units converted to kHz)       |
| `interlaced`                    | Byte 19 bit 0                                        |
| `h_front_porch`, `h_sync_width` | Bytes 7–10                                           |
| `v_front_porch`, `v_sync_width` | Bytes 15–18                                          |
| `sync`                          | `DigitalSeparate`; polarities from byte 19 bits 3–4  |

Null descriptors (pixel clock = 0) are silently skipped.

**Type II Video Timing blocks** (tag `0x04`) are decoded in both the dynamic and static
pipelines. Each 11-byte descriptor maps to a `VideoMode`:

| `VideoMode` field               | Source                                                                |
|---------------------------------|-----------------------------------------------------------------------|
| `width`, `height`               | Horizontal/Vertical Active (8-pixel and 1-line granules respectively) |
| `refresh_rate`                  | Derived: `(raw_clock + 1) × 10 000 / (h_total × v_total)`             |
| `pixel_clock_khz`               | Bytes 0–2 `(raw + 1) × 10` (10 kHz units converted to kHz)            |
| `interlaced`                    | Byte 3 bit 4                                                          |
| `h_front_porch`, `h_sync_width` | Byte 6 nibbles (8-pixel granule)                                      |
| `v_front_porch`, `v_sync_width` | Byte 9 nibbles (1-line granule)                                       |
| `sync`                          | `DigitalSeparate`; polarities from byte 3 bits 3–2                    |

Byte 9 is dual-role: the full byte encodes the total `v_blank`; the upper/lower nibbles
encode `v_front_porch − 1` and `v_sync_width − 1`. The implied back porch is
`v_blank − v_front_porch − v_sync_width`.

**Type III Short Video Timing blocks** (tag `0x05`) are decoded in both the dynamic and static
pipelines. Each 3-byte descriptor encodes only the horizontal active, aspect ratio, and refresh
rate; vertical active is derived from the aspect ratio:

| `VideoMode` field                                                | Source                                                   |
|------------------------------------------------------------------|----------------------------------------------------------|
| `width`                                                          | `(byte 1 + 1) × 8` pixels (8-pixel granule, max 2048)    |
| `height`                                                         | `width × height_factor / width_factor` from aspect ratio |
| `refresh_rate`                                                   | Byte 2 bits 6:0, plus 1 Hz                               |
| `interlaced`                                                     | Byte 2 bit 7                                             |
| `h_front_porch`, `h_sync_width`, `v_front_porch`, `v_sync_width` | 0 (not encoded)                                          |
| `sync`                                                           | `None` (not encoded)                                     |

Descriptors with undefined or reserved aspect ratio codes are silently skipped, as are those
where the derived vertical active is not a whole number of lines.

**Type IV Timing Code blocks** (tag `0x06`) are decoded in both the dynamic and static
pipelines. Each payload byte is a timing identifier resolved via a lookup table. The code
space is selected by bits 7:6 of the data block's revision byte:

| Revision bits 7:6 | Code space  | Lookup                                                |
|-------------------|-------------|-------------------------------------------------------|
| `0`               | VESA DMT ID | DMT v1.13 table (IDs 0x01–0x58)                       |
| `1`               | CTA-861 VIC | VIC table (codes 1–219)                               |
| `2`               | HDMI VIC    | 4 codes: 1=3840×2160@30, 2=@25, 3=@24, 4=4096×2160@24 |
| `3`               | Reserved    | silently skipped                                      |

Timing detail fields (`h_front_porch`, `h_sync_width`, `v_front_porch`, `v_sync_width`,
`sync`) are fully populated for both DMT-resolved and VIC-resolved modes. HDMI VIC modes
carry only `width`, `height`, and `refresh_rate` (timing detail is not defined for them).
Unrecognised codes are silently skipped.

**VESA Video Timing blocks** (tag `0x07`) are decoded in both the dynamic and static
pipelines. The payload is a presence bitmap: up to 10 bytes encoding DMT IDs 0x01–0x50.
Bit `i` (0-indexed, LSB-first within each byte) corresponds to DMT ID `i + 1`. Set bits
are resolved via the DMT v1.13 table with full timing detail; payload bytes beyond 10 are
ignored.

| `VideoMode` field                                                | Source                                                   |
|------------------------------------------------------------------|----------------------------------------------------------|
| `width`, `height`, `refresh_rate`, `interlaced`                  | DMT v1.13 table entry                                    |
| `h_front_porch`, `h_sync_width`, `v_front_porch`, `v_sync_width` | DMT v1.13 table entry                                    |
| `sync`                                                           | `DigitalSeparate`; polarities from DMT v1.13 table entry |

**CTA-861 Video Timing blocks** (tag `0x08`) are decoded in both the dynamic and static
pipelines. The payload is a presence bitmap: up to 8 bytes encoding CTA-861 VICs 1–64.
Bit `i` (0-indexed, LSB-first within each byte) corresponds to VIC `i + 1`. Set bits are
resolved via the CTA-861 VIC table with full timing detail; payload bytes beyond 8 are ignored.

| `VideoMode` field                                                | Source                                                     |
|------------------------------------------------------------------|------------------------------------------------------------|
| `width`, `height`, `refresh_rate`, `interlaced`                  | CTA-861 VIC table entry                                    |
| `h_front_porch`, `h_sync_width`, `v_front_porch`, `v_sync_width` | CTA-861 VIC table entry                                    |
| `sync`                                                           | `DigitalSeparate`; polarities from CTA-861 VIC table entry |

**Type V Short Video Timing blocks** (tag `0x11`) are decoded in both the dynamic and static
pipelines. Each 7-byte descriptor encodes width, height, and refresh rate directly; no blanking
detail or pixel clock is stored. Type V is always progressive.

| `VideoMode` field                                                | Source                                                   |
|------------------------------------------------------------------|----------------------------------------------------------|
| `width`                                                          | Bytes 1–2 exact pixel count (LE uint16)                  |
| `height`                                                         | Bytes 3–4 exact line count (LE uint16)                   |
| `refresh_rate`                                                   | Byte 5 + 1 Hz (range 1–256, clamped to 255)              |
| `interlaced`                                                     | Always `false` — Type V defines progressive-only timings |
| `pixel_clock_khz`                                                | Not populated — not encoded in the descriptor            |
| `h_front_porch`, `h_sync_width`, `v_front_porch`, `v_sync_width` | 0 (not encoded)                                          |
| `sync`                                                           | `None` (not encoded)                                     |

Descriptors with zero width or height are silently skipped.

**Type VI Detailed Video Timing blocks** (tag `0x13`) are decoded in both the dynamic and static
pipelines. Each descriptor is 14 or 17 bytes; the 17-byte form includes optional aspect/size
bytes that are not currently decoded. Pixel clock is stored in 1 kHz steps (not 10 kHz like
Types I and II), allowing higher precision up to ~4194 MHz.

| `VideoMode` field | Source                                                                                  |
|-------------------|-----------------------------------------------------------------------------------------|
| `width`           | Bytes 3–4 bits 14:0 (15-bit, exact pixel count)                                         |
| `height`          | Bytes 5–6 bits 14:0 (15-bit, exact line count)                                          |
| `refresh_rate`    | Derived: `pixel_clock_hz / (h_total × v_total)`                                         |
| `pixel_clock_khz` | Bytes 0–2 bits 21:0 (1 kHz steps, stored directly)                                      |
| `interlaced`      | Byte 13 bit 7                                                                           |
| `h_front_porch`   | Bytes 7–9 (H-fp, 12-bit packed across byte 8 and byte 9 bits 7:4)                       |
| `h_sync_width`    | Byte 10 (8-bit)                                                                         |
| `v_front_porch`   | Byte 12 (8-bit)                                                                         |
| `v_sync_width`    | Byte 13 bits 3:0                                                                        |
| `sync`            | `DigitalSeparate`; h_sync_positive = byte 3–4 bit 15, v_sync_positive = byte 5–6 bit 15 |

Null descriptors (pixel clock = 0) advance the cursor without emitting a mode. The descriptor
size (14 or 17) is determined by byte 2 bit 22.

## Warnings

| Variant                                               | Meaning                                                                                                                                                                                    |
|-------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `DisplayIdVersionUnknown(u8)`                         | Version byte is outside the known ranges (0x10–0x1F, 0x20). The block is skipped.                                                                                                          |
| `DisplayIdExtensionCountMismatch { declared, found }` | The extension count in the first fragment's header does not match the number of continuation blocks actually present. Processing continues with whatever fragments are available.          |
| `DisplayIdChecksumMismatch`                           | The DisplayID section checksum byte does not make `block[1..=4+N]` sum to zero mod 256. Processing continues with whatever data is present in the fragment.                                |
| `DisplayIdSectionBytesOutOfRange(u8)`                 | The `section_byte_count` field (byte 2) is larger than 122, placing the checksum byte outside the 128-byte extension block. The section is still parsed using the clamped available bytes. |

## Fragment layout reference

Each 128-byte EDID extension block carrying DisplayID has the following structure:

```
Byte 0:         0x70 (EDID extension tag)
Byte 1:         DisplayID version/revision
Byte 2:         Section byte count N (data block payload bytes in this fragment; max 122)
Byte 3:         Bits [7:3] = continuation block count
                Bits [2:0] = display product primary use case
Bytes 4..4+N-1: DisplayID data blocks
Byte 4+N:       DisplayID section checksum (sum of bytes 1..=4+N must be 0 mod 256)
Bytes 4+N+1..126: Padding zeros
Byte 127:       EDID extension block checksum (sum of all 128 bytes must be 0 mod 256)
```

Data blocks within the payload each begin with a 3-byte header:

```
Byte 0: Block tag
Byte 1: Revision
Byte 2: Payload length (bytes following this header)
```

Iteration stops at an end-of-section sentinel (tag `0x00`, length `0`) or when a block's
declared length would extend past the available payload.
