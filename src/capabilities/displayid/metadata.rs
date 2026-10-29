use super::{
    DISPLAYID_V2, TAG_ASCII_STRING, TAG_COLOR_CHARACTERISTICS, TAG_DISPLAY_DEVICE_DATA,
    TAG_DISPLAY_INTERFACE, TAG_DISPLAY_PARAMS, TAG_POWER_SEQUENCING, TAG_PRODUCT_ID,
    TAG_SERIAL_NUMBER, TAG_STEREO_DISPLAY_INTERFACE, TAG_TILED_TOPOLOGY,
    TAG_TRANSFER_CHARACTERISTICS, TAG_V2_CONTAINER_ID, TAG_V2_CTA_DISPLAYID, TAG_V2_DISPLAY_PARAMS,
    TAG_V2_DYNAMIC_TIMING_RANGE, TAG_V2_INTERFACE_FEATURES, TAG_V2_PRODUCT_ID,
    TAG_V2_STEREO_INTERFACE, TAG_V2_TILED_TOPOLOGY, TAG_V2_VENDOR_SPECIFIC, TAG_VIDEO_TIMING_RANGE,
    for_each_data_block,
};

use crate::capabilities::base::{decode_color_bit_depth, decode_manufacture_date};
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::capabilities::cea861::parse_cea861_data_block_collection;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::capabilities::DisplayCapabilities;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::color::{Chromaticity, ChromaticityPoint, ColorBitDepth};
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::diagnostics::EdidWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::diagnostics::ParseWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::manufacture::{ManufactureDate, ManufacturerId, MonitorString};
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::options::DecodeOptions;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::panel::{
    BacklightType, DisplayIdInterface, DisplayIdStereoInterface, DisplayIdTiledTopology,
    DisplayInterfaceType, DisplayTechnology, InterfaceContentProtection, OperatingMode,
    PhysicalOrientation, PowerSequencing, RotationCapability, ScanDirection, StereoSyncInterface,
    StereoViewingMode, SubpixelLayout, TileBezelInfo, TileTopologyBehavior, ZeroPixelLocation,
};
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::prelude::{Arc, Vec};
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::transfer::{
    DisplayIdTransferCharacteristic, TransferCurve, TransferPointEncoding,
};
#[cfg(any(feature = "alloc", feature = "std"))]
use display_types::DisplayIdCapabilities;
#[cfg(any(feature = "alloc", feature = "std"))]
use display_types::displayid::{
    Chromaticity12, ChromaticityPoint12, ColorDepthsFull, ColorDepthsSubsampled,
    CustomColorSpaceEotfCombo, DisplayIdImageSize, DisplayIdStereoInterfaceV2,
    DisplayIdVendorSpecific, DisplayInterfaceFeatures, DisplayParamsV2,
    DisplayTechnology as V2DisplayTechnology, DualInterfaceMirroring, DynamicTimingRange,
    ImageSizeUnit, ScanOrientation, StereoEye, StereoTimingCode, StereoTimingCodeType,
    StereoTimingScopeV2, StereoViewingMethodV2,
};

/// Decodes a Product Identification Block payload into `caps`.
///
/// Payload layout (DisplayID 1.x §4.2):
/// - Bytes 0–1: Manufacturer ID (2-byte PNP-encoded, same as EDID base block)
/// - Bytes 2–3: Product code (little-endian uint16)
/// - Bytes 4–7: Serial number (little-endian uint32; `0` = not specified)
/// - Byte  8:   Week of manufacture (`0` = unspecified, `0xFF` = model year)
/// - Byte  9:   Year (`byte + 1990`; when week = `0xFF`, this is the model year)
/// - Bytes 10+: ASCII product name (space-padded, `0x0A`-terminated; may be absent)
///
/// Fields are written only when the payload is long enough to contain them.
/// If the block is already populated (e.g., by the EDID base block), the values
/// are overwritten by the DisplayID data.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_product_id_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    // Manufacturer ID — 2-byte packed PNP encoding (same as EDID base block bytes 0x08–0x09).
    if payload.len() >= 2 {
        let id_raw = ((payload[0] as u16) << 8) | (payload[1] as u16);
        let char1 = ((id_raw >> 10) & 0x1F) as u8;
        let char2 = ((id_raw >> 5) & 0x1F) as u8;
        let char3 = (id_raw & 0x1F) as u8;
        if (1..=26).contains(&char1) && (1..=26).contains(&char2) && (1..=26).contains(&char3) {
            caps.manufacturer = Some(ManufacturerId([
                char1 + b'A' - 1,
                char2 + b'A' - 1,
                char3 + b'A' - 1,
            ]));
        }
    }

    // Product code (LE uint16).
    if payload.len() >= 4 {
        caps.product_code = Some(u16::from_le_bytes([payload[2], payload[3]]));
    }

    // Serial number (LE uint32; 0 = not specified).
    if payload.len() >= 8 {
        let sn = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
        if sn != 0 {
            caps.serial_number = Some(sn);
        }
    }

    // Manufacture / model year.
    if payload.len() >= 10 {
        caps.manufacture_date = Some(decode_manufacture_date(payload[8], payload[9]));
    }

    // Product name: bytes 10+ (ASCII, 0x0A-terminated, space-padded; max 13 bytes stored).
    if payload.len() >= 11 {
        let name_bytes = &payload[10..];
        let mut buf = [b' '; 13];
        let copy_len = name_bytes.len().min(13);
        buf[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        // Ensure there is a 0x0A terminator within the buffer.
        if !buf.contains(&0x0A) {
            let term_pos = copy_len.min(12);
            buf[term_pos] = 0x0A;
        }
        caps.display_name = Some(MonitorString(buf));
    }
}

/// Scans all data blocks in `payload` for a Product Identification Block (tag `0x00`)
/// and decodes the first one found into `caps`.
#[cfg(any(feature = "alloc", feature = "std"))]
#[cfg(test)]
pub(super) fn scan_product_id_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    let mut found = false;
    for_each_data_block(payload, |tag, _revision, block_payload| {
        if tag == TAG_PRODUCT_ID && !found {
            found = true;
            decode_product_id_block(block_payload, caps);
        }
    });
}

/// Decodes a DisplayID 2.x Product Identification Block payload (tag `0x20`).
///
/// Payload layout (DisplayID 2.x §4.1):
/// - Bytes 0–2:   IEEE OUI (manufacturer identifier; 3 raw bytes — not PNP encoded)
/// - Bytes 3–4:   Product code (little-endian uint16)
/// - Bytes 5–8:   Serial number (little-endian uint32; `0` = not specified)
/// - Byte  9:     Week of manufacture (`0` = unspecified, `0xFF` = model year)
/// - Byte  10:    Year (`byte + 2000`; when week = `0xFF`, this is the model year)
/// - Byte  11:    Product name length in bytes (`0` = no name; spec maximum 236)
/// - Bytes 12+:   Product name (no termination; ASCII / ISO 8859-1)
///
/// The OUI is written to `did.manufacturer_oui`. The V1 PNP-derived `caps.manufacturer`
/// field is intentionally left untouched — DisplayID 2.x identifies vendors by IEEE OUI,
/// which does not map onto the 3-letter PNP namespace.
///
/// Each field is written only when the payload is long enough to contain it. The product
/// name is truncated to fit `MonitorString`'s 13-byte buffer; longer names lose tail bytes.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_v2_product_id_block(
    payload: &[u8],
    caps: &mut DisplayCapabilities,
    did: &mut DisplayIdCapabilities,
) {
    if payload.len() >= 3 {
        did.manufacturer_oui = Some([payload[0], payload[1], payload[2]]);
    }
    if payload.len() >= 5 {
        caps.product_code = Some(u16::from_le_bytes([payload[3], payload[4]]));
    }
    if payload.len() >= 9 {
        let sn = u32::from_le_bytes([payload[5], payload[6], payload[7], payload[8]]);
        if sn != 0 {
            caps.serial_number = Some(sn);
        }
    }
    if payload.len() >= 11 {
        caps.manufacture_date = Some(decode_v2_manufacture_date(payload[9], payload[10]));
    }
    if payload.len() >= 12 {
        let name_len = payload[11] as usize;
        if name_len > 0 && payload.len() >= 12 + name_len {
            let name_bytes = &payload[12..12 + name_len];
            let mut buf = [b' '; 13];
            let copy_len = name_bytes.len().min(13);
            buf[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
            // Names ≤ 12 bytes leave the tail space-padded; `MonitorString::as_str`
            // strips trailing spaces. Names of exactly 13 bytes round-trip fully
            // because `as_str` falls back to the full buffer when no `0x0A` is found.
            // Names containing an embedded `0x0A` are truncated at the terminator
            // by `as_str`, matching the V1 wire convention.
            caps.display_name = Some(MonitorString(buf));
        }
    }
}

/// Decodes the DisplayID 2.x manufacture date encoding (year stored as `byte + 2000`).
#[cfg(any(feature = "alloc", feature = "std"))]
const fn decode_v2_manufacture_date(week: u8, year: u8) -> ManufactureDate {
    let y = year as u16 + 2000;
    match week {
        0xFF => ManufactureDate::ModelYear(y),
        0x00 => ManufactureDate::Manufactured {
            week: None,
            year: y,
        },
        w => ManufactureDate::Manufactured {
            week: Some(w),
            year: y,
        },
    }
}

/// Converts an IEEE 754-2008 binary16 (half-precision) value to `f32`.
///
/// Returns `None` for either zero (`0x8000` is the spec's "not used" sentinel; `0x0000`
/// is accepted leniently because 0 cd/m² is degenerate for any of the three luminance
/// fields and almost certainly indicates an EDID writer that confused the sign), or
/// when the value decodes to `NaN` / infinity (out-of-range for cd/m² readings).
#[cfg(any(feature = "alloc", feature = "std"))]
fn decode_luminance_f16(raw: u16) -> Option<f32> {
    if raw & 0x7FFF == 0 {
        return None;
    }
    let sign = u32::from((raw >> 15) & 0x1);
    let exp = u32::from((raw >> 10) & 0x1F);
    let mant = u32::from(raw & 0x3FF);

    let bits: u32 = if exp == 0 && mant == 0 {
        sign << 31
    } else if exp == 31 {
        // ±inf / NaN — not meaningful luminance.
        return None;
    } else if exp == 0 {
        // Subnormal: renormalise into f32 normal range.
        let mut m = mant;
        let mut e: i32 = -14;
        while (m & 0x400) == 0 {
            m <<= 1;
            e -= 1;
        }
        m &= 0x3FF;
        let f32_exp = (e + 127) as u32;
        (sign << 31) | (f32_exp << 23) | (m << 13)
    } else {
        let f32_exp = exp + 127 - 15;
        (sign << 31) | (f32_exp << 23) | (mant << 13)
    };

    Some(f32::from_bits(bits))
}

/// Maps the 3-bit DisplayID 2.x color depth field (block 0x21 byte 27, bits 2:0) to a
/// `(bpc, ColorBitDepth)` pair, the two representations needed at the call site.
///
/// Returns `None` for the "undefined" code (`0`) and for values reserved by the spec
/// (`6`, `7`). The 2.x encoding skips 14 bpc (which 1.x supports), so `5` decodes to 16.
#[cfg(any(feature = "alloc", feature = "std"))]
const fn decode_v2_color_bit_depth(field: u8) -> Option<(u8, ColorBitDepth)> {
    match field & 0x07 {
        1 => Some((6, ColorBitDepth::Depth6)),
        2 => Some((8, ColorBitDepth::Depth8)),
        3 => Some((10, ColorBitDepth::Depth10)),
        4 => Some((12, ColorBitDepth::Depth12)),
        5 => Some((16, ColorBitDepth::Depth16)),
        _ => None,
    }
}

/// Decodes a DisplayID 2.x Display Parameters Block payload (tag `0x21`).
///
/// Payload layout (DisplayID 2.x §4.2, fixed 29 bytes):
/// - Bytes  0–1: Horizontal image size (LE uint16; precision per revision bit 7)
/// - Bytes  2–3: Vertical image size (LE uint16)
/// - Bytes  4–5: Horizontal native pixel count (LE uint16; `0` = undefined)
/// - Bytes  6–7: Vertical native pixel count (LE uint16; `0` = undefined)
/// - Byte   8:   Feature support flags
///   - bits 2:0  Scan orientation
///   - bit  3    Luminance information: `0` = guaranteed minima, `1` = source guidance
///   - bit  6    Color space coordinates: `0` = CIE 1931 (x,y), `1` = CIE 1976 (u',v')
///   - bit  7    Audio output: `0` = integrated speakers, `1` = external jack
/// - Bytes  9–11: Primary 1 (red) chromaticity, 12-bit packed
/// - Bytes 12–14: Primary 2 (green) chromaticity
/// - Bytes 15–17: Primary 3 (blue) chromaticity
/// - Bytes 18–20: White point chromaticity
/// - Bytes 21–22: Max luminance, full coverage (IEEE 754 binary16; `0x8000` = unused)
/// - Bytes 23–24: Max luminance, 10% coverage (binary16)
/// - Bytes 25–26: Min luminance (binary16)
/// - Byte  27:    Color depth (bits 2:0) and display technology (bits 6:4)
/// - Byte  28:    Gamma EOTF, stored as `(γ − 1) × 100`; `0xFF` = unspecified
///
/// Image size precision is signalled by bit 7 of the data-block revision byte:
/// `0` = 0.1 mm units (default), `1` = 1 mm units. The size is stored as declared, with
/// its unit, in `did.image_size`.
///
/// Chromaticity, luminance, gamma, color depth, display technology, scan orientation,
/// audio routing, and the CIE-coordinate variant are stored on
/// `did.display_params_v2`. Native pixel count is mirrored onto `caps.native_pixels`,
/// alongside `caps.color_bit_depth` (when defined). Short payloads are silently ignored.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_v2_display_params_block(
    payload: &[u8],
    revision: u8,
    caps: &mut DisplayCapabilities,
    did: &mut DisplayIdCapabilities,
) {
    if payload.len() < 29 {
        return;
    }

    // 0x21 repurposes bit 7 of the data-block revision byte as the image-size precision
    // flag (in-band field, not part of the revision number). Other bits hold the
    // revision number; only rev 0 is currently defined and unknown rev numbers are not
    // surfaced as warnings here.
    let size_in_whole_mm = (revision >> 7) & 0x01 != 0;
    let h_size = u16::from_le_bytes([payload[0], payload[1]]);
    let v_size = u16::from_le_bytes([payload[2], payload[3]]);
    if h_size != 0 && v_size != 0 {
        let unit = if size_in_whole_mm {
            ImageSizeUnit::Millimetre
        } else {
            ImageSizeUnit::TenthMillimetre
        };
        did.image_size = Some(DisplayIdImageSize::new(h_size, v_size, unit));
    }

    let h_px = u16::from_le_bytes([payload[4], payload[5]]);
    let v_px = u16::from_le_bytes([payload[6], payload[7]]);
    if h_px != 0 && v_px != 0 {
        caps.native_pixels = Some((h_px, v_px));
    }

    let flags = payload[8];
    let scan_orientation = ScanOrientation::from_bits(flags);
    let luminance_guidance = (flags >> 3) & 0x01 != 0;
    let color_space_cie1976 = (flags >> 6) & 0x01 != 0;
    let audio_external = (flags >> 7) & 0x01 != 0;

    let read_chromaticity_point = |offset: usize| ChromaticityPoint12 {
        x_raw: u16::from(payload[offset]) | ((u16::from(payload[offset + 1]) & 0x0F) << 8),
        y_raw: ((u16::from(payload[offset + 1]) >> 4) & 0x0F)
            | (u16::from(payload[offset + 2]) << 4),
    };
    let chromaticity = Chromaticity12 {
        primary1: read_chromaticity_point(9),
        primary2: read_chromaticity_point(12),
        primary3: read_chromaticity_point(15),
        white: read_chromaticity_point(18),
    };

    let max_luminance_full = decode_luminance_f16(u16::from_le_bytes([payload[21], payload[22]]));
    let max_luminance_10pct = decode_luminance_f16(u16::from_le_bytes([payload[23], payload[24]]));
    let min_luminance = decode_luminance_f16(u16::from_le_bytes([payload[25], payload[26]]));

    let depth_tech_byte = payload[27];
    let (color_bit_depth_bpc, color_bit_depth_enum) =
        decode_v2_color_bit_depth(depth_tech_byte & 0x07).unzip();
    let display_technology = V2DisplayTechnology::from_byte((depth_tech_byte >> 4) & 0x07);

    let gamma_byte = payload[28];
    let gamma = if gamma_byte == 0xFF {
        None
    } else {
        Some(f32::from(gamma_byte) / 100.0 + 1.0)
    };

    if let Some(bpc_enum) = color_bit_depth_enum {
        caps.color_bit_depth = Some(bpc_enum);
    }

    let mut params = DisplayParamsV2::default();
    params.chromaticity = chromaticity;
    params.color_space_cie1976 = color_space_cie1976;
    params.max_luminance_full = max_luminance_full;
    params.max_luminance_10pct = max_luminance_10pct;
    params.min_luminance = min_luminance;
    params.luminance_guidance = luminance_guidance;
    params.color_bit_depth = color_bit_depth_bpc;
    params.display_technology = display_technology;
    params.gamma = gamma;
    params.scan_orientation = scan_orientation;
    params.audio_external = audio_external;
    did.display_params_v2 = Some(params);
}

/// Decodes a DisplayID 2.x Dynamic Video Timing Range Limits Block payload (tag `0x25`).
///
/// Payload layout (DisplayID 2.x §4.3, fixed 9 bytes):
/// - Bytes 0–2:  Minimum pixel clock in kHz (24-bit LE)
/// - Bytes 3–5:  Maximum pixel clock in kHz (24-bit LE)
/// - Byte  6:    Minimum vertical refresh rate in Hz
/// - Byte  7:    Maximum vertical refresh rate, low 8 bits
/// - Byte  8:    Support flags
///   - Bits 1:0  Maximum vertical refresh rate, high 2 bits (block revision ≥ 1; gives a 9-bit max)
///   - Bit  7    Seamless variable refresh rate: `0` = unsupported, `1` = supported
///     (fixed horizontal pixel rate, dynamic vertical blanking)
///
/// On block revision 0 the upper 2 bits of byte 8 are reserved; the max vertical refresh
/// rate is the 8-bit value from byte 7 alone.
///
/// The decoded record is stored on `did.dynamic_timing_range`. For interoperability with
/// the V1 0x09 path, `caps.max_pixel_clock_mhz` and `caps.min_v_rate` / `caps.max_v_rate`
/// are also populated. Pixel clock is converted from kHz to MHz, losing sub-MHz precision
/// in the unified field — callers needing kHz precision must read `did.dynamic_timing_range`.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_v2_dynamic_timing_range_block(
    payload: &[u8],
    revision: u8,
    caps: &mut DisplayCapabilities,
    did: &mut DisplayIdCapabilities,
) {
    if payload.len() < 9 {
        return;
    }

    let min_pixel_clock_khz =
        u32::from(payload[0]) | (u32::from(payload[1]) << 8) | (u32::from(payload[2]) << 16);
    let max_pixel_clock_khz =
        u32::from(payload[3]) | (u32::from(payload[4]) << 8) | (u32::from(payload[5]) << 16);
    let min_v_rate_hz = payload[6];
    let max_v_lsb = payload[7];
    let flags = payload[8];

    // 0x25 has no in-band fields in the data-block revision byte (unlike 0x21's bit 7
    // and 0x27's bits 7:6); the whole byte is the revision number. Spec defines rev 0
    // (8-bit max v rate) and rev 1 (9-bit max v rate using bits 1:0 of the flags byte).
    let max_v_rate_hz: u16 = if revision >= 1 {
        u16::from(max_v_lsb) | (u16::from(flags & 0x03) << 8)
    } else {
        u16::from(max_v_lsb)
    };
    let vrr_supported = (flags >> 7) & 0x01 != 0;

    let mut range = DynamicTimingRange::default();
    range.min_pixel_clock_khz = min_pixel_clock_khz;
    range.max_pixel_clock_khz = max_pixel_clock_khz;
    range.min_v_rate_hz = min_v_rate_hz;
    range.max_v_rate_hz = max_v_rate_hz;
    range.vrr_supported = vrr_supported;
    did.dynamic_timing_range = Some(range);

    if max_pixel_clock_khz != 0 {
        caps.max_pixel_clock_mhz =
            Some((max_pixel_clock_khz / 1000).min(u32::from(u16::MAX)) as u16);
    }
    if min_v_rate_hz != 0 {
        caps.min_v_rate = Some(u16::from(min_v_rate_hz));
    }
    if max_v_rate_hz != 0 {
        caps.max_v_rate = Some(max_v_rate_hz);
    }
}

/// Decodes a DisplayID 2.x Display Interface Features Block payload (tag `0x26`).
///
/// Payload layout (DisplayID 2.x §4.6, mandatory 9 bytes):
/// - Byte 0: RGB color depth bitmask
///   (bit 0 = 6 bpc, bit 1 = 8, bit 2 = 10, bit 3 = 12, bit 4 = 14, bit 5 = 16)
/// - Byte 1: YCbCr 4:4:4 color depth bitmask (same bit layout as RGB)
/// - Byte 2: YCbCr 4:2:2 color depth bitmask
///   (bit 0 = 8 bpc, bit 1 = 10, bit 2 = 12, bit 3 = 14, bit 4 = 16)
/// - Byte 3: YCbCr 4:2:0 color depth bitmask (same bit layout as 4:2:2)
/// - Byte 4: Minimum pixel rate at which YCbCr 4:2:0 is supported, in 74.25 MP/s units
///   (`0` = supported at all pixel rates)
/// - Byte 5: Audio capability flags (bit 5 = 32 kHz, bit 6 = 44.1 kHz, bit 7 = 48 kHz)
/// - Byte 6: Color space and EOTF defined-combinations bitmask
/// - Byte 7: Reserved (always 0; skipped)
/// - Byte 8: Count N of custom color space/EOTF combinations (0–7)
/// - Bytes 9–(8+N): N custom combo bytes (bits 7:4 = color space index, bits 3:0 = EOTF index)
///
/// The decoded record is stored on `did.interface_features`. If the payload is shorter
/// than the mandatory 9 bytes the block is skipped with no side effects.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_v2_interface_features_block(payload: &[u8], did: &mut DisplayIdCapabilities) {
    if payload.len() < 9 {
        return;
    }

    let mut features = DisplayInterfaceFeatures::default();
    features.color_depth_rgb = ColorDepthsFull::from_bits_truncate(payload[0]);
    features.color_depth_ycbcr444 = ColorDepthsFull::from_bits_truncate(payload[1]);
    features.color_depth_ycbcr422 = ColorDepthsSubsampled::from_bits_truncate(payload[2]);
    features.color_depth_ycbcr420 = ColorDepthsSubsampled::from_bits_truncate(payload[3]);
    features.min_ycbcr420_pixel_rate = payload[4];
    features.audio_flags = payload[5];
    features.color_space_eotf_combos = payload[6];
    // payload[7] is reserved; payload[8] is the count of custom combos.
    let count = (payload[8] as usize).min(7);
    let available = payload.len().saturating_sub(9);
    let n = count.min(available);
    for i in 0..n {
        features.custom_color_space_eotf_combos[i] =
            CustomColorSpaceEotfCombo::new((payload[9 + i] >> 4) & 0x0F, payload[9 + i] & 0x0F);
    }
    features.custom_color_space_eotf_count = n as u8;
    did.interface_features = Some(features);
}

/// Decodes a DisplayID 2.x Stereo Display Interface Block payload (tag `0x27`).
///
/// Payload layout (DisplayID 2.x §4.7, variable length):
/// - Byte 0: Length of the stereo descriptor that follows (in bytes; ≥ 1, includes the
///   method byte). The full descriptor occupies `payload[1..1 + payload[0]]`.
/// - Byte 1: Stereo viewing method
///   - `0x00` Field Sequential
///   - `0x01` Side-by-Side
///   - `0x02` Pixel Interleaved
///   - `0x03` Dual Interface
///   - `0x04` Multi-View
///   - `0x05` Stacked Frame
///   - `0xFF` Proprietary
///   - other  Reserved (surfaced as [`StereoViewingMethodV2::Reserved`])
/// - Byte 2 onwards: Method-specific arguments (length depends on method).
///
/// The `revision` byte's upper two bits encode the timing scope (see
/// [`StereoTimingScopeV2::from_revision`]). When the scope indicates inline timing codes,
/// the payload carries a list of DMT/VIC/HDMI-VIC code records after the stereo descriptor;
/// those are decoded into [`DisplayIdStereoInterfaceV2::timing_codes`].
///
/// The decoded record is stored on `did.stereo_interface_v2`. Payloads shorter than the
/// declared descriptor length, or with method-specific argument bytes missing, are skipped.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_v2_stereo_interface_block(
    payload: &[u8],
    revision: u8,
    did: &mut DisplayIdCapabilities,
) {
    if payload.len() < 2 {
        return;
    }
    let descriptor_len = payload[0] as usize;
    if descriptor_len < 1 || payload.len() < 1 + descriptor_len {
        return;
    }
    let method_byte = payload[1];
    let args = &payload[2..1 + descriptor_len];

    let method = match method_byte {
        0x00 => {
            if args.is_empty() {
                return;
            }
            StereoViewingMethodV2::FieldSequential {
                eye_on_high_half: if (args[0] & 0x01) != 0 {
                    StereoEye::Right
                } else {
                    StereoEye::Left
                },
            }
        }
        0x01 => {
            if args.is_empty() {
                return;
            }
            StereoViewingMethodV2::SideBySide {
                left_half: if (args[0] & 0x01) != 0 {
                    StereoEye::Right
                } else {
                    StereoEye::Left
                },
            }
        }
        0x02 => {
            if args.len() < 8 {
                return;
            }
            let mut pattern = [0u8; 8];
            pattern.copy_from_slice(&args[..8]);
            StereoViewingMethodV2::PixelInterleaved { pattern }
        }
        0x03 => {
            if args.is_empty() {
                return;
            }
            let eye = if (args[0] & 0x01) != 0 {
                StereoEye::Right
            } else {
                StereoEye::Left
            };
            let mirroring = match (args[0] >> 1) & 0x03 {
                0b00 => DualInterfaceMirroring::None,
                0b01 => DualInterfaceMirroring::LeftRight,
                0b10 => DualInterfaceMirroring::TopBottom,
                _ => DualInterfaceMirroring::Reserved,
            };
            StereoViewingMethodV2::DualInterface { eye, mirroring }
        }
        0x04 => {
            if args.len() < 2 {
                return;
            }
            StereoViewingMethodV2::MultiView {
                view_count: args[0],
                interleaving_method_code: args[1],
            }
        }
        0x05 => {
            if args.is_empty() {
                return;
            }
            StereoViewingMethodV2::StackedFrame {
                top_half: if (args[0] & 0x01) != 0 {
                    StereoEye::Right
                } else {
                    StereoEye::Left
                },
            }
        }
        0xFF => StereoViewingMethodV2::Proprietary,
        other => StereoViewingMethodV2::Reserved(other),
    };

    // 0x27 repurposes bits 7:6 of the data-block revision byte as the timing scope
    // (in-band field). The remaining bits hold the revision number; only rev 0 is
    // currently defined and unknown rev numbers are not surfaced as warnings here.
    let timing_scope = StereoTimingScopeV2::from_revision(revision);

    let mut timing_codes = Vec::new();
    if timing_scope.has_timing_codes() {
        let mut remaining = &payload[1 + descriptor_len..];
        while remaining.len() >= 2 {
            let header = remaining[0];
            let num_codes = (header & 0x1F) as usize;
            let code_type = match header >> 6 {
                0b00 => StereoTimingCodeType::Dmt,
                0b01 => StereoTimingCodeType::Vic,
                0b10 => StereoTimingCodeType::HdmiVic,
                _ => StereoTimingCodeType::Reserved,
            };
            if remaining.len() < 1 + num_codes {
                break;
            }
            for &code in &remaining[1..1 + num_codes] {
                timing_codes.push(StereoTimingCode::new(code_type, code));
            }
            remaining = &remaining[1 + num_codes..];
        }
    }

    let mut record = DisplayIdStereoInterfaceV2::default();
    record.timing_scope = timing_scope;
    record.method = method;
    record.timing_codes = timing_codes;
    did.stereo_interface_v2 = Some(record);
}

/// Decodes a DisplayID 2.x ContainerID Block payload (tag `0x29`).
///
/// Payload layout (DisplayID 2.x §4.9, fixed 16 bytes):
/// - Bytes 0–15: 128-bit UUID identifying the physical display container
///   (typically a Microsoft-style ContainerID GUID, used by the OS to group
///   related interfaces such as a tiled monitor's individual tile EDIDs).
///
/// The raw 16-byte buffer is stored on `did.container_id`. Endianness is
/// preserved as-is — byte ordering interpretation (mixed-endian for the
/// classic GUID layout vs. big-endian for RFC 4122) is left to consumers.
/// Payloads shorter than 16 bytes are skipped with no side effects.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_v2_container_id_block(payload: &[u8], did: &mut DisplayIdCapabilities) {
    if payload.len() < 16 {
        return;
    }
    let mut uuid = [0u8; 16];
    uuid.copy_from_slice(&payload[..16]);
    did.container_id = Some(uuid);
}

/// Decodes a DisplayID 2.x Vendor-Specific Block payload (tag `0x7E`).
///
/// Payload layout (DisplayID 2.x §4.10, minimum 3 bytes):
/// - Bytes 0–2: 3-byte IEEE OUI identifying the vendor (high-order byte first).
/// - Bytes 3+:  Opaque vendor-defined data; semantics are not interpreted here.
///
/// The decoded record is appended to `did.vendor_specific`. Multiple 0x7E blocks
/// are allowed in a single section — each is recorded in payload order. Payloads
/// shorter than 3 bytes (no complete OUI) are skipped.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_v2_vendor_specific_block(payload: &[u8], did: &mut DisplayIdCapabilities) {
    if payload.len() < 3 {
        return;
    }
    let mut record = DisplayIdVendorSpecific::default();
    record.oui = [payload[0], payload[1], payload[2]];
    record.data = payload[3..].to_vec();
    did.vendor_specific.push(record);
}

/// Decodes a DisplayID 2.x CTA DisplayID Block payload (tag `0x81`).
///
/// The payload is a CTA-861 data block collection — the same structure that follows
/// byte 4 of a CEA-861 extension block, but without DTDs or section flags. Each block
/// starts with a 1-byte header (`tag << 5 | length`) followed by `length` payload
/// bytes; a zero header byte terminates scanning.
///
/// Decoding is delegated to [`parse_cea861_data_block_collection`]. The resulting
/// CTA-861 capability state is merged into the sink's existing `Cea861Capabilities`
/// entry (extension tag `0x02`) under a take-mutate-restore pattern, so
/// 0x81-derived data combines with any data parsed from a real CEA-861 extension
/// block on the same EDID — regardless of which extension was processed first.
///
/// Spec marks revision 0 as the only defined value; non-zero revisions emit
/// [`EdidWarning::UnsupportedV2BlockRevision`] and the payload is parsed anyway
/// using the revision-0 wire format.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_v2_cta_displayid_block(
    payload: &[u8],
    revision: u8,
    caps: &mut DisplayCapabilities,
    warnings: &mut Vec<ParseWarning>,
    options: &DecodeOptions,
) {
    if revision != 0 {
        warnings.push(Arc::new(EdidWarning::UnsupportedV2BlockRevision {
            tag: TAG_V2_CTA_DISPLAYID,
            revision,
        }));
    }
    let mut cea_caps = caps
        .take_extension_data::<crate::capabilities::cea861::Cea861Capabilities>(0x02)
        .unwrap_or_else(|| {
            crate::capabilities::cea861::Cea861Capabilities::new(
                crate::capabilities::cea861::Cea861Flags::empty(),
            )
        });
    parse_cea861_data_block_collection(payload, caps, &mut cea_caps, warnings, options);
    caps.set_extension_data(0x02, cea_caps);
}

/// Decodes a Display Parameters Block payload into `caps`, and its image size into
/// `did.image_size`.
///
/// Payload layout (DisplayID 1.x §4.3, fixed 12 bytes):
/// - Bytes  0–1: Horizontal image size in tenths of mm (little-endian uint16; `0` = not defined)
/// - Bytes  2–3: Vertical image size in tenths of mm (little-endian uint16; `0` = not defined)
/// - Bytes  4–5: Horizontal native pixel count (little-endian uint16; `0` = not defined)
/// - Bytes  6–7: Vertical native pixel count (little-endian uint16; `0` = not defined)
/// - Byte   8:   Feature support flags (deinterlacing, audio, etc.; not decoded)
/// - Byte   9:   Gamma EOTF, stored as `(γ − 1) × 100`; `0xFF` = unspecified (not decoded)
/// - Byte  10:   Aspect ratio, stored as `(AR − 1) × 100` (same encoding as Display Device Data)
/// - Byte  11:   Color bit depth — low nibble = native bpc − 1, high nibble = overall bpc − 1
///
/// Image size and native pixel count are written only when both axes are non-zero.
/// Aspect ratio and color bit depth are written when the payload is long enough.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_display_params_block(
    payload: &[u8],
    caps: &mut DisplayCapabilities,
    did: &mut DisplayIdCapabilities,
) {
    // Image size in tenths of mm (bytes 0–3), as edid-decode and libdisplay-info read it.
    if payload.len() >= 4 {
        let h = u16::from_le_bytes([payload[0], payload[1]]);
        let v = u16::from_le_bytes([payload[2], payload[3]]);
        if h != 0 && v != 0 {
            did.image_size = Some(DisplayIdImageSize::new(
                h,
                v,
                ImageSizeUnit::TenthMillimetre,
            ));
        }
    }

    // Native pixel format (bytes 4–7).
    if payload.len() >= 8 {
        let h_px = u16::from_le_bytes([payload[4], payload[5]]);
        let v_px = u16::from_le_bytes([payload[6], payload[7]]);
        if h_px != 0 && v_px != 0 {
            caps.native_pixels = Some((h_px, v_px));
        }
    }

    // Aspect ratio (byte 10): stored as (AR − 1) × 100.
    if payload.len() >= 11 {
        caps.panel_aspect_ratio_100 = Some(payload[10]);
    }

    // Color bit depth (byte 11): low nibble = native bpc − 1.
    // Convert to EDID-style bits: bpc 6→1, 8→2, 10→3, 12→4, 14→5, 16→6.
    if payload.len() >= 12 {
        let bpc = (payload[11] & 0x0F) + 1;
        let edid_bits: u8 = match bpc {
            6 => 1,
            8 => 2,
            10 => 3,
            12 => 4,
            14 => 5,
            16 => 6,
            _ => 0,
        };
        caps.color_bit_depth = decode_color_bit_depth(edid_bits);
    }
}

/// Scans all data blocks in `payload` for a Display Parameters Block (tag `0x01`)
/// and decodes the first one found into `caps` and `did`.
#[cfg(any(feature = "alloc", feature = "std"))]
#[cfg(test)]
pub(super) fn scan_display_params_block(
    payload: &[u8],
    caps: &mut DisplayCapabilities,
    did: &mut DisplayIdCapabilities,
) {
    let mut found = false;
    for_each_data_block(payload, |tag, _revision, block_payload| {
        if tag == TAG_DISPLAY_PARAMS && !found {
            found = true;
            decode_display_params_block(block_payload, caps, did);
        }
    });
}

/// Decodes a Color Characteristics Block payload into `caps.chromaticity`.
///
/// Payload layout (DisplayID 1.x §4.4):
/// - Bytes  0–1:  Red primary x   (little-endian uint16; value × 1/1024 = CIE x)
/// - Bytes  2–3:  Red primary y
/// - Bytes  4–5:  Green primary x
/// - Bytes  6–7:  Green primary y
/// - Bytes  8–9:  Blue primary x
/// - Bytes 10–11: Blue primary y
/// - Bytes 12–13: White point x
/// - Bytes 14–15: White point y
///
/// The 16-bit values use the same 1/1024 scale as the 10-bit EDID base block encoding.
/// Only the lower 10 bits are significant; upper bits are reserved and masked out.
/// The full 16 bytes must be present; short payloads are ignored.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_color_characteristics_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    if payload.len() < 16 {
        return;
    }

    let read_point = |i: usize| ChromaticityPoint {
        x_raw: u16::from_le_bytes([payload[i], payload[i + 1]]) & 0x03FF,
        y_raw: u16::from_le_bytes([payload[i + 2], payload[i + 3]]) & 0x03FF,
    };

    caps.chromaticity = Chromaticity {
        red: read_point(0),
        green: read_point(4),
        blue: read_point(8),
        white: read_point(12),
    };
}

/// Scans all data blocks in `payload` for a Color Characteristics Block (tag `0x02`)
/// and decodes the first one found into `caps`.
#[cfg(any(feature = "alloc", feature = "std"))]
#[cfg(test)]
pub(super) fn scan_color_characteristics_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    let mut found = false;
    for_each_data_block(payload, |tag, _revision, block_payload| {
        if tag == TAG_COLOR_CHARACTERISTICS && !found {
            found = true;
            decode_color_characteristics_block(block_payload, caps);
        }
    });
}

/// Decodes a Video Timing Range Limits Block payload into `caps`.
///
/// Payload layout (DisplayID 1.x §4.5, 15 bytes):
/// - Bytes 0–2:  Minimum pixel clock in 10 kHz steps (24-bit LE; not stored)
/// - Bytes 3–5:  Maximum pixel clock in 10 kHz steps (24-bit LE; stored ÷ 100 → MHz)
/// - Byte  6:    Minimum horizontal frequency in kHz
/// - Byte  7:    Maximum horizontal frequency in kHz
/// - Bytes 8–9:  Minimum horizontal blanking in pixels (LE uint16; not stored)
/// - Byte  10:   Minimum vertical refresh rate in Hz
/// - Byte  11:   Maximum vertical refresh rate in Hz
/// - Bytes 12–13: Minimum vertical blanking in lines (LE uint16; not stored)
/// - Byte  14:   Video timing support flags (not stored)
///
/// Each field is written only when the payload is long enough to contain it.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_video_timing_range_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    // Maximum pixel clock (bytes 3–5): stored as MHz = raw_10khz / 100.
    if payload.len() >= 6 {
        let raw = (payload[3] as u32) | ((payload[4] as u32) << 8) | ((payload[5] as u32) << 16);
        caps.max_pixel_clock_mhz = Some((raw / 100) as u16);
    }

    // Horizontal frequency range (bytes 6–7).
    if payload.len() >= 8 {
        caps.min_h_rate_khz = Some(payload[6] as u16);
        caps.max_h_rate_khz = Some(payload[7] as u16);
    }

    // Vertical refresh rate range (bytes 10–11).
    if payload.len() >= 12 {
        caps.min_v_rate = Some(payload[10] as u16);
        caps.max_v_rate = Some(payload[11] as u16);
    }
}

/// Scans all data blocks in `payload` for a Video Timing Range Limits Block (tag `0x09`)
/// and decodes the first one found into `caps`.
#[cfg(any(feature = "alloc", feature = "std"))]
#[cfg(test)]
pub(super) fn scan_video_timing_range_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    let mut found = false;
    for_each_data_block(payload, |tag, _revision, block_payload| {
        if tag == TAG_VIDEO_TIMING_RANGE && !found {
            found = true;
            decode_video_timing_range_block(block_payload, caps);
        }
    });
}

/// Decodes a Product Serial Number Block payload into `caps.serial_number_string`.
///
/// Payload layout (DisplayID 1.x §4.8):
/// - Bytes 0+: ASCII serial number string (`0x0A`-terminated, space-padded).
///
/// The string is stored in the same `MonitorString` format used by EDID base-block
/// serial number descriptors (`0xFF`): up to 13 bytes, `0x0A`-terminated.
/// Empty payloads are silently ignored.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_serial_number_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    if payload.is_empty() {
        return;
    }
    let mut buf = [b' '; 13];
    let copy_len = payload.len().min(13);
    buf[..copy_len].copy_from_slice(&payload[..copy_len]);
    if !buf.contains(&0x0A) {
        buf[copy_len.min(12)] = 0x0A;
    }
    caps.serial_number_string = Some(MonitorString(buf));
}

/// Scans all data blocks in `payload` for a Product Serial Number Block (tag `0x0A`)
/// and decodes the first one found into `caps`.
#[cfg(any(feature = "alloc", feature = "std"))]
#[cfg(test)]
pub(super) fn scan_serial_number_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    let mut found = false;
    for_each_data_block(payload, |tag, _revision, block_payload| {
        if tag == TAG_SERIAL_NUMBER && !found {
            found = true;
            decode_serial_number_block(block_payload, caps);
        }
    });
}

/// Decodes a General Purpose ASCII String Block payload into the next free slot in
/// `caps.unspecified_text`.
///
/// Payload layout (DisplayID 1.x §4.9):
/// - Bytes 0+: ASCII string (`0x0A`-terminated, space-padded).
///
/// Uses the same `MonitorString` format (up to 13 bytes, `0x0A`-terminated) as the
/// EDID base-block unspecified-text descriptor. If all four `unspecified_text` slots
/// are already populated the block is silently dropped.
/// Empty payloads are silently ignored.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_ascii_string_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    if payload.is_empty() {
        return;
    }
    let Some(slot) = caps.unspecified_text.iter_mut().find(|s| s.is_none()) else {
        return;
    };
    let mut buf = [b' '; 13];
    let copy_len = payload.len().min(13);
    buf[..copy_len].copy_from_slice(&payload[..copy_len]);
    if !buf.contains(&0x0A) {
        buf[copy_len.min(12)] = 0x0A;
    }
    *slot = Some(MonitorString(buf));
}

/// Decodes a Display Device Data Block payload into `caps`.
///
/// Payload layout (DisplayID 1.x §4.10, 13 bytes):
/// - Byte  0:    Bits 7:4 = display technology; bits 3:0 = sub-type code
/// - Byte  1:    Bits 3:0 = operating mode; bits 5:4 = backlight type;
///   bit 6 = DE signal used; bit 7 = DE polarity (1 = positive)
/// - Bytes 2–3:  Horizontal native pixel count (LE uint16; 0 = not defined)
/// - Bytes 4–5:  Vertical native pixel count (LE uint16; 0 = not defined)
/// - Byte  6:    Aspect ratio = byte / 100 + 1 (raw value stored as-is)
/// - Byte  7:    Bits 1:0 = physical orientation; bits 3:2 = rotation capability;
///   bits 5:4 = zero pixel location; bits 7:6 = scan direction
/// - Byte  8:    RGB sub-pixel layout code
/// - Byte  9:    Horizontal pixel pitch in 0.01 mm steps (0 = not defined)
/// - Byte 10:    Vertical pixel pitch in 0.01 mm steps (0 = not defined)
/// - Byte 11:    Color bit depth: bits 3:0 = bpc − 1
/// - Byte 12:    Pixel response time in ms (0 = not defined)
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_display_device_data_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    // Byte 0: display technology (bits 7:4) and sub-type (bits 3:0).
    if !payload.is_empty() {
        caps.display_technology = Some(DisplayTechnology::from_nibble(payload[0] >> 4));
        caps.display_subtype = Some(payload[0] & 0x0F);
    }

    // Byte 1: operating mode (bits 3:0), backlight type (bits 5:4), DE flags (bits 7:6).
    if payload.len() >= 2 {
        caps.operating_mode = Some(OperatingMode::from_nibble(payload[1] & 0x0F));
        caps.backlight_type = Some(BacklightType::from_bits((payload[1] >> 4) & 0x03));
        caps.data_enable_used = Some((payload[1] & 0x40) != 0);
        caps.data_enable_positive = Some((payload[1] & 0x80) != 0);
    }

    // Bytes 2–5: native pixel format (h × v, LE uint16 each; 0 = not defined).
    if payload.len() >= 6 {
        let h = u16::from_le_bytes([payload[2], payload[3]]);
        let v = u16::from_le_bytes([payload[4], payload[5]]);
        if h != 0 && v != 0 {
            caps.native_pixels = Some((h, v));
        }
    }

    // Byte 6: aspect ratio raw byte ((AR − 1) × 100).
    if payload.len() >= 7 {
        caps.panel_aspect_ratio_100 = Some(payload[6]);
    }

    // Byte 7: orientation flags.
    if payload.len() >= 8 {
        caps.physical_orientation = Some(PhysicalOrientation::from_bits(payload[7] & 0x03));
        caps.rotation_capability = Some(RotationCapability::from_bits((payload[7] >> 2) & 0x03));
        caps.zero_pixel_location = Some(ZeroPixelLocation::from_bits((payload[7] >> 4) & 0x03));
        caps.scan_direction = Some(ScanDirection::from_bits((payload[7] >> 6) & 0x03));
    }

    // Byte 8: sub-pixel layout.
    if payload.len() >= 9 {
        caps.subpixel_layout = Some(SubpixelLayout::from_byte(payload[8]));
    }

    // Bytes 9–10: pixel pitch H and V in 0.01 mm steps (0 = not defined).
    if payload.len() >= 11 {
        let h_pitch = payload[9];
        let v_pitch = payload[10];
        if h_pitch != 0 && v_pitch != 0 {
            caps.pixel_pitch_hundredths_mm = Some((h_pitch, v_pitch));
        }
    }

    // Byte 11: color bit depth (bits 3:0 = bpc − 1).
    // Convert: bpc = raw + 1; EDID-style mapping: 6→1, 8→2, 10→3, 12→4, 14→5, 16→6.
    if payload.len() >= 12 {
        let bpc = (payload[11] & 0x0F) + 1;
        let edid_bits: u8 = match bpc {
            6 => 1,
            8 => 2,
            10 => 3,
            12 => 4,
            14 => 5,
            16 => 6,
            _ => 0,
        };
        caps.color_bit_depth = decode_color_bit_depth(edid_bits);
    }

    // Byte 12: pixel response time in ms (0 = not defined).
    if payload.len() >= 13 {
        let rt = payload[12];
        if rt != 0 {
            caps.pixel_response_time_ms = Some(rt);
        }
    }
}

/// Scans all data blocks in `payload` for a Display Device Data Block (tag `0x0C`)
/// and decodes the first one found into `caps`.
#[cfg(any(feature = "alloc", feature = "std"))]
#[cfg(test)]
pub(super) fn scan_display_device_data_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    let mut found = false;
    for_each_data_block(payload, |tag, _revision, block_payload| {
        if tag == TAG_DISPLAY_DEVICE_DATA && !found {
            found = true;
            decode_display_device_data_block(block_payload, caps);
        }
    });
}

/// Decodes an Interface Power Sequencing Block payload into `caps`.
///
/// Payload layout (DisplayID 1.x §4.11, 8 bytes):
/// - Byte 0:  T1 minimum — power supply enable to interface signal valid (2 ms units)
/// - Byte 1:  T2 minimum — interface signal enable to backlight enable (2 ms units)
/// - Byte 2:  T3 minimum — backlight disable to interface signal disable (2 ms units)
/// - Byte 3:  T4 minimum — interface signal disable to power supply disable (2 ms units)
/// - Byte 4:  T5 minimum — power supply off time (2 ms units)
/// - Byte 5:  T6 minimum — backlight off time (2 ms units)
/// - Bytes 6–7: Reserved (ignored)
///
/// Payloads shorter than 6 bytes are silently skipped.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_power_sequencing_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    if payload.len() >= 6 {
        caps.power_sequencing = Some(PowerSequencing::new(
            payload[0], payload[1], payload[2], payload[3], payload[4], payload[5],
        ));
    }
}

/// Scans all data blocks in `payload` for an Interface Power Sequencing Block (tag `0x0D`)
/// and decodes the first one found into `caps`.
#[cfg(any(feature = "alloc", feature = "std"))]
#[cfg(test)]
pub(super) fn scan_power_sequencing_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    let mut found = false;
    for_each_data_block(payload, |tag, _revision, block_payload| {
        if tag == TAG_POWER_SEQUENCING && !found {
            found = true;
            decode_power_sequencing_block(block_payload, caps);
        }
    });
}

/// Decodes a Transfer Characteristics Block payload into `caps`.
///
/// Payload layout (DisplayID 1.x §4.12):
/// - Byte 0 bits 7:6: Point encoding — `00` = 8-bit, `01` = 10-bit, `10` = 12-bit
/// - Byte 0 bit 5: Multi-channel flag — when set, sample data encodes three equal-length
///   sequential regions: red, green, blue (in that order)
/// - Bytes 1+: Packed sample data (see encoding variants below)
///
/// **8-bit encoding** — 1 byte per point, values 0–255, normalized to `[0.0, 1.0]`.
///
/// **10-bit encoding** — 5 bytes per 4 points, packed MSB-first:
/// ```text
/// byte0[7:0] = p0[9:2]
/// byte1[7:6] = p0[1:0],  byte1[5:0] = p1[9:4]
/// byte2[7:4] = p1[3:0],  byte2[3:0] = p2[9:6]
/// byte3[7:2] = p2[5:0],  byte3[1:0] = p3[9:8]
/// byte4[7:0] = p3[7:0]
/// ```
///
/// **12-bit encoding** — 3 bytes per 2 points, packed MSB-first:
/// ```text
/// byte0[7:0] = p0[11:4]
/// byte1[7:4] = p0[3:0],  byte1[3:0] = p1[11:8]
/// byte2[7:0] = p1[7:0]
/// ```
///
/// Payloads with a reserved encoding byte (bits 7:6 = `11`) push an
/// [`EdidWarning::UnknownTransferEncoding`] warning and are otherwise skipped.
/// Payloads shorter than 2 bytes are silently skipped.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_transfer_characteristics_block(
    payload: &[u8],
    caps: &mut DisplayCapabilities,
) {
    if payload.len() < 2 {
        return;
    }

    let encoding = match (payload[0] >> 6) & 0x03 {
        0x00 => TransferPointEncoding::Bits8,
        0x01 => TransferPointEncoding::Bits10,
        0x02 => TransferPointEncoding::Bits12,
        bits => {
            caps.warnings
                .push(Arc::new(EdidWarning::UnknownTransferEncoding(bits)));
            return;
        }
    };
    let multi_channel = (payload[0] & 0x20) != 0;

    let data = &payload[1..];

    /// Unpack all 8-bit samples from `src` into normalized `[0.0, 1.0]` f32 values.
    fn unpack8(src: &[u8]) -> Vec<f32> {
        src.iter().map(|&b| b as f32 / 255.0).collect()
    }

    /// Unpack all 10-bit samples (5 bytes per 4 points) from `src`.
    fn unpack10(src: &[u8]) -> Vec<f32> {
        let mut pts = Vec::new();
        let mut i = 0;
        while i + 5 <= src.len() {
            let [b0, b1, b2, b3, b4] = [
                src[i] as u16,
                src[i + 1] as u16,
                src[i + 2] as u16,
                src[i + 3] as u16,
                src[i + 4] as u16,
            ];
            pts.push(((b0 << 2) | (b1 >> 6)) as f32 / 1023.0);
            pts.push((((b1 & 0x3F) << 4) | (b2 >> 4)) as f32 / 1023.0);
            pts.push((((b2 & 0x0F) << 6) | (b3 >> 2)) as f32 / 1023.0);
            pts.push((((b3 & 0x03) << 8) | b4) as f32 / 1023.0);
            i += 5;
        }
        pts
    }

    /// Unpack all 12-bit samples (3 bytes per 2 points) from `src`.
    fn unpack12(src: &[u8]) -> Vec<f32> {
        let mut pts = Vec::new();
        let mut i = 0;
        while i + 3 <= src.len() {
            let [b0, b1, b2] = [src[i] as u16, src[i + 1] as u16, src[i + 2] as u16];
            pts.push(((b0 << 4) | (b1 >> 4)) as f32 / 4095.0);
            pts.push((((b1 & 0x0F) << 8) | b2) as f32 / 4095.0);
            i += 3;
        }
        pts
    }

    let curve = if multi_channel {
        // Sample data is three equal sequential regions: red, green, blue.
        let total = data.len();
        if total % 3 != 0 {
            return; // malformed: cannot split evenly
        }
        let region = total / 3;
        let (r_data, rest) = data.split_at(region);
        let (g_data, b_data) = rest.split_at(region);
        let (red, green, blue) = match encoding {
            TransferPointEncoding::Bits8 => (unpack8(r_data), unpack8(g_data), unpack8(b_data)),
            TransferPointEncoding::Bits10 => (unpack10(r_data), unpack10(g_data), unpack10(b_data)),
            TransferPointEncoding::Bits12 => (unpack12(r_data), unpack12(g_data), unpack12(b_data)),
            _ => return,
        };
        TransferCurve::Rgb { red, green, blue }
    } else {
        let points = match encoding {
            TransferPointEncoding::Bits8 => unpack8(data),
            TransferPointEncoding::Bits10 => unpack10(data),
            TransferPointEncoding::Bits12 => unpack12(data),
            _ => return,
        };
        TransferCurve::Luminance(points)
    };

    caps.transfer_characteristic = Some(DisplayIdTransferCharacteristic::new(encoding, curve));
}

/// Scans all data blocks in `payload` for a Transfer Characteristics Block (tag `0x0E`)
/// and decodes the first one found into `caps`.
#[cfg(any(feature = "alloc", feature = "std"))]
#[cfg(test)]
pub(super) fn scan_transfer_characteristics_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    let mut found = false;
    for_each_data_block(payload, |tag, _revision, block_payload| {
        if tag == TAG_TRANSFER_CHARACTERISTICS && !found {
            found = true;
            decode_transfer_characteristics_block(block_payload, caps);
        }
    });
}

/// Decodes a Display Interface Data Block payload into `caps`.
///
/// Payload layout (DisplayID 1.x §4.13, minimum 7 bytes):
/// - Byte 0 bits 3:0: Interface type — 0=undefined, 1=analog, 2=LVDS single, 3=LVDS dual,
///   4=TMDS single, 5=TMDS dual, 6=eDP, 7=DisplayPort, 8=proprietary, 9–F=reserved
/// - Byte 0 bit 4:   Spread spectrum clocking supported
/// - Byte 0 bits 7:5: Reserved
/// - Byte 1 bits 3:0: Number of data lanes / LVDS pairs (raw count)
/// - Byte 1 bits 7:4: Reserved
/// - Bytes 2–3:       Minimum pixel clock, LE uint16, in units of 10 kHz
/// - Bytes 4–5:       Maximum pixel clock, LE uint16, in units of 10 kHz
/// - Byte 6 bits 1:0: Content protection type (0=none, 1=HDCP, 2=DPCP)
/// - Byte 6 bits 7:2: Reserved
///
/// Payloads shorter than 7 bytes are silently skipped.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_display_interface_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    if payload.len() < 7 {
        return;
    }
    let interface_type = DisplayInterfaceType::from_nibble(payload[0]);
    let spread_spectrum = (payload[0] & 0x10) != 0;
    let num_lanes = payload[1] & 0x0F;
    let min_pixel_clock_10khz = u32::from(u16::from_le_bytes([payload[2], payload[3]]));
    let max_pixel_clock_10khz = u32::from(u16::from_le_bytes([payload[4], payload[5]]));
    let content_protection = InterfaceContentProtection::from_bits(payload[6]);

    caps.display_id_interface = Some(DisplayIdInterface::new(
        interface_type,
        spread_spectrum,
        num_lanes,
        min_pixel_clock_10khz,
        max_pixel_clock_10khz,
        content_protection,
    ));
}

/// Scans all data blocks in `payload` for a Display Interface Data Block (tag `0x0F`)
/// and decodes the first one found into `caps`.
#[cfg(any(feature = "alloc", feature = "std"))]
#[cfg(test)]
pub(super) fn scan_display_interface_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    let mut found = false;
    for_each_data_block(payload, |tag, _revision, block_payload| {
        if tag == TAG_DISPLAY_INTERFACE && !found {
            found = true;
            decode_display_interface_block(block_payload, caps);
        }
    });
}

/// Decodes a Stereo Display Interface Data Block payload into `caps`.
///
/// Payload layout (DisplayID 1.x §4.14, minimum 2 bytes):
/// - Byte 0 bits 3:0: Stereo viewing mode
///   0=field sequential, 1=side-by-side, 2=top-and-bottom,
///   3=row interleaved, 4=column interleaved, 5=pixel interleaved, 6–15=reserved
/// - Byte 0 bit 4: 3D sync signal polarity (1=positive, 0=negative)
///   Only meaningful for field sequential mode.
/// - Byte 0 bits 7:5: Reserved
/// - Byte 1: Stereo sync interface type (how sync reaches the glasses)
///   0=via display connector, 1=VESA 3-pin DIN, 2=infrared, 3=RF wireless, 4+=reserved
///
/// Payloads shorter than 2 bytes are silently skipped.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_stereo_display_interface_block(
    payload: &[u8],
    caps: &mut DisplayCapabilities,
) {
    if payload.len() < 2 {
        return;
    }
    let viewing_mode = StereoViewingMode::from_nibble(payload[0]);
    let sync_polarity_positive = (payload[0] & 0x10) != 0;
    let sync_interface = StereoSyncInterface::from_byte(payload[1]);

    caps.stereo_interface = Some(DisplayIdStereoInterface::new(
        viewing_mode,
        sync_polarity_positive,
        sync_interface,
    ));
}

/// Scans all data blocks in `payload` for a Stereo Display Interface Data Block (tag `0x10`)
/// and decodes the first one found into `caps`.
#[cfg(any(feature = "alloc", feature = "std"))]
#[cfg(test)]
pub(super) fn scan_stereo_display_interface_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    let mut found = false;
    for_each_data_block(payload, |tag, _revision, block_payload| {
        if tag == TAG_STEREO_DISPLAY_INTERFACE && !found {
            found = true;
            decode_stereo_display_interface_block(block_payload, caps);
        }
    });
}

/// Decodes a Tiled Display Topology Data Block payload into `caps`.
///
/// Payload layout (DisplayID 1.x §4.15, minimum 7 bytes):
/// - Byte 0 bit 7:   Single enclosure (1 = all tiles in the same physical case)
/// - Byte 0 bit 6:   Has bezel information (if 1, bytes 7–10 contain bezel sizes)
/// - Byte 0 bits 5:4: Topology behavior when tiles are missing
///   0=undefined, 1=no image until all present, 2=scale when missing, 3=reserved
/// - Byte 0 bits 3:0: Reserved
/// - Byte 1 bits 7:4: Number of horizontal tiles minus 1 (0 → 1 tile … 15 → 16 tiles)
/// - Byte 1 bits 3:0: Number of vertical tiles minus 1
/// - Byte 2 bits 7:4: Zero-based column index of this tile
/// - Byte 2 bits 3:0: Zero-based row index of this tile
/// - Bytes 3–4:       Tile pixel width (LE uint16)
/// - Bytes 5–6:       Tile pixel height (LE uint16)
/// - Bytes 7–10 (optional, when bit 6 of byte 0 is set):
///   Byte 7: Top bezel in pixels
///   Byte 8: Bottom bezel in pixels
///   Byte 9: Right bezel in pixels
///   Byte 10: Left bezel in pixels
///
/// Payloads shorter than 7 bytes are silently skipped.
/// Bezel info is decoded only when the flag is set and at least 11 bytes are present.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn decode_tiled_topology_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    if payload.len() < 7 {
        return;
    }
    let single_enclosure = (payload[0] & 0x80) != 0;
    let has_bezel_info = (payload[0] & 0x40) != 0;
    let topology_behavior = TileTopologyBehavior::from_bits((payload[0] >> 4) & 0x03);

    let h_tile_count = (payload[1] >> 4) + 1;
    let v_tile_count = (payload[1] & 0x0F) + 1;
    let h_tile_location = payload[2] >> 4;
    let v_tile_location = payload[2] & 0x0F;

    let tile_width_px = u16::from_le_bytes([payload[3], payload[4]]);
    let tile_height_px = u16::from_le_bytes([payload[5], payload[6]]);

    let bezel = if has_bezel_info && payload.len() >= 11 {
        Some(TileBezelInfo::new(
            payload[7],
            payload[8],
            payload[9],
            payload[10],
        ))
    } else {
        None
    };

    caps.tiled_topology = Some(DisplayIdTiledTopology::new(
        single_enclosure,
        topology_behavior,
        h_tile_count,
        v_tile_count,
        h_tile_location,
        v_tile_location,
        tile_width_px,
        tile_height_px,
        bezel,
    ));
}

/// Scans all data blocks in `payload` for a Tiled Display Topology Data Block (tag `0x12`)
/// and decodes the first one found into `caps`.
#[cfg(any(feature = "alloc", feature = "std"))]
#[cfg(test)]
pub(super) fn scan_tiled_topology_block(payload: &[u8], caps: &mut DisplayCapabilities) {
    let mut found = false;
    for_each_data_block(payload, |tag, _revision, block_payload| {
        if tag == TAG_TILED_TOPOLOGY && !found {
            found = true;
            decode_tiled_topology_block(block_payload, caps);
        }
    });
}

/// Scans all DisplayID metadata blocks in a single pass.
///
/// Calls [`for_each_data_block`] once over `payload` and dispatches every
/// recognised metadata tag to the appropriate `decode_*` function. Single-
/// instance tags (every tag except [`TAG_ASCII_STRING`]) are guarded by a
/// bool so that only the first occurrence takes effect.
///
/// This replaces the individual `scan_*` calls in the alloc pipeline and
/// reduces the number of passes over the payload from one-per-tag to one.
///
/// `version` is the DisplayID version byte from the section header. The decoder
/// dispatches to the V1 or V2 metadata block tags based on `version`. The V2 path
/// also writes into `did` (e.g., `manufacturer_oui` from block 0x20).
#[cfg(any(feature = "alloc", feature = "std"))]
pub(super) fn scan_all_metadata_blocks(
    payload: &[u8],
    version: u8,
    caps: &mut DisplayCapabilities,
    did: &mut DisplayIdCapabilities,
    warnings: &mut Vec<ParseWarning>,
    options: &DecodeOptions,
) {
    if version == DISPLAYID_V2 {
        let mut found_v2_product_id = false;
        let mut found_v2_display_params = false;
        let mut found_v2_dynamic_timing_range = false;
        let mut found_v2_interface_features = false;
        let mut found_v2_stereo_interface = false;
        let mut found_v2_tiled_topology = false;
        let mut found_v2_container_id = false;
        for_each_data_block(payload, |tag, revision, block_payload| match tag {
            TAG_V2_PRODUCT_ID if !found_v2_product_id => {
                found_v2_product_id = true;
                decode_v2_product_id_block(block_payload, caps, did);
            }
            TAG_V2_DISPLAY_PARAMS if !found_v2_display_params => {
                found_v2_display_params = true;
                decode_v2_display_params_block(block_payload, revision, caps, did);
            }
            TAG_V2_DYNAMIC_TIMING_RANGE if !found_v2_dynamic_timing_range => {
                found_v2_dynamic_timing_range = true;
                decode_v2_dynamic_timing_range_block(block_payload, revision, caps, did);
            }
            TAG_V2_INTERFACE_FEATURES if !found_v2_interface_features => {
                found_v2_interface_features = true;
                decode_v2_interface_features_block(block_payload, did);
            }
            TAG_V2_STEREO_INTERFACE if !found_v2_stereo_interface => {
                found_v2_stereo_interface = true;
                decode_v2_stereo_interface_block(block_payload, revision, did);
            }
            TAG_V2_TILED_TOPOLOGY if !found_v2_tiled_topology => {
                found_v2_tiled_topology = true;
                decode_tiled_topology_block(block_payload, caps);
            }
            TAG_V2_CONTAINER_ID if !found_v2_container_id => {
                found_v2_container_id = true;
                decode_v2_container_id_block(block_payload, did);
            }
            TAG_V2_VENDOR_SPECIFIC => {
                decode_v2_vendor_specific_block(block_payload, did);
            }
            TAG_V2_CTA_DISPLAYID => {
                decode_v2_cta_displayid_block(block_payload, revision, caps, warnings, options);
            }
            _ => {}
        });
        return;
    }
    let mut found_product_id = false;
    let mut found_display_params = false;
    let mut found_color_characteristics = false;
    let mut found_video_timing_range = false;
    let mut found_serial_number = false;
    let mut found_display_device_data = false;
    let mut found_power_sequencing = false;
    let mut found_transfer_characteristics = false;
    let mut found_display_interface = false;
    let mut found_stereo_display_interface = false;
    let mut found_tiled_topology = false;

    for_each_data_block(payload, |tag, _revision, block_payload| match tag {
        TAG_PRODUCT_ID if !found_product_id => {
            found_product_id = true;
            decode_product_id_block(block_payload, caps);
        }
        TAG_DISPLAY_PARAMS if !found_display_params => {
            found_display_params = true;
            decode_display_params_block(block_payload, caps, did);
        }
        TAG_COLOR_CHARACTERISTICS if !found_color_characteristics => {
            found_color_characteristics = true;
            decode_color_characteristics_block(block_payload, caps);
        }
        TAG_VIDEO_TIMING_RANGE if !found_video_timing_range => {
            found_video_timing_range = true;
            decode_video_timing_range_block(block_payload, caps);
        }
        TAG_SERIAL_NUMBER if !found_serial_number => {
            found_serial_number = true;
            decode_serial_number_block(block_payload, caps);
        }
        TAG_ASCII_STRING => {
            decode_ascii_string_block(block_payload, caps);
        }
        TAG_DISPLAY_DEVICE_DATA if !found_display_device_data => {
            found_display_device_data = true;
            decode_display_device_data_block(block_payload, caps);
        }
        TAG_POWER_SEQUENCING if !found_power_sequencing => {
            found_power_sequencing = true;
            decode_power_sequencing_block(block_payload, caps);
        }
        TAG_TRANSFER_CHARACTERISTICS if !found_transfer_characteristics => {
            found_transfer_characteristics = true;
            decode_transfer_characteristics_block(block_payload, caps);
        }
        TAG_DISPLAY_INTERFACE if !found_display_interface => {
            found_display_interface = true;
            decode_display_interface_block(block_payload, caps);
        }
        TAG_STEREO_DISPLAY_INTERFACE if !found_stereo_display_interface => {
            found_stereo_display_interface = true;
            decode_stereo_display_interface_block(block_payload, caps);
        }
        TAG_TILED_TOPOLOGY if !found_tiled_topology => {
            found_tiled_topology = true;
            decode_tiled_topology_block(block_payload, caps);
        }
        _ => {}
    });
}

#[cfg(test)]
#[cfg(any(feature = "alloc", feature = "std"))]
mod tests {
    use super::*;
    use crate::model::color::{Chromaticity, ColorBitDepth};
    use crate::model::manufacture::{ManufactureDate, ManufacturerId, MonitorString};

    // -----------------------------------------------------------------------
    // Shared test helpers
    // -----------------------------------------------------------------------

    fn make_product_id_payload(
        manufacturer_raw: u16, // packed PNP encoding
        product_code: u16,
        serial: u32,
        week: u8,
        year_offset: u8, // actual year = offset + 1990
        name: Option<&[u8]>,
    ) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&manufacturer_raw.to_be_bytes());
        v.extend_from_slice(&product_code.to_le_bytes());
        v.extend_from_slice(&serial.to_le_bytes());
        v.push(week);
        v.push(year_offset);
        if let Some(n) = name {
            v.extend_from_slice(n);
        }
        v
    }

    fn make_display_params_payload(
        h_tenths_mm: u16,
        v_tenths_mm: u16,
        h_native_px: u16,
        v_native_px: u16,
        aspect_byte: u8, // (AR − 1) × 100
        depth_byte: u8,  // low nibble = native bpc − 1
    ) -> [u8; 12] {
        let mut p = [0u8; 12];
        p[0..2].copy_from_slice(&h_tenths_mm.to_le_bytes());
        p[2..4].copy_from_slice(&v_tenths_mm.to_le_bytes());
        p[4..6].copy_from_slice(&h_native_px.to_le_bytes());
        p[6..8].copy_from_slice(&v_native_px.to_le_bytes());
        // p[8]: feature flags (zeroed)
        // p[9]: gamma (zeroed)
        p[10] = aspect_byte;
        p[11] = depth_byte;
        p
    }

    fn make_color_characteristics_payload(
        red: (u16, u16),
        green: (u16, u16),
        blue: (u16, u16),
        white: (u16, u16),
    ) -> [u8; 16] {
        let mut p = [0u8; 16];
        let mut write = |offset: usize, val: (u16, u16)| {
            p[offset..offset + 2].copy_from_slice(&val.0.to_le_bytes());
            p[offset + 2..offset + 4].copy_from_slice(&val.1.to_le_bytes());
        };
        write(0, red);
        write(4, green);
        write(8, blue);
        write(12, white);
        p
    }

    /// Pack three ASCII uppercase letters into the 2-byte PNP manufacturer ID encoding.
    fn pack_manufacturer_id(a: u8, b: u8, c: u8) -> u16 {
        let ca = (a - b'A' + 1) as u16;
        let cb = (b - b'A' + 1) as u16;
        let cc = (c - b'A' + 1) as u16;
        (ca << 10) | (cb << 5) | cc
    }

    // -----------------------------------------------------------------------
    // Product Identification Block (tag 0x00)
    // -----------------------------------------------------------------------

    #[test]
    fn test_product_id_manufacturer_and_product_code() {
        let packed = pack_manufacturer_id(b'S', b'A', b'M');
        let payload = make_product_id_payload(packed, 0x1234, 0, 0, 0, None);
        let mut caps = DisplayCapabilities::default();
        decode_product_id_block(&payload, &mut caps);
        assert_eq!(caps.manufacturer, Some(ManufacturerId(*b"SAM")));
        assert_eq!(caps.product_code, Some(0x1234));
    }

    #[test]
    fn test_product_id_serial_number() {
        let packed = pack_manufacturer_id(b'D', b'E', b'L');
        let payload = make_product_id_payload(packed, 0x0001, 0xDEADBEEF, 0, 0, None);
        let mut caps = DisplayCapabilities::default();
        decode_product_id_block(&payload, &mut caps);
        assert_eq!(caps.serial_number, Some(0xDEAD_BEEF));
    }

    #[test]
    fn test_product_id_zero_serial_not_stored() {
        let packed = pack_manufacturer_id(b'G', b'S', b'M');
        let payload = make_product_id_payload(packed, 0x0001, 0, 0, 0, None);
        let mut caps = DisplayCapabilities::default();
        decode_product_id_block(&payload, &mut caps);
        assert_eq!(caps.serial_number, None);
    }

    #[test]
    fn test_product_id_manufacture_date() {
        let packed = pack_manufacturer_id(b'A', b'P', b'L');
        // Week 10, year 2020 → year_byte = 2020 - 1990 = 30
        let payload = make_product_id_payload(packed, 0x0001, 0, 10, 30, None);
        let mut caps = DisplayCapabilities::default();
        decode_product_id_block(&payload, &mut caps);
        assert_eq!(
            caps.manufacture_date,
            Some(ManufactureDate::Manufactured {
                week: Some(10),
                year: 2020
            })
        );
    }

    #[test]
    fn test_product_id_display_name() {
        let packed = pack_manufacturer_id(b'H', b'W', b'P');
        let name: &[u8] = b"Z27k G2\x0a     ";
        let payload = make_product_id_payload(packed, 0x0042, 0, 0, 34, Some(name));
        let mut caps = DisplayCapabilities::default();
        decode_product_id_block(&payload, &mut caps);
        assert_eq!(caps.display_name.as_deref(), Some("Z27k G2"));
    }

    #[test]
    fn test_product_id_too_short_does_not_panic() {
        // Only 1 byte — too short for any field.
        let payload = [0xFFu8];
        let mut caps = DisplayCapabilities::default();
        decode_product_id_block(&payload, &mut caps);
        assert_eq!(caps.manufacturer, None);
        assert_eq!(caps.product_code, None);
    }

    // -----------------------------------------------------------------------
    // V2 Product Identification Block (tag 0x20)
    // -----------------------------------------------------------------------

    fn make_v2_product_id_payload(
        oui: [u8; 3],
        product_code: u16,
        serial: u32,
        week: u8,
        year_offset: u8, // actual year = offset + 2000
        name: Option<&[u8]>,
    ) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&oui);
        v.extend_from_slice(&product_code.to_le_bytes());
        v.extend_from_slice(&serial.to_le_bytes());
        v.push(week);
        v.push(year_offset);
        match name {
            Some(n) => {
                v.push(n.len() as u8);
                v.extend_from_slice(n);
            }
            None => v.push(0),
        }
        v
    }

    #[test]
    fn test_v2_product_id_oui_and_product_code() {
        // 00-1A-7E is the LG Electronics OUI.
        let payload = make_v2_product_id_payload([0x00, 0x1A, 0x7E], 0x1234, 0, 0, 0, None);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_product_id_block(&payload, &mut caps, &mut did);
        assert_eq!(did.manufacturer_oui, Some([0x00, 0x1A, 0x7E]));
        assert_eq!(caps.product_code, Some(0x1234));
        // V2 must not populate the V1 PNP-encoded manufacturer field.
        assert_eq!(caps.manufacturer, None);
    }

    #[test]
    fn test_v2_product_id_serial_number() {
        let payload =
            make_v2_product_id_payload([0xAA, 0xBB, 0xCC], 0x0001, 0xDEAD_BEEF, 0, 0, None);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_product_id_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.serial_number, Some(0xDEAD_BEEF));
    }

    #[test]
    fn test_v2_product_id_zero_serial_not_stored() {
        let payload = make_v2_product_id_payload([0xAA, 0xBB, 0xCC], 0x0001, 0, 0, 0, None);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_product_id_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.serial_number, None);
    }

    #[test]
    fn test_v2_product_id_year_uses_2000_offset() {
        // Week 10, year 2024 → year_byte = 2024 - 2000 = 24.
        let payload = make_v2_product_id_payload([0x00, 0x1A, 0x7E], 0x0001, 0, 10, 24, None);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_product_id_block(&payload, &mut caps, &mut did);
        assert_eq!(
            caps.manufacture_date,
            Some(ManufactureDate::Manufactured {
                week: Some(10),
                year: 2024,
            })
        );
    }

    #[test]
    fn test_v2_product_id_model_year() {
        // week = 0xFF marks the year as a model year.
        let payload = make_v2_product_id_payload([0x00, 0x1A, 0x7E], 0x0001, 0, 0xFF, 25, None);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_product_id_block(&payload, &mut caps, &mut did);
        assert_eq!(
            caps.manufacture_date,
            Some(ManufactureDate::ModelYear(2025))
        );
    }

    #[test]
    fn test_v2_product_id_display_name() {
        let name: &[u8] = b"UltraGear";
        let payload = make_v2_product_id_payload([0x00, 0x1A, 0x7E], 0x0042, 0, 0, 24, Some(name));
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_product_id_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.display_name.as_deref(), Some("UltraGear"));
    }

    #[test]
    fn test_v2_product_id_long_name_truncated() {
        // Names longer than MonitorString's 13-byte buffer are tail-truncated to fit;
        // all 13 bytes are kept (`MonitorString::as_str` falls back to the full buffer
        // when no `0x0A` terminator is present).
        let name: &[u8] = b"Big Long Display Name";
        let payload = make_v2_product_id_payload([0x00, 0x1A, 0x7E], 0x0001, 0, 0, 24, Some(name));
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_product_id_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.display_name.as_deref(), Some("Big Long Disp"));
    }

    #[test]
    fn test_v2_product_id_exact_13_byte_name_preserved() {
        // Boundary: name is exactly 13 bytes, no embedded 0x0A. All 13 chars must
        // survive — earlier code lost the last char to a forced terminator.
        let name: &[u8] = b"ThirteenChars";
        assert_eq!(name.len(), 13);
        let payload = make_v2_product_id_payload([0x00, 0x1A, 0x7E], 0x0001, 0, 0, 24, Some(name));
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_product_id_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.display_name.as_deref(), Some("ThirteenChars"));
    }

    #[test]
    fn test_v2_product_id_zero_length_name_skipped() {
        let payload = make_v2_product_id_payload([0x00, 0x1A, 0x7E], 0x0001, 0, 0, 24, None);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_product_id_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.display_name, None);
    }

    #[test]
    fn test_v2_product_id_truncated_name_payload_skipped() {
        // Length byte declares 20 bytes but only 4 follow — name field must be ignored.
        let mut payload =
            make_v2_product_id_payload([0x00, 0x1A, 0x7E], 0x0001, 0, 0, 24, Some(b"abcd"));
        // Overwrite the name length byte at offset 11 to 20.
        payload[11] = 20;
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_product_id_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.display_name, None);
        // Other fields still decoded.
        assert_eq!(did.manufacturer_oui, Some([0x00, 0x1A, 0x7E]));
    }

    #[test]
    fn test_v2_product_id_too_short_does_not_panic() {
        // Two bytes — not even enough for the OUI.
        let payload = [0x00u8, 0x1A];
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_product_id_block(&payload, &mut caps, &mut did);
        assert_eq!(did.manufacturer_oui, None);
        assert_eq!(caps.product_code, None);
    }

    #[test]
    fn test_v2_product_id_dispatched_only_for_v2_section() {
        // A 0x20 block that, on a V1 section, would parse as garbage. On a V1 section,
        // 0x20 is outside the V1 metadata tag space and the V2 decoder must not run.
        let body = make_v2_product_id_payload([0x00, 0x1A, 0x7E], 0xAAAA, 0, 0, 24, None);
        let mut block_payload = Vec::new();
        block_payload.push(TAG_V2_PRODUCT_ID); // tag
        block_payload.push(0x00); // revision
        block_payload.push(body.len() as u8); // length
        block_payload.extend_from_slice(&body);

        // V1 section: nothing should be decoded.
        let mut caps_v1 = DisplayCapabilities::default();
        let mut did_v1 = DisplayIdCapabilities::new(0x13, 0);
        scan_all_metadata_blocks(
            &block_payload,
            0x13,
            &mut caps_v1,
            &mut did_v1,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert_eq!(did_v1.manufacturer_oui, None);
        assert_eq!(caps_v1.product_code, None);

        // V2 section: decoder runs.
        let mut caps_v2 = DisplayCapabilities::default();
        let mut did_v2 = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &block_payload,
            DISPLAYID_V2,
            &mut caps_v2,
            &mut did_v2,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert_eq!(did_v2.manufacturer_oui, Some([0x00, 0x1A, 0x7E]));
        assert_eq!(caps_v2.product_code, Some(0xAAAA));
    }

    // -----------------------------------------------------------------------
    // V2 Display Parameters Block (tag 0x21)
    // -----------------------------------------------------------------------

    /// Encodes a 12-bit (x_raw, y_raw) chromaticity pair into the 24-bit packed layout
    /// used by DisplayID 2.x block 0x21 primaries / white point.
    fn pack_chromaticity12(x: u16, y: u16) -> [u8; 3] {
        let x = x & 0x0FFF;
        let y = y & 0x0FFF;
        [
            (x & 0xFF) as u8,
            (((x >> 8) & 0x0F) | ((y & 0x0F) << 4)) as u8,
            ((y >> 4) & 0xFF) as u8,
        ]
    }

    /// Encodes an `f32` luminance value (cd/m²) into IEEE 754 binary16 little-endian.
    /// Limited to positive normals — this is enough for the cd/m² values used in tests.
    fn encode_luminance_f16(v: f32) -> [u8; 2] {
        let bits = v.to_bits();
        let sign = (bits >> 31) & 0x1;
        let exp_f32 = ((bits >> 23) & 0xFF) as i32;
        let mant_f32 = bits & 0x7F_FFFF;
        let exp_f16 = exp_f32 - 127 + 15;
        assert!(
            (1..=30).contains(&exp_f16),
            "encode_luminance_f16 only supports normal binary16 values"
        );
        let mant_f16 = (mant_f32 >> 13) & 0x3FF;
        let raw = ((sign as u16) << 15) | ((exp_f16 as u16) << 10) | (mant_f16 as u16);
        raw.to_le_bytes()
    }

    #[allow(clippy::too_many_arguments)] // mirrors the wire-format field list
    fn make_v2_display_params_payload(
        h_size: u16,
        v_size: u16,
        h_pixels: u16,
        v_pixels: u16,
        flags: u8,
        chromaticity: (u16, u16, u16, u16, u16, u16, u16, u16), // r, g, b, w (x,y) interleaved
        max_lum_full: u16,                                      // raw f16 LE bits
        max_lum_10pct: u16,
        min_lum: u16,
        depth_tech_byte: u8,
        gamma_byte: u8,
    ) -> [u8; 29] {
        let mut p = [0u8; 29];
        p[0..2].copy_from_slice(&h_size.to_le_bytes());
        p[2..4].copy_from_slice(&v_size.to_le_bytes());
        p[4..6].copy_from_slice(&h_pixels.to_le_bytes());
        p[6..8].copy_from_slice(&v_pixels.to_le_bytes());
        p[8] = flags;
        p[9..12].copy_from_slice(&pack_chromaticity12(chromaticity.0, chromaticity.1));
        p[12..15].copy_from_slice(&pack_chromaticity12(chromaticity.2, chromaticity.3));
        p[15..18].copy_from_slice(&pack_chromaticity12(chromaticity.4, chromaticity.5));
        p[18..21].copy_from_slice(&pack_chromaticity12(chromaticity.6, chromaticity.7));
        p[21..23].copy_from_slice(&max_lum_full.to_le_bytes());
        p[23..25].copy_from_slice(&max_lum_10pct.to_le_bytes());
        p[25..27].copy_from_slice(&min_lum.to_le_bytes());
        p[27] = depth_tech_byte;
        p[28] = gamma_byte;
        p
    }

    #[test]
    fn test_v2_display_params_image_size_tenths() {
        // Revision bit 7 = 0: 0.1 mm units. 5970×3360 → 597×336 mm.
        let p = make_v2_display_params_payload(
            5970,
            3360,
            0,
            0,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x8000,
            0x8000,
            0x8000,
            0x00,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x00, &mut caps, &mut did);
        let size = DisplayIdImageSize::new(5970, 3360, ImageSizeUnit::TenthMillimetre);
        assert_eq!(did.image_size, Some(size));
        // DisplayID's size stays out of the base block's preferred-timing field.
        assert_eq!(caps.preferred_image_size_mm, None);
    }

    #[test]
    fn test_v2_display_params_image_size_whole_mm() {
        // Revision bit 7 = 1: 1 mm units. 597×336 → 597×336 mm.
        let p = make_v2_display_params_payload(
            597,
            336,
            0,
            0,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x8000,
            0x8000,
            0x8000,
            0x00,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        let size = DisplayIdImageSize::new(597, 336, ImageSizeUnit::Millimetre);
        assert_eq!(did.image_size, Some(size));
    }

    #[test]
    fn test_v2_display_params_zero_size_not_stored() {
        let p = make_v2_display_params_payload(
            0,
            0,
            0,
            0,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x8000,
            0x8000,
            0x8000,
            0x00,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        assert_eq!(did.image_size, None);
    }

    #[test]
    fn test_v2_display_params_native_pixels() {
        let p = make_v2_display_params_payload(
            0,
            0,
            3840,
            2160,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x8000,
            0x8000,
            0x8000,
            0x00,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        assert_eq!(caps.native_pixels, Some((3840, 2160)));
    }

    #[test]
    fn test_v2_display_params_chromaticity_round_trip() {
        // sRGB primaries (12-bit raw values, each ≈ original × 4096).
        let r = (2867, 1474); // (0.700, 0.360)
        let g = (1228, 2867); // (0.300, 0.700)
        let b = (614, 245); // (0.150, 0.060)
        let w = (1294, 1347); // (0.316, 0.329)
        let p = make_v2_display_params_payload(
            0,
            0,
            0,
            0,
            0,
            (r.0, r.1, g.0, g.1, b.0, b.1, w.0, w.1),
            0x8000,
            0x8000,
            0x8000,
            0x00,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        let chrom = did.display_params_v2.as_ref().unwrap().chromaticity;
        assert_eq!((chrom.primary1.x_raw, chrom.primary1.y_raw), r);
        assert_eq!((chrom.primary2.x_raw, chrom.primary2.y_raw), g);
        assert_eq!((chrom.primary3.x_raw, chrom.primary3.y_raw), b);
        assert_eq!((chrom.white.x_raw, chrom.white.y_raw), w);
    }

    #[test]
    fn test_v2_display_params_luminance_decoded() {
        let max_full = encode_luminance_f16(1000.0);
        let max_10pct = encode_luminance_f16(1500.0);
        let min = encode_luminance_f16(0.05);
        let p = make_v2_display_params_payload(
            0,
            0,
            0,
            0,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            u16::from_le_bytes(max_full),
            u16::from_le_bytes(max_10pct),
            u16::from_le_bytes(min),
            0x00,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        let params = did.display_params_v2.as_ref().unwrap();
        assert!((params.max_luminance_full.unwrap() - 1000.0).abs() < 1.0);
        assert!((params.max_luminance_10pct.unwrap() - 1500.0).abs() < 1.0);
        assert!((params.min_luminance.unwrap() - 0.05).abs() < 1e-3);
    }

    #[test]
    fn test_v2_display_params_negative_zero_luminance_is_unused() {
        let p = make_v2_display_params_payload(
            0,
            0,
            0,
            0,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x8000, // -0 = unused
            0x8000,
            0x8000,
            0x00,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        let params = did.display_params_v2.as_ref().unwrap();
        assert_eq!(params.max_luminance_full, None);
        assert_eq!(params.max_luminance_10pct, None);
        assert_eq!(params.min_luminance, None);
    }

    #[test]
    fn test_v2_display_params_positive_zero_luminance_is_unused() {
        // +0 (0x0000) is not the spec's sentinel, but 0 cd/m² is degenerate for any
        // luminance field — accept it leniently as "unused" rather than Some(0.0).
        let p = make_v2_display_params_payload(
            0,
            0,
            0,
            0,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x0000,
            0x0000,
            0x0000,
            0x00,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        let params = did.display_params_v2.as_ref().unwrap();
        assert_eq!(params.max_luminance_full, None);
        assert_eq!(params.max_luminance_10pct, None);
        assert_eq!(params.min_luminance, None);
    }

    #[test]
    fn test_v2_display_params_color_depth_decoded() {
        // Bits 2:0 = 3 → 10 bpc.
        let p = make_v2_display_params_payload(
            0,
            0,
            0,
            0,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x8000,
            0x8000,
            0x8000,
            0x03,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        let params = did.display_params_v2.as_ref().unwrap();
        assert_eq!(params.color_bit_depth, Some(10));
        assert_eq!(caps.color_bit_depth, Some(ColorBitDepth::Depth10));
    }

    #[test]
    fn test_v2_display_params_color_depth_5_decodes_to_16() {
        // Bits 2:0 = 5 → 16 bpc (DisplayID 2.x has no 14 bpc encoding).
        let p = make_v2_display_params_payload(
            0,
            0,
            0,
            0,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x8000,
            0x8000,
            0x8000,
            0x05,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        assert_eq!(
            did.display_params_v2.as_ref().unwrap().color_bit_depth,
            Some(16)
        );
        assert_eq!(caps.color_bit_depth, Some(ColorBitDepth::Depth16));
    }

    #[test]
    fn test_v2_display_params_display_technology_decoded() {
        // Bits 6:4 = 2 → AMOLED.
        let p = make_v2_display_params_payload(
            0,
            0,
            0,
            0,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x8000,
            0x8000,
            0x8000,
            0x20,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        assert_eq!(
            did.display_params_v2.as_ref().unwrap().display_technology,
            V2DisplayTechnology::Amoled
        );
    }

    #[test]
    fn test_v2_display_params_gamma_decoded() {
        // gamma_byte = 120 → (120/100) + 1 = 2.20.
        let p = make_v2_display_params_payload(
            0,
            0,
            0,
            0,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x8000,
            0x8000,
            0x8000,
            0x00,
            120,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        let g = did.display_params_v2.as_ref().unwrap().gamma.unwrap();
        assert!((g - 2.20).abs() < 1e-4);
    }

    #[test]
    fn test_v2_display_params_gamma_unspecified() {
        let p = make_v2_display_params_payload(
            0,
            0,
            0,
            0,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x8000,
            0x8000,
            0x8000,
            0x00,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        assert_eq!(did.display_params_v2.as_ref().unwrap().gamma, None);
    }

    #[test]
    fn test_v2_display_params_feature_flags_decoded() {
        // bit 3 = luminance guidance, bit 6 = CIE 1976, bit 7 = external audio,
        // bits 2:0 = 0b101 = LeftRightBottomTop scan.
        let flags = 0b1100_1101;
        let p = make_v2_display_params_payload(
            0,
            0,
            0,
            0,
            flags,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x8000,
            0x8000,
            0x8000,
            0x00,
            0xFF,
        );
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&p, 0x80, &mut caps, &mut did);
        let params = did.display_params_v2.as_ref().unwrap();
        assert!(params.luminance_guidance);
        assert!(params.color_space_cie1976);
        assert!(params.audio_external);
        assert_eq!(params.scan_orientation, ScanOrientation::LeftRightBottomTop);
    }

    #[test]
    fn test_v2_display_params_short_payload_skipped() {
        let short = [0u8; 28];
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_display_params_block(&short, 0x80, &mut caps, &mut did);
        assert!(did.display_params_v2.is_none());
    }

    #[test]
    fn test_v2_display_params_dispatched_only_for_v2_section() {
        let body = make_v2_display_params_payload(
            597,
            336,
            3840,
            2160,
            0,
            (0, 0, 0, 0, 0, 0, 0, 0),
            0x8000,
            0x8000,
            0x8000,
            0x03,
            0xFF,
        );
        let mut block_payload = Vec::new();
        block_payload.push(TAG_V2_DISPLAY_PARAMS); // tag
        block_payload.push(0x80); // revision (image-size precision = 1 mm)
        block_payload.push(body.len() as u8); // length
        block_payload.extend_from_slice(&body);

        // V1 section: should not decode V2 0x21.
        let mut caps_v1 = DisplayCapabilities::default();
        let mut did_v1 = DisplayIdCapabilities::new(0x13, 0);
        scan_all_metadata_blocks(
            &block_payload,
            0x13,
            &mut caps_v1,
            &mut did_v1,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert!(did_v1.display_params_v2.is_none());

        // V2 section: decoder runs.
        let mut caps_v2 = DisplayCapabilities::default();
        let mut did_v2 = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &block_payload,
            DISPLAYID_V2,
            &mut caps_v2,
            &mut did_v2,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert!(did_v2.display_params_v2.is_some());
        let size = DisplayIdImageSize::new(597, 336, ImageSizeUnit::Millimetre);
        assert_eq!(did_v2.image_size, Some(size));
        assert_eq!(caps_v2.native_pixels, Some((3840, 2160)));
    }

    // -----------------------------------------------------------------------
    // V2 Dynamic Video Timing Range Limits Block (tag 0x25)
    // -----------------------------------------------------------------------

    fn make_v2_dynamic_timing_range_payload(
        min_pclk_khz: u32,
        max_pclk_khz: u32,
        min_v_hz: u8,
        max_v_hz: u16,
        vrr: bool,
    ) -> [u8; 9] {
        let mut p = [0u8; 9];
        p[0] = (min_pclk_khz & 0xFF) as u8;
        p[1] = ((min_pclk_khz >> 8) & 0xFF) as u8;
        p[2] = ((min_pclk_khz >> 16) & 0xFF) as u8;
        p[3] = (max_pclk_khz & 0xFF) as u8;
        p[4] = ((max_pclk_khz >> 8) & 0xFF) as u8;
        p[5] = ((max_pclk_khz >> 16) & 0xFF) as u8;
        p[6] = min_v_hz;
        p[7] = (max_v_hz & 0xFF) as u8;
        let mut flags = ((max_v_hz >> 8) & 0x03) as u8;
        if vrr {
            flags |= 0x80;
        }
        p[8] = flags;
        p
    }

    #[test]
    fn test_v2_dynamic_timing_range_basic() {
        // 25 MHz–600 MHz, 24–60 Hz, no VRR. Block revision 0 (8-bit max v rate).
        let p = make_v2_dynamic_timing_range_payload(25_000, 600_000, 24, 60, false);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_dynamic_timing_range_block(&p, 0x00, &mut caps, &mut did);
        let r = did.dynamic_timing_range.unwrap();
        assert_eq!(r.min_pixel_clock_khz, 25_000);
        assert_eq!(r.max_pixel_clock_khz, 600_000);
        assert_eq!(r.min_v_rate_hz, 24);
        assert_eq!(r.max_v_rate_hz, 60);
        assert!(!r.vrr_supported);
        assert_eq!(caps.max_pixel_clock_mhz, Some(600));
        assert_eq!(caps.min_v_rate, Some(24));
        assert_eq!(caps.max_v_rate, Some(60));
    }

    #[test]
    fn test_v2_dynamic_timing_range_vrr_flag() {
        let p = make_v2_dynamic_timing_range_payload(25_000, 600_000, 30, 144, true);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_dynamic_timing_range_block(&p, 0x01, &mut caps, &mut did);
        assert!(did.dynamic_timing_range.unwrap().vrr_supported);
    }

    #[test]
    fn test_v2_dynamic_timing_range_revision_1_uses_9_bit_max_v_rate() {
        // Max v rate = 480 Hz (0x1E0). Low 8 bits = 0xE0 in byte 7; high 2 bits = 0b01 in flags[1:0].
        let p = make_v2_dynamic_timing_range_payload(25_000, 600_000, 24, 480, false);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_dynamic_timing_range_block(&p, 0x01, &mut caps, &mut did);
        assert_eq!(did.dynamic_timing_range.unwrap().max_v_rate_hz, 480);
        assert_eq!(caps.max_v_rate, Some(480));
    }

    #[test]
    fn test_v2_dynamic_timing_range_revision_0_ignores_high_bits() {
        // Encode a 9-bit max_v_rate (480 Hz) but pass revision 0 — the high 2 bits
        // are reserved on revision 0, so only the low 8 bits should decode (0xE0 = 224).
        let p = make_v2_dynamic_timing_range_payload(25_000, 600_000, 24, 480, false);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_dynamic_timing_range_block(&p, 0x00, &mut caps, &mut did);
        assert_eq!(did.dynamic_timing_range.unwrap().max_v_rate_hz, 0xE0);
    }

    #[test]
    fn test_v2_dynamic_timing_range_pixel_clock_khz_precision_preserved() {
        // 148_500 kHz = 148.5 MHz; the kHz precision is preserved on did.dynamic_timing_range
        // even though the unified caps field rounds down to 148 MHz.
        let p = make_v2_dynamic_timing_range_payload(25_000, 148_500, 24, 60, false);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_dynamic_timing_range_block(&p, 0x00, &mut caps, &mut did);
        assert_eq!(
            did.dynamic_timing_range.unwrap().max_pixel_clock_khz,
            148_500
        );
        assert_eq!(caps.max_pixel_clock_mhz, Some(148));
    }

    #[test]
    fn test_v2_dynamic_timing_range_short_payload_skipped() {
        let short = [0u8; 8];
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_dynamic_timing_range_block(&short, 0x01, &mut caps, &mut did);
        assert!(did.dynamic_timing_range.is_none());
    }

    #[test]
    fn test_v2_dynamic_timing_range_dispatched_only_for_v2_section() {
        let body = make_v2_dynamic_timing_range_payload(25_000, 600_000, 24, 60, true);
        let mut block_payload = Vec::new();
        block_payload.push(TAG_V2_DYNAMIC_TIMING_RANGE);
        block_payload.push(0x01); // revision 1
        block_payload.push(body.len() as u8);
        block_payload.extend_from_slice(&body);

        // V1 section: must not decode.
        let mut caps_v1 = DisplayCapabilities::default();
        let mut did_v1 = DisplayIdCapabilities::new(0x13, 0);
        scan_all_metadata_blocks(
            &block_payload,
            0x13,
            &mut caps_v1,
            &mut did_v1,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert!(did_v1.dynamic_timing_range.is_none());

        // V2 section: decoder runs.
        let mut caps_v2 = DisplayCapabilities::default();
        let mut did_v2 = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &block_payload,
            DISPLAYID_V2,
            &mut caps_v2,
            &mut did_v2,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        let r = did_v2.dynamic_timing_range.unwrap();
        assert_eq!(r.max_pixel_clock_khz, 600_000);
        assert!(r.vrr_supported);
    }

    // -----------------------------------------------------------------------
    // V2 Display Interface Features Block (tag 0x26)
    // -----------------------------------------------------------------------

    fn make_v2_interface_features_payload(
        rgb: u8,
        ycbcr444: u8,
        ycbcr422: u8,
        ycbcr420: u8,
        min_420_rate: u8,
        audio: u8,
        cs_eotf_1: u8,
    ) -> [u8; 9] {
        [
            rgb,
            ycbcr444,
            ycbcr422,
            ycbcr420,
            min_420_rate,
            audio,
            cs_eotf_1,
            0x00,
            0x00,
        ]
    }

    #[test]
    fn test_v2_interface_features_basic() {
        // RGB: 8/10/12 bpc; YCbCr 4:4:4: 8/10; YCbCr 4:2:2: 8/10/12;
        // YCbCr 4:2:0: 10; min 4:2:0 rate at 74.25 MP/s; 48 kHz audio; default colorspace.
        let p = make_v2_interface_features_payload(
            0b0000_1110,
            0b0000_0110,
            0b0000_0111,
            0b0000_0010,
            1,
            0b1000_0000,
            0b0000_0001,
        );
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_interface_features_block(&p, &mut did);
        let f = did.interface_features.unwrap();
        assert_eq!(f.color_depth_rgb.bits(), 0b0000_1110);
        assert_eq!(f.color_depth_ycbcr444.bits(), 0b0000_0110);
        assert_eq!(f.color_depth_ycbcr422.bits(), 0b0000_0111);
        assert_eq!(f.color_depth_ycbcr420.bits(), 0b0000_0010);
        assert_eq!(f.min_ycbcr420_pixel_rate, 1);
        assert_eq!(f.audio_flags, 0b1000_0000);
        assert_eq!(f.color_space_eotf_combos, 0b0000_0001);
    }

    #[test]
    fn test_v2_interface_features_all_zero_payload() {
        let p = make_v2_interface_features_payload(0, 0, 0, 0, 0, 0, 0);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_interface_features_block(&p, &mut did);
        // Even fully-zero (no formats supported) is a valid decoded record.
        let f = did.interface_features.unwrap();
        assert_eq!(f.color_depth_rgb.bits(), 0);
        assert_eq!(f.audio_flags, 0);
    }

    #[test]
    fn test_v2_interface_features_short_payload_skipped() {
        let short = [0x3E, 0x06, 0x07, 0x02, 0x00, 0x80, 0x01, 0x00]; // 8 bytes
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_interface_features_block(&short, &mut did);
        assert!(did.interface_features.is_none());
    }

    #[test]
    fn test_v2_interface_features_no_custom_combos_when_count_zero() {
        // count = 0 → custom_color_space_eotf_count = 0.
        let p = make_v2_interface_features_payload(0x3E, 0x06, 0x07, 0x02, 0, 0x80, 0x01);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_interface_features_block(&p, &mut did);
        let f = did.interface_features.unwrap();
        assert_eq!(f.color_space_eotf_combos, 0x01);
        assert_eq!(f.custom_color_space_eotf_count, 0);
    }

    #[test]
    fn test_v2_interface_features_custom_combos_decoded() {
        // count = 2; combo[0] = cs=6 (BT.2020), eotf=8 (ST 2084); combo[1] = cs=1 (sRGB), eotf=1.
        let mut p = make_v2_interface_features_payload(0, 0, 0, 0, 0, 0, 0).to_vec();
        p[7] = 0x00; // reserved
        p[8] = 2; // count
        p.push((6 << 4) | 8); // BT.2020 / ST 2084
        p.push((1 << 4) | 1); // sRGB / sRGB
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_interface_features_block(&p, &mut did);
        let f = did.interface_features.unwrap();
        assert_eq!(f.custom_color_space_eotf_count, 2);
        assert_eq!(f.custom_color_space_eotf_combos[0].color_space, 6);
        assert_eq!(f.custom_color_space_eotf_combos[0].eotf, 8);
        assert_eq!(f.custom_color_space_eotf_combos[1].color_space, 1);
        assert_eq!(f.custom_color_space_eotf_combos[1].eotf, 1);
    }

    #[test]
    fn test_v2_interface_features_custom_combos_capped_at_7() {
        // count = 9 (> 7 max); only 7 decoded.
        let mut p = make_v2_interface_features_payload(0, 0, 0, 0, 0, 0, 0).to_vec();
        p[8] = 9;
        p.extend(core::iter::repeat_n(0x11u8, 9));
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_interface_features_block(&p, &mut did);
        let f = did.interface_features.unwrap();
        assert_eq!(f.custom_color_space_eotf_count, 7);
    }

    #[test]
    fn test_v2_interface_features_dispatched_only_for_v2_section() {
        let body = make_v2_interface_features_payload(0x3E, 0x06, 0x07, 0x02, 0, 0x80, 0x01);
        let mut block_payload = Vec::new();
        block_payload.push(TAG_V2_INTERFACE_FEATURES);
        block_payload.push(0x00); // revision
        block_payload.push(body.len() as u8);
        block_payload.extend_from_slice(&body);

        // V1 section: must not decode.
        let mut caps_v1 = DisplayCapabilities::default();
        let mut did_v1 = DisplayIdCapabilities::new(0x13, 0);
        scan_all_metadata_blocks(
            &block_payload,
            0x13,
            &mut caps_v1,
            &mut did_v1,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert!(did_v1.interface_features.is_none());

        // V2 section: decoder runs.
        let mut caps_v2 = DisplayCapabilities::default();
        let mut did_v2 = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &block_payload,
            DISPLAYID_V2,
            &mut caps_v2,
            &mut did_v2,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        let f = did_v2.interface_features.unwrap();
        assert_eq!(f.color_depth_rgb.bits(), 0x3E);
        assert_eq!(f.audio_flags, 0x80);
    }

    #[test]
    fn test_v2_interface_features_only_first_block_decoded() {
        // Two 0x26 blocks back-to-back: the second must be ignored.
        let first = make_v2_interface_features_payload(0x3E, 0x06, 0x07, 0x02, 0, 0x80, 0x01);
        let second = make_v2_interface_features_payload(0xFF, 0xFF, 0xFF, 0xFF, 9, 0xE0, 0xFF);
        let mut payload = Vec::new();
        for body in [first, second] {
            payload.push(TAG_V2_INTERFACE_FEATURES);
            payload.push(0x00);
            payload.push(body.len() as u8);
            payload.extend_from_slice(&body);
        }
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &payload,
            DISPLAYID_V2,
            &mut caps,
            &mut did,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        let f = did.interface_features.unwrap();
        assert_eq!(f.color_depth_rgb.bits(), 0x3E);
        assert_eq!(f.audio_flags, 0x80);
    }

    // -----------------------------------------------------------------------
    // Display Parameters Block (tag 0x01)
    // -----------------------------------------------------------------------

    #[test]
    fn test_display_params_image_size() {
        // A 27" 16:9 panel, 597 × 336 mm, stored in tenths of mm as edid-decode and
        // libdisplay-info read them.
        let payload = make_display_params_payload(5970, 3360, 0, 0, 0, 0x00);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x12, 0);
        decode_display_params_block(&payload, &mut caps, &mut did);
        let size = DisplayIdImageSize::new(5970, 3360, ImageSizeUnit::TenthMillimetre);
        assert_eq!(did.image_size, Some(size));
        assert_eq!(size.tenths_mm(), (5970, 3360));
        assert_eq!(caps.preferred_image_size_mm, None);
    }

    #[test]
    fn test_display_params_zero_size_not_stored() {
        let payload = make_display_params_payload(0, 0, 0, 0, 0, 0x00);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x12, 0);
        decode_display_params_block(&payload, &mut caps, &mut did);
        assert_eq!(did.image_size, None);
    }

    #[test]
    fn test_display_params_partial_zero_size_not_stored() {
        let payload = make_display_params_payload(5970, 0, 0, 0, 0, 0x00);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x12, 0);
        decode_display_params_block(&payload, &mut caps, &mut did);
        assert_eq!(did.image_size, None);
    }

    #[test]
    fn test_display_params_native_pixels() {
        let payload = make_display_params_payload(5970, 3360, 1920, 1080, 0, 0x00);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x12, 0);
        decode_display_params_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.native_pixels, Some((1920, 1080)));
    }

    #[test]
    fn test_display_params_zero_native_pixels_not_stored() {
        let payload = make_display_params_payload(5970, 3360, 0, 0, 0, 0x00);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x12, 0);
        decode_display_params_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.native_pixels, None);
    }

    #[test]
    fn test_display_params_aspect_ratio() {
        // 16:9 → (16/9 − 1) × 100 ≈ 78
        let payload = make_display_params_payload(5970, 3360, 0, 0, 78, 0x00);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x12, 0);
        decode_display_params_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.panel_aspect_ratio_100, Some(78));
    }

    #[test]
    fn test_display_params_color_bit_depth_8bpc() {
        // Low nibble = bpc − 1: 8bpc → 7 → byte = 0x07
        let payload = make_display_params_payload(5970, 3360, 0, 0, 0, 0x07);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x12, 0);
        decode_display_params_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.color_bit_depth, Some(ColorBitDepth::Depth8));
    }

    #[test]
    fn test_display_params_color_bit_depth_10bpc() {
        // 10bpc → 9 → byte = 0x09
        let payload = make_display_params_payload(5970, 3360, 0, 0, 0, 0x09);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x12, 0);
        decode_display_params_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.color_bit_depth, Some(ColorBitDepth::Depth10));
    }

    #[test]
    fn test_display_params_undefined_bit_depth_not_stored() {
        // Low nibble = 0 → bpc = 1 → not a valid even bit depth → None
        let payload = make_display_params_payload(5970, 3360, 0, 0, 0, 0x00);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x12, 0);
        decode_display_params_block(&payload, &mut caps, &mut did);
        assert_eq!(caps.color_bit_depth, None);
    }

    #[test]
    fn test_display_params_too_short_does_not_panic() {
        // Only 3 bytes — too short for image size.
        let payload = [0x55u8, 0x01, 0x00];
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x12, 0);
        decode_display_params_block(&payload, &mut caps, &mut did);
        assert_eq!(did.image_size, None);
    }

    // -----------------------------------------------------------------------
    // Color Characteristics Block (tag 0x02)
    // -----------------------------------------------------------------------

    #[test]
    fn test_color_characteristics_primaries_decoded() {
        // sRGB-like primaries: R(0.64, 0.33), G(0.30, 0.60), B(0.15, 0.06), D65(0.3127, 0.3290)
        // Scaled × 1024: R(655, 338), G(307, 614), B(154, 61), W(320, 337)
        let payload =
            make_color_characteristics_payload((655, 338), (307, 614), (154, 61), (320, 337));
        let mut caps = DisplayCapabilities::default();
        decode_color_characteristics_block(&payload, &mut caps);
        assert_eq!(caps.chromaticity.red.x_raw, 655);
        assert_eq!(caps.chromaticity.red.y_raw, 338);
        assert_eq!(caps.chromaticity.green.x_raw, 307);
        assert_eq!(caps.chromaticity.green.y_raw, 614);
        assert_eq!(caps.chromaticity.blue.x_raw, 154);
        assert_eq!(caps.chromaticity.blue.y_raw, 61);
        assert_eq!(caps.chromaticity.white.x_raw, 320);
        assert_eq!(caps.chromaticity.white.y_raw, 337);
    }

    #[test]
    fn test_color_characteristics_upper_bits_masked() {
        // Bits above 10 (mask 0x03FF) should be stripped — store 0x04FF, expect 0x00FF.
        let payload = make_color_characteristics_payload(
            (0x04FF, 0x04FF),
            (0x04FF, 0x04FF),
            (0x04FF, 0x04FF),
            (0x04FF, 0x04FF),
        );
        let mut caps = DisplayCapabilities::default();
        decode_color_characteristics_block(&payload, &mut caps);
        assert_eq!(caps.chromaticity.red.x_raw, 0x00FF);
    }

    #[test]
    fn test_color_characteristics_short_payload_ignored() {
        // A 15-byte payload — one byte short of the minimum 16. Must not modify chromaticity.
        let payload = [0u8; 15];
        let mut caps = DisplayCapabilities::default();
        decode_color_characteristics_block(&payload, &mut caps);
        assert_eq!(caps.chromaticity, Chromaticity::default());
    }

    // -----------------------------------------------------------------------
    // Video Timing Range Limits Block (tag 0x09)
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)] // mirrors the wire-format field list
    fn make_video_timing_range_payload(
        min_pixel_clock_10khz: u32,
        max_pixel_clock_10khz: u32,
        min_h_khz: u8,
        max_h_khz: u8,
        min_h_blank: u16,
        min_v_rate: u8,
        max_v_rate: u8,
        min_v_blank: u16,
        flags: u8,
    ) -> [u8; 15] {
        let mut p = [0u8; 15];
        p[0] = (min_pixel_clock_10khz & 0xFF) as u8;
        p[1] = ((min_pixel_clock_10khz >> 8) & 0xFF) as u8;
        p[2] = ((min_pixel_clock_10khz >> 16) & 0xFF) as u8;
        p[3] = (max_pixel_clock_10khz & 0xFF) as u8;
        p[4] = ((max_pixel_clock_10khz >> 8) & 0xFF) as u8;
        p[5] = ((max_pixel_clock_10khz >> 16) & 0xFF) as u8;
        p[6] = min_h_khz;
        p[7] = max_h_khz;
        p[8..10].copy_from_slice(&min_h_blank.to_le_bytes());
        p[10] = min_v_rate;
        p[11] = max_v_rate;
        p[12..14].copy_from_slice(&min_v_blank.to_le_bytes());
        p[14] = flags;
        p
    }

    #[test]
    fn test_video_timing_range_all_fields_decoded() {
        // max_pixel_clock = 33750 × 10 kHz = 337.5 MHz → stored as 337 MHz
        // h range: 30–135 kHz, v range: 48–240 Hz
        let payload = make_video_timing_range_payload(1000, 33750, 30, 135, 160, 48, 240, 45, 0);
        let mut caps = DisplayCapabilities::default();
        decode_video_timing_range_block(&payload, &mut caps);
        assert_eq!(caps.max_pixel_clock_mhz, Some(337));
        assert_eq!(caps.min_h_rate_khz, Some(30));
        assert_eq!(caps.max_h_rate_khz, Some(135));
        assert_eq!(caps.min_v_rate, Some(48));
        assert_eq!(caps.max_v_rate, Some(240));
    }

    #[test]
    fn test_video_timing_range_typical_monitor() {
        // 1920×1080@60 Hz monitor: max clock ~148.5 MHz = 14850 × 10 kHz → 148 MHz
        // h: 30–83 kHz, v: 56–75 Hz
        let payload = make_video_timing_range_payload(3000, 14850, 30, 83, 160, 56, 75, 45, 0x00);
        let mut caps = DisplayCapabilities::default();
        decode_video_timing_range_block(&payload, &mut caps);
        assert_eq!(caps.max_pixel_clock_mhz, Some(148));
        assert_eq!(caps.min_h_rate_khz, Some(30));
        assert_eq!(caps.max_h_rate_khz, Some(83));
        assert_eq!(caps.min_v_rate, Some(56));
        assert_eq!(caps.max_v_rate, Some(75));
    }

    #[test]
    fn test_video_timing_range_short_payload_partial_decode() {
        // Only 8 bytes — enough for max_pixel_clock and h rates, not v rates.
        let mut payload = [0u8; 8];
        payload[3] = 0x52; // max_pixel_clock low byte
        payload[4] = 0x39; // max_pixel_clock mid byte (0x3952 = 14674 × 10 kHz → 146 MHz)
        payload[6] = 25; // min_h_rate_khz
        payload[7] = 90; // max_h_rate_khz
        let mut caps = DisplayCapabilities::default();
        decode_video_timing_range_block(&payload, &mut caps);
        assert_eq!(caps.max_pixel_clock_mhz, Some(146));
        assert_eq!(caps.min_h_rate_khz, Some(25));
        assert_eq!(caps.max_h_rate_khz, Some(90));
        assert_eq!(caps.min_v_rate, None);
        assert_eq!(caps.max_v_rate, None);
    }

    #[test]
    fn test_video_timing_range_too_short_does_not_panic() {
        // Only 5 bytes — too short even for max_pixel_clock.
        let payload = [0u8; 5];
        let mut caps = DisplayCapabilities::default();
        decode_video_timing_range_block(&payload, &mut caps);
        assert_eq!(caps.max_pixel_clock_mhz, None);
        assert_eq!(caps.min_h_rate_khz, None);
        assert_eq!(caps.min_v_rate, None);
    }

    // -----------------------------------------------------------------------
    // Product Serial Number Block (tag 0x0A)
    // -----------------------------------------------------------------------

    #[test]
    fn test_serial_number_short_string() {
        let payload = b"SN12345\x0a     ";
        let mut caps = DisplayCapabilities::default();
        decode_serial_number_block(payload, &mut caps);
        assert_eq!(caps.serial_number_string.as_deref(), Some("SN12345"));
    }

    #[test]
    fn test_serial_number_full_13_bytes_no_terminator_gets_one_added() {
        // 13 bytes of non-0x0A content — a terminator must be inserted at position 12.
        let payload = b"ABCDEFGHIJKLM";
        let mut caps = DisplayCapabilities::default();
        decode_serial_number_block(payload, &mut caps);
        let s = caps.serial_number_string.unwrap();
        assert_eq!(s.0[12], 0x0A);
    }

    #[test]
    fn test_serial_number_truncated_at_13_bytes() {
        // Payload longer than 13 bytes; only the first 13 bytes are copied into the buffer,
        // but the 13th is overwritten by the 0x0A terminator, so 12 visible characters remain.
        let payload = b"ABCDEFGHIJKLMNOPQRST";
        let mut caps = DisplayCapabilities::default();
        decode_serial_number_block(payload, &mut caps);
        assert_eq!(caps.serial_number_string.as_deref(), Some("ABCDEFGHIJKL"));
    }

    #[test]
    fn test_serial_number_empty_payload_not_stored() {
        let payload: &[u8] = &[];
        let mut caps = DisplayCapabilities::default();
        decode_serial_number_block(payload, &mut caps);
        assert_eq!(caps.serial_number_string, None);
    }

    // -----------------------------------------------------------------------
    // General Purpose ASCII String Block (tag 0x0B)
    // -----------------------------------------------------------------------

    #[test]
    fn test_ascii_string_stored_in_first_slot() {
        let payload = b"Hello\x0a       ";
        let mut caps = DisplayCapabilities::default();
        decode_ascii_string_block(payload, &mut caps);
        assert_eq!(caps.unspecified_text[0].as_deref(), Some("Hello"));
        assert!(caps.unspecified_text[1].is_none());
    }

    #[test]
    fn test_ascii_string_multiple_blocks_fill_slots() {
        let mut caps = DisplayCapabilities::default();
        decode_ascii_string_block(b"First\x0a       ", &mut caps);
        decode_ascii_string_block(b"Second\x0a      ", &mut caps);
        assert_eq!(caps.unspecified_text[0].as_deref(), Some("First"));
        assert_eq!(caps.unspecified_text[1].as_deref(), Some("Second"));
        assert!(caps.unspecified_text[2].is_none());
    }

    #[test]
    fn test_ascii_string_overflow_beyond_four_slots_dropped() {
        let mut caps = DisplayCapabilities::default();
        for i in 0u8..5 {
            decode_ascii_string_block(&[b'A' + i, 0x0A], &mut caps);
        }
        // Only 4 slots available; fifth block is silently dropped.
        assert!(caps.unspecified_text.iter().all(|s| s.is_some()));
        assert_eq!(caps.unspecified_text[3].as_deref(), Some("D"));
    }

    #[test]
    fn test_ascii_string_empty_payload_not_stored() {
        let mut caps = DisplayCapabilities::default();
        decode_ascii_string_block(&[], &mut caps);
        assert!(caps.unspecified_text[0].is_none());
    }

    // -----------------------------------------------------------------------
    // Display Device Data Block (tag 0x0C)
    // -----------------------------------------------------------------------

    use crate::model::panel::{
        BacklightType, DisplayTechnology, OperatingMode, PhysicalOrientation, RotationCapability,
        ScanDirection, SubpixelLayout, ZeroPixelLocation,
    };

    /// Builds a full 13-byte Display Device Data payload.
    #[allow(clippy::too_many_arguments)]
    fn make_display_device_data_payload(
        tech: u8,      // bits 7:4 of byte 0
        subtype: u8,   // bits 3:0 of byte 0
        b1: u8,        // byte 1 raw
        h_native: u16, // bytes 2–3
        v_native: u16, // bytes 4–5
        ar_100: u8,    // byte 6
        orient: u8,    // byte 7
        subpixel: u8,  // byte 8
        h_pitch: u8,   // byte 9
        v_pitch: u8,   // byte 10
        bpc_raw: u8,   // byte 11 bits 3:0
        response: u8,  // byte 12
    ) -> [u8; 13] {
        let mut p = [0u8; 13];
        p[0] = (tech << 4) | (subtype & 0x0F);
        p[1] = b1;
        p[2..4].copy_from_slice(&h_native.to_le_bytes());
        p[4..6].copy_from_slice(&v_native.to_le_bytes());
        p[6] = ar_100;
        p[7] = orient;
        p[8] = subpixel;
        p[9] = h_pitch;
        p[10] = v_pitch;
        p[11] = bpc_raw & 0x0F;
        p[12] = response;
        p
    }

    #[test]
    fn test_display_device_data_technology_decoded() {
        // tech=6 → OLED; subtype=2
        let p = make_display_device_data_payload(6, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.display_technology, Some(DisplayTechnology::Oled));
        assert_eq!(caps.display_subtype, Some(2));
    }

    #[test]
    fn test_display_device_data_unknown_technology() {
        // tech=0xF → Unknown(15)
        let p = make_display_device_data_payload(0xF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(
            caps.display_technology,
            Some(DisplayTechnology::Unknown(15))
        );
    }

    #[test]
    fn test_display_device_data_operating_mode_and_backlight() {
        // operating mode=1 (NonContinuous), backlight=2 (Dc), DE used=1 (+ve)
        // byte1: mode bits 3:0 = 0x01, backlight bits 5:4 = 0b10 → 0x21, DE bit6=1, DE pol bit7=1
        // 0b1110_0001 = 0xE1
        let p = make_display_device_data_payload(0, 0, 0xE1, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.operating_mode, Some(OperatingMode::NonContinuous));
        assert_eq!(caps.backlight_type, Some(BacklightType::Dc));
        assert_eq!(caps.data_enable_used, Some(true));
        assert_eq!(caps.data_enable_positive, Some(true));
    }

    #[test]
    fn test_display_device_data_no_de_signal() {
        // DE bit = 0, polarity irrelevant but reads as false
        let p = make_display_device_data_payload(0, 0, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.data_enable_used, Some(false));
    }

    #[test]
    fn test_display_device_data_native_pixels_decoded() {
        let p = make_display_device_data_payload(0, 0, 0, 1920, 1080, 0, 0, 0, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.native_pixels, Some((1920, 1080)));
    }

    #[test]
    fn test_display_device_data_zero_native_pixels_not_stored() {
        let p = make_display_device_data_payload(0, 0, 0, 0, 1080, 0, 0, 0, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.native_pixels, None);
    }

    #[test]
    fn test_display_device_data_aspect_ratio_stored() {
        // 16:9 ≈ AR 1.78 → (AR-1)×100 ≈ 78; raw byte = 78
        let p = make_display_device_data_payload(0, 0, 0, 0, 0, 78, 0, 0, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.panel_aspect_ratio_100, Some(78));
    }

    #[test]
    fn test_display_device_data_orientation_flags() {
        // orient byte: bits 1:0=1 (Portrait), bits 3:2=1 (CW90), bits 5:4=2 (LowerLeft), bits 7:6=1 (Normal)
        // = 0b01_10_01_01 = 0x65
        let p = make_display_device_data_payload(0, 0, 0, 0, 0, 0, 0x65, 0, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(
            caps.physical_orientation,
            Some(PhysicalOrientation::Portrait)
        );
        assert_eq!(caps.rotation_capability, Some(RotationCapability::Cw90));
        assert_eq!(caps.zero_pixel_location, Some(ZeroPixelLocation::LowerLeft));
        assert_eq!(caps.scan_direction, Some(ScanDirection::Normal));
    }

    #[test]
    fn test_display_device_data_subpixel_layout_rgb_vertical() {
        let p = make_display_device_data_payload(0, 0, 0, 0, 0, 0, 0, 0x01, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.subpixel_layout, Some(SubpixelLayout::RgbVertical));
    }

    #[test]
    fn test_display_device_data_subpixel_layout_unknown() {
        let p = make_display_device_data_payload(0, 0, 0, 0, 0, 0, 0, 0xAB, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.subpixel_layout, Some(SubpixelLayout::Unknown(0xAB)));
    }

    #[test]
    fn test_display_device_data_pixel_pitch_decoded() {
        // h_pitch=28 (0.28 mm), v_pitch=29 (0.29 mm)
        let p = make_display_device_data_payload(0, 0, 0, 0, 0, 0, 0, 0, 28, 29, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.pixel_pitch_hundredths_mm, Some((28, 29)));
    }

    #[test]
    fn test_display_device_data_zero_pixel_pitch_not_stored() {
        let p = make_display_device_data_payload(0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.pixel_pitch_hundredths_mm, None);
    }

    #[test]
    fn test_display_device_data_8bpc_decoded() {
        // bpc_raw = 7 → bpc = 8 → ColorBitDepth::Depth8
        let p = make_display_device_data_payload(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.color_bit_depth, Some(ColorBitDepth::Depth8));
    }

    #[test]
    fn test_display_device_data_10bpc_decoded() {
        // bpc_raw = 9 → bpc = 10 → ColorBitDepth::Depth10
        let p = make_display_device_data_payload(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.color_bit_depth, Some(ColorBitDepth::Depth10));
    }

    #[test]
    fn test_display_device_data_unknown_bpc_clears_field() {
        // bpc_raw = 0 → bpc = 1 → no EDID mapping → None
        let p = make_display_device_data_payload(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        caps.color_bit_depth = Some(ColorBitDepth::Depth8); // pre-populated
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.color_bit_depth, None);
    }

    #[test]
    fn test_display_device_data_response_time_decoded() {
        // response_time = 5 ms
        let p = make_display_device_data_payload(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.pixel_response_time_ms, Some(5));
    }

    #[test]
    fn test_display_device_data_zero_response_time_not_stored() {
        let p = make_display_device_data_payload(0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&p, &mut caps);
        assert_eq!(caps.pixel_response_time_ms, None);
    }

    #[test]
    fn test_display_device_data_short_payload_decodes_available_bytes() {
        // 2-byte payload: only technology and operating mode fields should be set.
        let payload = [0x60u8, 0x01]; // tech=6 (OLED), mode=1 (NonContinuous)
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&payload, &mut caps);
        assert_eq!(caps.display_technology, Some(DisplayTechnology::Oled));
        assert_eq!(caps.operating_mode, Some(OperatingMode::NonContinuous));
        assert_eq!(caps.native_pixels, None);
        assert_eq!(caps.color_bit_depth, None);
    }

    #[test]
    fn test_display_device_data_empty_payload_does_not_panic() {
        let mut caps = DisplayCapabilities::default();
        decode_display_device_data_block(&[], &mut caps);
        assert_eq!(caps.display_technology, None);
        assert_eq!(caps.color_bit_depth, None);
    }

    // -----------------------------------------------------------------------
    // Interface Power Sequencing Block (tag 0x0D)
    // -----------------------------------------------------------------------

    fn make_power_sequencing_payload(t1: u8, t2: u8, t3: u8, t4: u8, t5: u8, t6: u8) -> [u8; 8] {
        [t1, t2, t3, t4, t5, t6, 0x00, 0x00]
    }

    #[test]
    fn test_power_sequencing_all_fields_decoded() {
        let payload = make_power_sequencing_payload(10, 5, 3, 2, 50, 20);
        let mut caps = DisplayCapabilities::default();
        decode_power_sequencing_block(&payload, &mut caps);
        let ps = caps
            .power_sequencing
            .expect("power_sequencing should be Some");
        assert_eq!(ps.t1_power_to_signal, 10);
        assert_eq!(ps.t2_signal_to_backlight, 5);
        assert_eq!(ps.t3_backlight_to_signal_off, 3);
        assert_eq!(ps.t4_signal_to_power_off, 2);
        assert_eq!(ps.t5_power_off_min, 50);
        assert_eq!(ps.t6_backlight_off_min, 20);
    }

    #[test]
    fn test_power_sequencing_zero_delays_stored() {
        // Zero is a valid value (0 ms minimum delay).
        let payload = make_power_sequencing_payload(0, 0, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_power_sequencing_block(&payload, &mut caps);
        assert!(caps.power_sequencing.is_some());
        let ps = caps.power_sequencing.unwrap();
        assert_eq!(ps.t1_power_to_signal, 0);
        assert_eq!(ps.t5_power_off_min, 0);
    }

    #[test]
    fn test_power_sequencing_reserved_bytes_ignored() {
        // Reserved bytes 6–7 must not affect the decoded struct.
        let mut payload = make_power_sequencing_payload(1, 2, 3, 4, 5, 6);
        payload[6] = 0xFF;
        payload[7] = 0xFF;
        let mut caps = DisplayCapabilities::default();
        decode_power_sequencing_block(&payload, &mut caps);
        let ps = caps.power_sequencing.unwrap();
        assert_eq!(ps.t6_backlight_off_min, 6);
    }

    #[test]
    fn test_power_sequencing_exact_6_bytes_accepted() {
        // A 6-byte payload (without reserved bytes) should still decode successfully.
        let payload = [10u8, 5, 3, 2, 50, 20];
        let mut caps = DisplayCapabilities::default();
        decode_power_sequencing_block(&payload, &mut caps);
        assert!(caps.power_sequencing.is_some());
    }

    #[test]
    fn test_power_sequencing_short_payload_skipped() {
        // Payloads shorter than 6 bytes must be silently ignored.
        let payload = [10u8, 5, 3, 2, 50]; // only 5 bytes
        let mut caps = DisplayCapabilities::default();
        decode_power_sequencing_block(&payload, &mut caps);
        assert_eq!(caps.power_sequencing, None);
    }

    #[test]
    fn test_power_sequencing_empty_payload_skipped() {
        let mut caps = DisplayCapabilities::default();
        decode_power_sequencing_block(&[], &mut caps);
        assert_eq!(caps.power_sequencing, None);
    }

    // -----------------------------------------------------------------------
    // Transfer Characteristics Block (tag 0x0E)
    // -----------------------------------------------------------------------

    #[test]
    fn test_transfer_characteristics_8bit_luminance() {
        // byte 0 = 0x00 (8-bit, single-channel); bytes 1–3 = black, mid, white
        let payload = [0x00u8, 0x00, 0x80, 0xFF];
        let mut caps = DisplayCapabilities::default();
        decode_transfer_characteristics_block(&payload, &mut caps);
        let tc = caps.transfer_characteristic.expect("should be Some");
        assert_eq!(tc.encoding, TransferPointEncoding::Bits8);
        let TransferCurve::Luminance(pts) = tc.curve else {
            panic!("expected Luminance")
        };
        assert_eq!(pts.len(), 3);
        assert!((pts[0] - 0.0).abs() < 0.001);
        assert!((pts[1] - 0x80 as f32 / 255.0).abs() < 0.001);
        assert!((pts[2] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_transfer_characteristics_10bit_luminance() {
        // byte 0 = 0x40 (10-bit, single-channel)
        // 5-byte group encodes 4 points: 1023, 512, 255, 0
        // p0 = 1023 (0x3FF): byte0=0xFF, byte1[7:6]=0b11
        // p1 = 512  (0x200): byte1[5:0]=0b00_1000=0x08, byte2[7:4]=0b0000
        // p2 = 255  (0x0FF): byte2[3:0]=0b0000, byte3[7:2]=0b11_1111=0x3F... let's just pick easy values
        // Use p0=1023, p1=0, p2=0, p3=0:
        // 5 bytes: [0xFF, 0xC0, 0x00, 0x00, 0x00]
        let payload = [0x40u8, 0xFF, 0xC0, 0x00, 0x00, 0x00];
        let mut caps = DisplayCapabilities::default();
        decode_transfer_characteristics_block(&payload, &mut caps);
        let tc = caps.transfer_characteristic.expect("should be Some");
        assert_eq!(tc.encoding, TransferPointEncoding::Bits10);
        let TransferCurve::Luminance(pts) = tc.curve else {
            panic!("expected Luminance")
        };
        assert_eq!(pts.len(), 4);
        assert!((pts[0] - 1.0).abs() < 0.001);
        assert!((pts[1] - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_transfer_characteristics_12bit_luminance() {
        // byte 0 = 0x80 (12-bit, single-channel)
        // 3 bytes encode 2 points: p0=0xFFF (4095), p1=0x000 (0)
        // byte0=0xFF, byte1=0xF0, byte2=0x00
        let payload = [0x80u8, 0xFF, 0xF0, 0x00];
        let mut caps = DisplayCapabilities::default();
        decode_transfer_characteristics_block(&payload, &mut caps);
        let tc = caps.transfer_characteristic.expect("should be Some");
        assert_eq!(tc.encoding, TransferPointEncoding::Bits12);
        let TransferCurve::Luminance(pts) = tc.curve else {
            panic!("expected Luminance")
        };
        assert_eq!(pts.len(), 2);
        assert!((pts[0] - 1.0).abs() < 0.001);
        assert!((pts[1] - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_transfer_characteristics_8bit_rgb() {
        // byte 0 = 0x20 (8-bit, multi-channel)
        // 6 sample bytes → 2 per channel: R=[0xFF,0x80], G=[0x40,0x20], B=[0x10,0x08]
        let payload = [0x20u8, 0xFF, 0x80, 0x40, 0x20, 0x10, 0x08];
        let mut caps = DisplayCapabilities::default();
        decode_transfer_characteristics_block(&payload, &mut caps);
        let tc = caps.transfer_characteristic.expect("should be Some");
        assert_eq!(tc.encoding, TransferPointEncoding::Bits8);
        let TransferCurve::Rgb { red, green, blue } = tc.curve else {
            panic!("expected Rgb")
        };
        assert_eq!(red.len(), 2);
        assert_eq!(green.len(), 2);
        assert_eq!(blue.len(), 2);
        assert!((red[0] - 1.0).abs() < 0.001);
        assert!((red[1] - 0x80 as f32 / 255.0).abs() < 0.001);
        assert!((green[0] - 0x40 as f32 / 255.0).abs() < 0.001);
        assert!((blue[1] - 0x08 as f32 / 255.0).abs() < 0.001);
    }

    #[test]
    fn test_transfer_characteristics_reserved_encoding_skipped() {
        // bits 7:6 = 0b11 → reserved → no field written
        let payload = [0xC0u8, 0x00, 0x80, 0xFF];
        let mut caps = DisplayCapabilities::default();
        decode_transfer_characteristics_block(&payload, &mut caps);
        assert_eq!(caps.transfer_characteristic, None);
    }

    #[test]
    fn test_transfer_characteristics_too_short_skipped() {
        let payload = [0x00u8]; // only 1 byte — need at least 2
        let mut caps = DisplayCapabilities::default();
        decode_transfer_characteristics_block(&payload, &mut caps);
        assert_eq!(caps.transfer_characteristic, None);
    }

    #[test]
    fn test_transfer_characteristics_empty_skipped() {
        let mut caps = DisplayCapabilities::default();
        decode_transfer_characteristics_block(&[], &mut caps);
        assert_eq!(caps.transfer_characteristic, None);
    }

    #[test]
    fn test_transfer_characteristics_rgb_non_divisible_by_3_skipped() {
        // Multi-channel flag set, but 7 sample bytes can't be split evenly → skip
        let payload = [0x20u8, 0xFF, 0x80, 0x40, 0x20, 0x10, 0x08, 0x04]; // 7 sample bytes
        let mut caps = DisplayCapabilities::default();
        decode_transfer_characteristics_block(&payload, &mut caps);
        assert_eq!(caps.transfer_characteristic, None);
    }

    // -----------------------------------------------------------------------
    // Display Interface Data Block (tag 0x0F)
    // -----------------------------------------------------------------------

    fn make_display_interface_payload(
        interface_type: u8, // bits 3:0
        spread_spectrum: bool,
        num_lanes: u8, // bits 3:0
        min_clock_10khz: u16,
        max_clock_10khz: u16,
        content_protection: u8, // bits 1:0
    ) -> [u8; 7] {
        let mut p = [0u8; 7];
        p[0] = (interface_type & 0x0F) | if spread_spectrum { 0x10 } else { 0x00 };
        p[1] = num_lanes & 0x0F;
        p[2..4].copy_from_slice(&min_clock_10khz.to_le_bytes());
        p[4..6].copy_from_slice(&max_clock_10khz.to_le_bytes());
        p[6] = content_protection & 0x03;
        p
    }

    #[test]
    fn test_display_interface_displayport_no_cp() {
        // DP interface, 4 lanes, 10 kHz min, 33750 (337.5 MHz) max, no content protection
        let payload = make_display_interface_payload(0x07, false, 4, 10, 33750, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_interface_block(&payload, &mut caps);
        let iface = caps.display_id_interface.expect("should be Some");
        assert_eq!(iface.interface_type, DisplayInterfaceType::DisplayPort);
        assert!(!iface.spread_spectrum);
        assert_eq!(iface.num_lanes, 4);
        assert_eq!(iface.min_pixel_clock_10khz, 10);
        assert_eq!(iface.max_pixel_clock_10khz, 33750);
        assert_eq!(iface.content_protection, InterfaceContentProtection::None);
    }

    #[test]
    fn test_display_interface_edp_with_spread_spectrum() {
        let payload = make_display_interface_payload(0x06, true, 2, 500, 14850, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_interface_block(&payload, &mut caps);
        let iface = caps.display_id_interface.expect("should be Some");
        assert_eq!(
            iface.interface_type,
            DisplayInterfaceType::EmbeddedDisplayPort
        );
        assert!(iface.spread_spectrum);
        assert_eq!(iface.num_lanes, 2);
    }

    #[test]
    fn test_display_interface_lvds_dual_hdcp() {
        let payload = make_display_interface_payload(0x03, false, 1, 200, 8000, 1);
        let mut caps = DisplayCapabilities::default();
        decode_display_interface_block(&payload, &mut caps);
        let iface = caps.display_id_interface.expect("should be Some");
        assert_eq!(iface.interface_type, DisplayInterfaceType::LvdsDual);
        assert_eq!(iface.content_protection, InterfaceContentProtection::Hdcp);
    }

    #[test]
    fn test_display_interface_tmds_single_dpcp() {
        let payload = make_display_interface_payload(0x04, false, 0, 1000, 14850, 2);
        let mut caps = DisplayCapabilities::default();
        decode_display_interface_block(&payload, &mut caps);
        let iface = caps.display_id_interface.expect("should be Some");
        assert_eq!(iface.interface_type, DisplayInterfaceType::TmdsSingle);
        assert_eq!(iface.content_protection, InterfaceContentProtection::Dpcp);
    }

    #[test]
    fn test_display_interface_reserved_type_stored() {
        // Reserved type 0x0A should be stored as Reserved(0x0A), not discarded.
        let payload = make_display_interface_payload(0x0A, false, 0, 0, 0, 0);
        let mut caps = DisplayCapabilities::default();
        decode_display_interface_block(&payload, &mut caps);
        let iface = caps.display_id_interface.expect("should be Some");
        assert_eq!(iface.interface_type, DisplayInterfaceType::Reserved(0x0A));
    }

    #[test]
    fn test_display_interface_short_payload_skipped() {
        // 6 bytes — one short of the minimum 7.
        let payload = [0x07u8, 0x04, 0x0A, 0x00, 0x00, 0x82];
        let mut caps = DisplayCapabilities::default();
        decode_display_interface_block(&payload, &mut caps);
        assert_eq!(caps.display_id_interface, None);
    }

    #[test]
    fn test_display_interface_empty_payload_skipped() {
        let mut caps = DisplayCapabilities::default();
        decode_display_interface_block(&[], &mut caps);
        assert_eq!(caps.display_id_interface, None);
    }

    // -----------------------------------------------------------------------
    // Stereo Display Interface Data Block (tag 0x10)
    // -----------------------------------------------------------------------

    fn make_stereo_payload(
        viewing_mode: u8,    // bits 3:0
        sync_positive: bool, // bit 4
        sync_interface: u8,  // byte 1
    ) -> [u8; 2] {
        let b0 = (viewing_mode & 0x0F) | if sync_positive { 0x10 } else { 0x00 };
        [b0, sync_interface]
    }

    #[test]
    fn test_stereo_field_sequential_ir() {
        let payload = make_stereo_payload(0, true, 2); // field seq, positive polarity, IR
        let mut caps = DisplayCapabilities::default();
        decode_stereo_display_interface_block(&payload, &mut caps);
        let s = caps.stereo_interface.expect("should be Some");
        assert_eq!(s.viewing_mode, StereoViewingMode::FieldSequential);
        assert!(s.sync_polarity_positive);
        assert_eq!(s.sync_interface, StereoSyncInterface::Infrared);
    }

    #[test]
    fn test_stereo_side_by_side_display_connector() {
        let payload = make_stereo_payload(1, false, 0);
        let mut caps = DisplayCapabilities::default();
        decode_stereo_display_interface_block(&payload, &mut caps);
        let s = caps.stereo_interface.expect("should be Some");
        assert_eq!(s.viewing_mode, StereoViewingMode::SideBySide);
        assert!(!s.sync_polarity_positive);
        assert_eq!(s.sync_interface, StereoSyncInterface::DisplayConnector);
    }

    #[test]
    fn test_stereo_top_and_bottom_vesa_din() {
        let payload = make_stereo_payload(2, false, 1);
        let mut caps = DisplayCapabilities::default();
        decode_stereo_display_interface_block(&payload, &mut caps);
        let s = caps.stereo_interface.expect("should be Some");
        assert_eq!(s.viewing_mode, StereoViewingMode::TopAndBottom);
        assert_eq!(s.sync_interface, StereoSyncInterface::VesaDin);
    }

    #[test]
    fn test_stereo_row_interleaved_rf() {
        let payload = make_stereo_payload(3, false, 3);
        let mut caps = DisplayCapabilities::default();
        decode_stereo_display_interface_block(&payload, &mut caps);
        let s = caps.stereo_interface.expect("should be Some");
        assert_eq!(s.viewing_mode, StereoViewingMode::RowInterleaved);
        assert_eq!(s.sync_interface, StereoSyncInterface::RadioFrequency);
    }

    #[test]
    fn test_stereo_pixel_interleaved_reserved_sync() {
        // Pixel interleaved with a reserved sync interface value (e.g. 0x0A)
        let payload = make_stereo_payload(5, false, 0x0A);
        let mut caps = DisplayCapabilities::default();
        decode_stereo_display_interface_block(&payload, &mut caps);
        let s = caps.stereo_interface.expect("should be Some");
        assert_eq!(s.viewing_mode, StereoViewingMode::PixelInterleaved);
        assert_eq!(s.sync_interface, StereoSyncInterface::Reserved(0x0A));
    }

    #[test]
    fn test_stereo_reserved_viewing_mode_stored() {
        // Reserved viewing mode 0x09 should be stored as Reserved(0x09).
        let payload = make_stereo_payload(0x09, false, 0);
        let mut caps = DisplayCapabilities::default();
        decode_stereo_display_interface_block(&payload, &mut caps);
        let s = caps.stereo_interface.expect("should be Some");
        assert_eq!(s.viewing_mode, StereoViewingMode::Reserved(0x09));
    }

    #[test]
    fn test_stereo_short_payload_skipped() {
        // Only 1 byte — need at least 2.
        let payload = [0x00u8];
        let mut caps = DisplayCapabilities::default();
        decode_stereo_display_interface_block(&payload, &mut caps);
        assert_eq!(caps.stereo_interface, None);
    }

    #[test]
    fn test_stereo_empty_payload_skipped() {
        let mut caps = DisplayCapabilities::default();
        decode_stereo_display_interface_block(&[], &mut caps);
        assert_eq!(caps.stereo_interface, None);
    }

    // -----------------------------------------------------------------------
    // Tiled Display Topology Data Block (tag 0x12)
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)] // mirrors the wire-format field list
    fn make_tiled_topology_payload(
        single_enclosure: bool,
        has_bezel: bool,
        behavior: u8,       // bits 5:4 of byte 0
        h_tiles_minus1: u8, // 0–15
        v_tiles_minus1: u8, // 0–15
        h_location: u8,
        v_location: u8,
        tile_w: u16,
        tile_h: u16,
        bezel: Option<(u8, u8, u8, u8)>, // top, bottom, right, left
    ) -> Vec<u8> {
        let mut v = Vec::new();
        let b0 = if single_enclosure { 0x80 } else { 0 }
            | if has_bezel { 0x40 } else { 0 }
            | ((behavior & 0x03) << 4);
        v.push(b0);
        v.push((h_tiles_minus1 << 4) | (v_tiles_minus1 & 0x0F));
        v.push((h_location << 4) | (v_location & 0x0F));
        v.extend_from_slice(&tile_w.to_le_bytes());
        v.extend_from_slice(&tile_h.to_le_bytes());
        if let Some((top, bot, right, left)) = bezel {
            v.extend_from_slice(&[top, bot, right, left]);
        }
        v
    }

    #[test]
    fn test_tiled_topology_2x2_grid_top_left_tile() {
        // 2×2 grid, this tile is at position (0,0) = top-left, 1920×1080
        let payload = make_tiled_topology_payload(true, false, 1, 1, 1, 0, 0, 1920, 1080, None);
        let mut caps = DisplayCapabilities::default();
        decode_tiled_topology_block(&payload, &mut caps);
        let t = caps.tiled_topology.expect("should be Some");
        assert!(t.single_enclosure);
        assert_eq!(t.topology_behavior, TileTopologyBehavior::RequireAllTiles);
        assert_eq!(t.h_tile_count, 2);
        assert_eq!(t.v_tile_count, 2);
        assert_eq!(t.h_tile_location, 0);
        assert_eq!(t.v_tile_location, 0);
        assert_eq!(t.tile_width_px, 1920);
        assert_eq!(t.tile_height_px, 1080);
        assert_eq!(t.bezel, None);
    }

    #[test]
    fn test_tiled_topology_with_bezel_info() {
        // 3×1 grid, this tile is at (1,0), 2560×1440, bezel top=8 bot=8 right=4 left=4
        let payload =
            make_tiled_topology_payload(false, true, 2, 2, 0, 1, 0, 2560, 1440, Some((8, 8, 4, 4)));
        let mut caps = DisplayCapabilities::default();
        decode_tiled_topology_block(&payload, &mut caps);
        let t = caps.tiled_topology.expect("should be Some");
        assert!(!t.single_enclosure);
        assert_eq!(t.topology_behavior, TileTopologyBehavior::ScaleWhenMissing);
        assert_eq!(t.h_tile_count, 3);
        assert_eq!(t.v_tile_count, 1);
        assert_eq!(t.h_tile_location, 1);
        assert_eq!(t.v_tile_location, 0);
        assert_eq!(t.tile_width_px, 2560);
        let bezel = t.bezel.expect("bezel should be Some");
        assert_eq!(bezel.top_px, 8);
        assert_eq!(bezel.bottom_px, 8);
        assert_eq!(bezel.right_px, 4);
        assert_eq!(bezel.left_px, 4);
    }

    #[test]
    fn test_tiled_topology_bezel_flag_set_but_payload_too_short_gives_none() {
        // has_bezel flag set, but only 7 bytes (not the 11 needed for bezel)
        let payload = make_tiled_topology_payload(
            true, true, 0, 1, 1, 0, 0, 1920, 1080, None, // bezel=None → 7 bytes
        );
        assert_eq!(payload.len(), 7);
        let mut caps = DisplayCapabilities::default();
        decode_tiled_topology_block(&payload, &mut caps);
        let t = caps.tiled_topology.expect("should be Some");
        assert_eq!(t.bezel, None); // flag was set but bytes aren't there
    }

    #[test]
    fn test_tiled_topology_max_grid_16x16() {
        // 16×16 grid (h_tiles_minus1=15, v_tiles_minus1=15), tile at (15,15)
        let payload = make_tiled_topology_payload(false, false, 0, 15, 15, 15, 15, 800, 600, None);
        let mut caps = DisplayCapabilities::default();
        decode_tiled_topology_block(&payload, &mut caps);
        let t = caps.tiled_topology.expect("should be Some");
        assert_eq!(t.h_tile_count, 16);
        assert_eq!(t.v_tile_count, 16);
        assert_eq!(t.h_tile_location, 15);
        assert_eq!(t.v_tile_location, 15);
    }

    #[test]
    fn test_tiled_topology_short_payload_skipped() {
        // Only 6 bytes — one short of the minimum 7.
        let payload = vec![0x80u8, 0x11, 0x00, 0x80, 0x07, 0x38];
        let mut caps = DisplayCapabilities::default();
        decode_tiled_topology_block(&payload, &mut caps);
        assert_eq!(caps.tiled_topology, None);
    }

    #[test]
    fn test_tiled_topology_empty_payload_skipped() {
        let mut caps = DisplayCapabilities::default();
        decode_tiled_topology_block(&[], &mut caps);
        assert_eq!(caps.tiled_topology, None);
    }

    // -----------------------------------------------------------------------
    // V2 Tiled Display Topology Data Block (tag 0x28)
    // -----------------------------------------------------------------------

    #[test]
    fn test_v2_tiled_topology_dispatched_for_v2_section() {
        let body = make_tiled_topology_payload(true, false, 1, 1, 1, 0, 0, 1920, 1080, None);
        let mut block_payload = Vec::new();
        block_payload.push(TAG_V2_TILED_TOPOLOGY);
        block_payload.push(0x00);
        block_payload.push(body.len() as u8);
        block_payload.extend_from_slice(&body);

        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &block_payload,
            DISPLAYID_V2,
            &mut caps,
            &mut did,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        let t = caps
            .tiled_topology
            .expect("V2 0x28 should populate tiled_topology");
        assert_eq!(t.h_tile_count, 2);
        assert_eq!(t.v_tile_count, 2);
    }

    #[test]
    fn test_v2_tiled_topology_v1_tag_ignored_in_v2_section() {
        // 1.x tag 0x12 must not decode under a V2 section header.
        let body = make_tiled_topology_payload(true, false, 1, 1, 1, 0, 0, 1920, 1080, None);
        let mut block_payload = Vec::new();
        block_payload.push(TAG_TILED_TOPOLOGY); // 0x12 (V1 tag)
        block_payload.push(0x00);
        block_payload.push(body.len() as u8);
        block_payload.extend_from_slice(&body);

        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &block_payload,
            DISPLAYID_V2,
            &mut caps,
            &mut did,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert!(caps.tiled_topology.is_none());
    }

    #[test]
    fn test_v2_tiled_topology_v2_tag_ignored_in_v1_section() {
        // 2.x tag 0x28 must not decode under a V1 section header.
        let body = make_tiled_topology_payload(true, false, 1, 1, 1, 0, 0, 1920, 1080, None);
        let mut block_payload = Vec::new();
        block_payload.push(TAG_V2_TILED_TOPOLOGY); // 0x28
        block_payload.push(0x00);
        block_payload.push(body.len() as u8);
        block_payload.extend_from_slice(&body);

        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x13, 0);
        scan_all_metadata_blocks(
            &block_payload,
            0x13,
            &mut caps,
            &mut did,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert!(caps.tiled_topology.is_none());
    }

    #[test]
    fn test_v2_tiled_topology_first_wins() {
        let first = make_tiled_topology_payload(true, false, 1, 1, 1, 0, 0, 1920, 1080, None);
        let second = make_tiled_topology_payload(false, false, 0, 3, 3, 1, 1, 800, 600, None);
        let mut payload = Vec::new();
        for body in [first, second] {
            payload.push(TAG_V2_TILED_TOPOLOGY);
            payload.push(0x00);
            payload.push(body.len() as u8);
            payload.extend_from_slice(&body);
        }
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &payload,
            DISPLAYID_V2,
            &mut caps,
            &mut did,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        let t = caps
            .tiled_topology
            .expect("first 0x28 should populate tiled_topology");
        assert_eq!(t.h_tile_count, 2);
        assert_eq!(t.v_tile_count, 2);
    }

    // -----------------------------------------------------------------------
    // V2 Stereo Display Interface Block (tag 0x27)
    // -----------------------------------------------------------------------

    fn make_v2_stereo_payload(method: u8, args: &[u8]) -> Vec<u8> {
        // payload[0] = descriptor length (1 method byte + args), [1] = method, [2..] = args.
        let mut v = Vec::with_capacity(2 + args.len());
        v.push(1 + args.len() as u8);
        v.push(method);
        v.extend_from_slice(args);
        v
    }

    #[test]
    fn test_v2_stereo_field_sequential() {
        let p = make_v2_stereo_payload(0x00, &[0x01]); // polarity bit set
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0x00, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(r.timing_scope, StereoTimingScopeV2::ExplicitTimingsOnly);
        assert!(matches!(
            r.method,
            StereoViewingMethodV2::FieldSequential {
                eye_on_high_half: StereoEye::Right
            }
        ));
    }

    #[test]
    fn test_v2_stereo_side_by_side_left_eye_in_left_half() {
        let p = make_v2_stereo_payload(0x01, &[0x00]);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0x00, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(
            r.method,
            StereoViewingMethodV2::SideBySide {
                left_half: StereoEye::Left
            }
        );
    }

    #[test]
    fn test_v2_stereo_side_by_side_right_eye_in_left_half() {
        let p = make_v2_stereo_payload(0x01, &[0x01]);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0x00, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(
            r.method,
            StereoViewingMethodV2::SideBySide {
                left_half: StereoEye::Right
            }
        );
    }

    #[test]
    fn test_v2_stereo_pixel_interleaved() {
        let pattern = [0xAA, 0x55, 0xF0, 0x0F, 0xCC, 0x33, 0xFF, 0x00];
        let p = make_v2_stereo_payload(0x02, &pattern);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0x00, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(
            r.method,
            StereoViewingMethodV2::PixelInterleaved { pattern }
        );
    }

    #[test]
    fn test_v2_stereo_dual_interface_left_no_mirror() {
        let p = make_v2_stereo_payload(0x03, &[0x00]); // eye=L, mirror=00
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0x00, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(
            r.method,
            StereoViewingMethodV2::DualInterface {
                eye: StereoEye::Left,
                mirroring: DualInterfaceMirroring::None
            }
        );
    }

    #[test]
    fn test_v2_stereo_dual_interface_right_top_bottom_mirror() {
        // bit0=1 (Right), bits2:1=10 (TopBottom)
        let p = make_v2_stereo_payload(0x03, &[0b0000_0101]);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0x00, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(
            r.method,
            StereoViewingMethodV2::DualInterface {
                eye: StereoEye::Right,
                mirroring: DualInterfaceMirroring::TopBottom
            }
        );
    }

    #[test]
    fn test_v2_stereo_multi_view() {
        let p = make_v2_stereo_payload(0x04, &[8, 0x42]);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0x00, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(
            r.method,
            StereoViewingMethodV2::MultiView {
                view_count: 8,
                interleaving_method_code: 0x42
            }
        );
    }

    #[test]
    fn test_v2_stereo_stacked_frame_top_is_right() {
        let p = make_v2_stereo_payload(0x05, &[0x01]);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0x00, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(
            r.method,
            StereoViewingMethodV2::StackedFrame {
                top_half: StereoEye::Right
            }
        );
    }

    #[test]
    fn test_v2_stereo_proprietary() {
        let p = make_v2_stereo_payload(0xFF, &[]);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0x00, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(r.method, StereoViewingMethodV2::Proprietary);
    }

    #[test]
    fn test_v2_stereo_reserved_method() {
        let p = make_v2_stereo_payload(0x42, &[0x00]);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0x00, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(r.method, StereoViewingMethodV2::Reserved(0x42));
    }

    #[test]
    fn test_v2_stereo_timing_scope_decoded_from_revision() {
        // Revision bits 7:6 = 0b11 → ListedTimingCodesOnly
        let p = make_v2_stereo_payload(0x00, &[0x00]);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0b1100_0000, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(r.timing_scope, StereoTimingScopeV2::ListedTimingCodesOnly);
        assert!(r.has_timing_codes());
    }

    #[test]
    fn test_v2_stereo_short_payload_skipped() {
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&[0x02], 0x00, &mut did);
        assert!(did.stereo_interface_v2.is_none());
    }

    #[test]
    fn test_v2_stereo_method_args_truncated_skipped() {
        // method 0x02 (pixel interleaved) needs 8 arg bytes; descriptor claims 9 bytes total
        // but payload only carries 4. Decoder must skip without panicking.
        let p = vec![0x09, 0x02, 0xAA, 0x55, 0xF0]; // 1 + 1 + 3 args, claim 9
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0x00, &mut did);
        assert!(did.stereo_interface_v2.is_none());
    }

    #[test]
    fn test_v2_stereo_dispatched_only_for_v2_section() {
        let body = make_v2_stereo_payload(0x01, &[0x00]);
        let mut block_payload = Vec::new();
        block_payload.push(TAG_V2_STEREO_INTERFACE);
        block_payload.push(0x00); // revision
        block_payload.push(body.len() as u8);
        block_payload.extend_from_slice(&body);

        // V1 section: must not decode.
        let mut caps_v1 = DisplayCapabilities::default();
        let mut did_v1 = DisplayIdCapabilities::new(0x13, 0);
        scan_all_metadata_blocks(
            &block_payload,
            0x13,
            &mut caps_v1,
            &mut did_v1,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert!(did_v1.stereo_interface_v2.is_none());

        // V2 section: decoder runs.
        let mut caps_v2 = DisplayCapabilities::default();
        let mut did_v2 = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &block_payload,
            DISPLAYID_V2,
            &mut caps_v2,
            &mut did_v2,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        let r = did_v2.stereo_interface_v2.unwrap();
        assert_eq!(
            r.method,
            StereoViewingMethodV2::SideBySide {
                left_half: StereoEye::Left
            }
        );
    }

    #[test]
    fn test_v2_stereo_first_wins() {
        let first = make_v2_stereo_payload(0x01, &[0x00]); // SideBySide left=L
        let second = make_v2_stereo_payload(0x05, &[0x01]); // StackedFrame top=R
        let mut payload = Vec::new();
        for body in [first, second] {
            payload.push(TAG_V2_STEREO_INTERFACE);
            payload.push(0x00);
            payload.push(body.len() as u8);
            payload.extend_from_slice(&body);
        }
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &payload,
            DISPLAYID_V2,
            &mut caps,
            &mut did,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        let r = did.stereo_interface_v2.unwrap();
        assert!(matches!(r.method, StereoViewingMethodV2::SideBySide { .. }));
    }

    // -----------------------------------------------------------------------
    // V2 ContainerID Block (tag 0x29)
    #[test]
    fn test_v2_stereo_timing_codes_decoded() {
        // ListedTimingCodesOnly scope (revision bits 7:6 = 0b11).
        // Stereo descriptor: method 0x00 (FieldSequential), arg 0x01.
        // Timing codes: one record with type=VIC (0b01), count=2, codes [97, 32].
        let descriptor_len = 2u8; // method byte + 1 arg
        let mut p = vec![descriptor_len, 0x00, 0x01]; // descriptor
        // Header: bits 7:6 = 0b01 (VIC), bits 4:0 = 2 (count)
        p.push((0b01 << 6) | 2);
        p.push(97);
        p.push(32);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0b1100_0000, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(r.timing_scope, StereoTimingScopeV2::ListedTimingCodesOnly);
        assert_eq!(r.timing_codes.len(), 2);
        assert_eq!(r.timing_codes[0].code_type, StereoTimingCodeType::Vic);
        assert_eq!(r.timing_codes[0].code, 97);
        assert_eq!(r.timing_codes[1].code, 32);
    }

    #[test]
    fn test_v2_stereo_timing_codes_multiple_records() {
        // Two records: one DMT (code 0x09), one HDMI VIC (code 1).
        let descriptor_len = 1u8;
        let mut p = vec![descriptor_len, 0xFF]; // Proprietary method, no args
        p.push(1); // DMT (0b00 << 6), count=1
        p.push(0x09);
        p.push((0b10 << 6) | 1); // HDMI VIC, count=1
        p.push(1);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0b0100_0000, &mut did); // ExplicitAndListedTimings
        let r = did.stereo_interface_v2.unwrap();
        assert_eq!(r.timing_codes.len(), 2);
        assert_eq!(r.timing_codes[0].code_type, StereoTimingCodeType::Dmt);
        assert_eq!(r.timing_codes[0].code, 0x09);
        assert_eq!(r.timing_codes[1].code_type, StereoTimingCodeType::HdmiVic);
        assert_eq!(r.timing_codes[1].code, 1);
    }

    #[test]
    fn test_v2_stereo_no_timing_codes_when_scope_excludes_them() {
        // ExplicitTimingsOnly (revision 0x00): no timing-code list even if bytes follow.
        let descriptor_len = 1u8;
        let mut p = vec![descriptor_len, 0xFF]; // Proprietary
        p.push(0x41); // would be a record header if codes were parsed
        p.push(99);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_stereo_interface_block(&p, 0x00, &mut did);
        let r = did.stereo_interface_v2.unwrap();
        assert!(r.timing_codes.is_empty());
    }

    // -----------------------------------------------------------------------

    const SAMPLE_UUID: [u8; 16] = [
        0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88,
    ];

    #[test]
    fn test_v2_container_id_basic() {
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_container_id_block(&SAMPLE_UUID, &mut did);
        assert_eq!(did.container_id, Some(SAMPLE_UUID));
    }

    #[test]
    fn test_v2_container_id_short_payload_skipped() {
        let short = [0u8; 15];
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_container_id_block(&short, &mut did);
        assert!(did.container_id.is_none());
    }

    #[test]
    fn test_v2_container_id_ignores_trailing_bytes() {
        let mut payload = SAMPLE_UUID.to_vec();
        payload.extend_from_slice(&[0xAA; 8]);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_container_id_block(&payload, &mut did);
        assert_eq!(did.container_id, Some(SAMPLE_UUID));
    }

    #[test]
    fn test_v2_container_id_dispatched_only_for_v2_section() {
        let mut block_payload = Vec::new();
        block_payload.push(TAG_V2_CONTAINER_ID);
        block_payload.push(0x00);
        block_payload.push(SAMPLE_UUID.len() as u8);
        block_payload.extend_from_slice(&SAMPLE_UUID);

        // V1 section: must not decode.
        let mut caps_v1 = DisplayCapabilities::default();
        let mut did_v1 = DisplayIdCapabilities::new(0x13, 0);
        scan_all_metadata_blocks(
            &block_payload,
            0x13,
            &mut caps_v1,
            &mut did_v1,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert!(did_v1.container_id.is_none());

        // V2 section: decoder runs.
        let mut caps_v2 = DisplayCapabilities::default();
        let mut did_v2 = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &block_payload,
            DISPLAYID_V2,
            &mut caps_v2,
            &mut did_v2,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert_eq!(did_v2.container_id, Some(SAMPLE_UUID));
    }

    #[test]
    fn test_v2_container_id_first_wins() {
        let second = [0xFFu8; 16];
        let mut payload = Vec::new();
        for body in [SAMPLE_UUID, second] {
            payload.push(TAG_V2_CONTAINER_ID);
            payload.push(0x00);
            payload.push(body.len() as u8);
            payload.extend_from_slice(&body);
        }
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &payload,
            DISPLAYID_V2,
            &mut caps,
            &mut did,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert_eq!(did.container_id, Some(SAMPLE_UUID));
    }

    // -----------------------------------------------------------------------
    // V2 Vendor-Specific Block (tag 0x7E)
    // -----------------------------------------------------------------------

    const DOLBY_OUI: [u8; 3] = [0x00, 0xD0, 0x46];
    const MICROSOFT_OUI: [u8; 3] = [0xCA, 0x12, 0x5C];

    #[test]
    fn test_v2_vendor_specific_basic() {
        let mut payload = DOLBY_OUI.to_vec();
        payload.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_vendor_specific_block(&payload, &mut did);
        assert_eq!(did.vendor_specific.len(), 1);
        assert_eq!(did.vendor_specific[0].oui, DOLBY_OUI);
        assert_eq!(
            did.vendor_specific[0].data.as_slice(),
            &[0xDE, 0xAD, 0xBE, 0xEF]
        );
    }

    #[test]
    fn test_v2_vendor_specific_oui_only_no_data() {
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_vendor_specific_block(&DOLBY_OUI, &mut did);
        assert_eq!(did.vendor_specific.len(), 1);
        assert_eq!(did.vendor_specific[0].oui, DOLBY_OUI);
        assert!(did.vendor_specific[0].data.is_empty());
    }

    #[test]
    fn test_v2_vendor_specific_short_payload_skipped() {
        let short = [0x00, 0xD0]; // 2 bytes, no full OUI
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        decode_v2_vendor_specific_block(&short, &mut did);
        assert!(did.vendor_specific.is_empty());
    }

    #[test]
    fn test_v2_vendor_specific_dispatched_only_for_v2_section() {
        let body = {
            let mut v = DOLBY_OUI.to_vec();
            v.extend_from_slice(&[0x01, 0x02]);
            v
        };
        let mut block_payload = Vec::new();
        block_payload.push(TAG_V2_VENDOR_SPECIFIC);
        block_payload.push(0x00);
        block_payload.push(body.len() as u8);
        block_payload.extend_from_slice(&body);

        // V1 section: 0x7E is not a 1.x tag — must not decode.
        let mut caps_v1 = DisplayCapabilities::default();
        let mut did_v1 = DisplayIdCapabilities::new(0x13, 0);
        scan_all_metadata_blocks(
            &block_payload,
            0x13,
            &mut caps_v1,
            &mut did_v1,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert!(did_v1.vendor_specific.is_empty());

        // V2 section: decoder runs.
        let mut caps_v2 = DisplayCapabilities::default();
        let mut did_v2 = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &block_payload,
            DISPLAYID_V2,
            &mut caps_v2,
            &mut did_v2,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert_eq!(did_v2.vendor_specific.len(), 1);
        assert_eq!(did_v2.vendor_specific[0].oui, DOLBY_OUI);
    }

    #[test]
    fn test_v2_vendor_specific_collects_multiple_in_payload_order() {
        let first = {
            let mut v = DOLBY_OUI.to_vec();
            v.push(0x11);
            v
        };
        let second = {
            let mut v = MICROSOFT_OUI.to_vec();
            v.extend_from_slice(&[0x22, 0x33]);
            v
        };
        let mut payload = Vec::new();
        for body in [&first, &second] {
            payload.push(TAG_V2_VENDOR_SPECIFIC);
            payload.push(0x00);
            payload.push(body.len() as u8);
            payload.extend_from_slice(body);
        }
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &payload,
            DISPLAYID_V2,
            &mut caps,
            &mut did,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert_eq!(did.vendor_specific.len(), 2);
        assert_eq!(did.vendor_specific[0].oui, DOLBY_OUI);
        assert_eq!(did.vendor_specific[0].data.as_slice(), &[0x11]);
        assert_eq!(did.vendor_specific[1].oui, MICROSOFT_OUI);
        assert_eq!(did.vendor_specific[1].data.as_slice(), &[0x22, 0x33]);
    }

    // -----------------------------------------------------------------------
    // First-wins: each single-instance scan function ignores duplicate blocks
    // -----------------------------------------------------------------------

    /// Wraps `payload` in a 3-byte data block header for `tag`.
    fn make_block(tag: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![tag, 0x00, payload.len() as u8];
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn test_scan_product_id_first_wins() {
        let first = make_block(
            0x00,
            &make_product_id_payload(
                pack_manufacturer_id(b'S', b'A', b'M'),
                0x1111,
                0,
                0,
                0,
                None,
            ),
        );
        let second = make_block(
            0x00,
            &make_product_id_payload(
                pack_manufacturer_id(b'D', b'E', b'L'),
                0x2222,
                0,
                0,
                0,
                None,
            ),
        );
        let payload = [first, second].concat();
        let mut caps = DisplayCapabilities::default();
        scan_product_id_block(&payload, &mut caps);
        assert_eq!(caps.manufacturer, Some(ManufacturerId(*b"SAM")));
        assert_eq!(caps.product_code, Some(0x1111));
    }

    #[test]
    fn test_scan_display_params_first_wins() {
        let first = make_block(0x01, &make_display_params_payload(6000, 4000, 0, 0, 0, 0));
        let second = make_block(0x01, &make_display_params_payload(3000, 2000, 0, 0, 0, 0));
        let payload = [first, second].concat();
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x12, 0);
        scan_display_params_block(&payload, &mut caps, &mut did);
        let size = DisplayIdImageSize::new(6000, 4000, ImageSizeUnit::TenthMillimetre);
        assert_eq!(did.image_size, Some(size));
    }

    #[test]
    fn test_scan_color_characteristics_first_wins() {
        let first = make_block(
            0x02,
            &make_color_characteristics_payload((100, 50), (200, 150), (50, 25), (300, 300)),
        );
        let second = make_block(
            0x02,
            &make_color_characteristics_payload((900, 800), (700, 600), (500, 400), (512, 512)),
        );
        let payload = [first, second].concat();
        let mut caps = DisplayCapabilities::default();
        scan_color_characteristics_block(&payload, &mut caps);
        assert_eq!(caps.chromaticity.red.x_raw, 100);
        assert_eq!(caps.chromaticity.red.y_raw, 50);
    }

    #[test]
    fn test_scan_video_timing_range_first_wins() {
        // max_pixel_clock: first = 14850×10kHz → 148 MHz, second = 30000×10kHz → 300 MHz
        let first = make_block(
            0x09,
            &make_video_timing_range_payload(0, 14850, 30, 83, 0, 48, 75, 0, 0),
        );
        let second = make_block(
            0x09,
            &make_video_timing_range_payload(0, 30000, 10, 200, 0, 24, 240, 0, 0),
        );
        let payload = [first, second].concat();
        let mut caps = DisplayCapabilities::default();
        scan_video_timing_range_block(&payload, &mut caps);
        assert_eq!(caps.max_pixel_clock_mhz, Some(148));
        assert_eq!(caps.max_h_rate_khz, Some(83));
    }

    #[test]
    fn test_scan_serial_number_first_wins() {
        let first = make_block(0x0A, b"FIRST\x0a       ");
        let second = make_block(0x0A, b"SECOND\x0a      ");
        let payload = [first, second].concat();
        let mut caps = DisplayCapabilities::default();
        scan_serial_number_block(&payload, &mut caps);
        assert_eq!(
            caps.serial_number_string,
            Some(MonitorString(*b"FIRST\x0a       "))
        );
    }

    #[test]
    fn test_scan_display_device_data_first_wins() {
        // byte 0: technology nibble 0x0 = Tft; 0x6 = Oled. Pad to 1 byte payload.
        let first = make_block(0x0C, &[0x00]); // Tft
        let second = make_block(0x0C, &[0x60]); // Oled
        let payload = [first, second].concat();
        let mut caps = DisplayCapabilities::default();
        scan_display_device_data_block(&payload, &mut caps);
        assert_eq!(caps.display_technology, Some(DisplayTechnology::Tft));
    }

    #[test]
    fn test_scan_power_sequencing_first_wins() {
        let first = make_block(0x0D, &[10, 20, 30, 40, 50, 60]);
        let second = make_block(0x0D, &[99, 99, 99, 99, 99, 99]);
        let payload = [first, second].concat();
        let mut caps = DisplayCapabilities::default();
        scan_power_sequencing_block(&payload, &mut caps);
        let ps = caps.power_sequencing.unwrap();
        assert_eq!(ps.t1_power_to_signal, 10);
        assert_eq!(ps.t2_signal_to_backlight, 20);
    }

    #[test]
    fn test_scan_transfer_characteristics_first_wins() {
        // byte 0 = 0x00: 8-bit encoding, single channel. byte 1 = sample value.
        let first = make_block(0x0E, &[0x00, 0xFF]); // one point at 1.0
        let second = make_block(0x0E, &[0x00, 0x00]); // one point at 0.0
        let payload = [first, second].concat();
        let mut caps = DisplayCapabilities::default();
        scan_transfer_characteristics_block(&payload, &mut caps);
        let tc = caps.transfer_characteristic.unwrap();
        if let TransferCurve::Luminance(pts) = tc.curve {
            assert_eq!(pts.len(), 1);
            assert_eq!(pts[0], 1.0_f32);
        } else {
            panic!("expected Luminance curve");
        }
    }

    #[test]
    fn test_scan_display_interface_first_wins() {
        // byte 0 bits 3:0: 0x7 = DisplayPort, 0x2 = LvdsSingle. Payload must be >= 7 bytes.
        let first = make_block(0x0F, &[0x07, 0, 0, 0, 0, 0, 0]);
        let second = make_block(0x0F, &[0x02, 0, 0, 0, 0, 0, 0]);
        let payload = [first, second].concat();
        let mut caps = DisplayCapabilities::default();
        scan_display_interface_block(&payload, &mut caps);
        assert_eq!(
            caps.display_id_interface.unwrap().interface_type,
            DisplayInterfaceType::DisplayPort,
        );
    }

    #[test]
    fn test_scan_stereo_display_interface_first_wins() {
        // byte 0 bits 3:0: 0x0 = FieldSequential, 0x1 = SideBySide. Payload must be >= 2 bytes.
        let first = make_block(0x10, &[0x00, 0x00]); // FieldSequential
        let second = make_block(0x10, &[0x01, 0x00]); // SideBySide
        let payload = [first, second].concat();
        let mut caps = DisplayCapabilities::default();
        scan_stereo_display_interface_block(&payload, &mut caps);
        assert_eq!(
            caps.stereo_interface.unwrap().viewing_mode,
            StereoViewingMode::FieldSequential,
        );
    }

    #[test]
    fn test_scan_tiled_topology_first_wins() {
        // byte 1 bits 7:4 = h_tile_count-1, bits 3:0 = v_tile_count-1.
        // Payload must be >= 7 bytes; bytes 3-6 are tile dimensions (can be zero).
        let first = make_block(0x12, &[0x00, 0x11, 0x00, 0, 0, 0, 0]); // 2×2 grid
        let second = make_block(0x12, &[0x00, 0x22, 0x00, 0, 0, 0, 0]); // 3×3 grid
        let payload = [first, second].concat();
        let mut caps = DisplayCapabilities::default();
        scan_tiled_topology_block(&payload, &mut caps);
        let topo = caps.tiled_topology.unwrap();
        assert_eq!(topo.h_tile_count, 2);
        assert_eq!(topo.v_tile_count, 2);
    }

    // -----------------------------------------------------------------------
    // V2 CTA DisplayID Block (tag 0x81)
    // -----------------------------------------------------------------------

    /// Builds a CTA-861 Video Data Block holding the given VICs as Short Video Descriptors
    /// (1 byte each, native bit clear). Returns the bytes including the 1-byte CTA header.
    fn cta_video_data_block(vics: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(1 + vics.len());
        out.push((0x02 << 5) | (vics.len() as u8 & 0x1F)); // tag=2, length=N
        out.extend_from_slice(vics);
        out
    }

    /// Wraps a CTA-861 data block collection in a DisplayID 2.x block header (tag 0x81,
    /// revision `revision`, length = collection.len()). Returns bytes ready to drop into a
    /// DisplayID V2 section payload.
    fn make_v2_cta_displayid_block(revision: u8, collection: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(3 + collection.len());
        out.push(TAG_V2_CTA_DISPLAYID);
        out.push(revision);
        out.push(collection.len() as u8);
        out.extend_from_slice(collection);
        out
    }

    #[test]
    fn test_v2_cta_displayid_basic_decodes_vics() {
        // VIC 1 = 640x480@60.
        let collection = cta_video_data_block(&[1]);
        let payload = make_v2_cta_displayid_block(0, &collection);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        let mut warnings: Vec<ParseWarning> = Vec::new();
        scan_all_metadata_blocks(
            &payload,
            DISPLAYID_V2,
            &mut caps,
            &mut did,
            &mut warnings,
            &DecodeOptions::new(),
        );

        let cea = caps
            .get_extension_data::<crate::capabilities::cea861::Cea861Capabilities>(0x02)
            .expect("0x81 must populate the 0x02 Cea861Capabilities entry");
        assert_eq!(cea.vics, [(1, false)]);
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 640 && m.height == 480),
            "VIC 1 mode 640x480 must reach caps.supported_modes",
        );
        assert!(warnings.is_empty(), "revision 0 must not warn");
    }

    #[test]
    fn test_v2_cta_displayid_dispatched_only_for_v2_section() {
        let collection = cta_video_data_block(&[1]);
        let payload = make_v2_cta_displayid_block(0, &collection);

        // V1 section: 0x81 is outside the V1 metadata tag space; decoder must not run.
        let mut caps_v1 = DisplayCapabilities::default();
        let mut did_v1 = DisplayIdCapabilities::new(0x13, 0);
        scan_all_metadata_blocks(
            &payload,
            0x13,
            &mut caps_v1,
            &mut did_v1,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );
        assert!(
            caps_v1
                .get_extension_data::<crate::capabilities::cea861::Cea861Capabilities>(0x02)
                .is_none(),
        );
    }

    #[test]
    fn test_v2_cta_displayid_merges_with_existing_cea_extension_data() {
        // Pre-populate as if the CEA-861 handler ran first and stored two VICs.
        let mut caps = DisplayCapabilities::default();
        let mut existing = crate::capabilities::cea861::Cea861Capabilities::new(
            crate::capabilities::cea861::Cea861Flags::empty(),
        );
        existing.vics.push((16, true)); // VIC 16 = 1920x1080@60
        caps.set_extension_data(0x02, existing);

        // Now run 0x81 carrying VIC 1 (640x480@60).
        let collection = cta_video_data_block(&[1]);
        let payload = make_v2_cta_displayid_block(0, &collection);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &payload,
            DISPLAYID_V2,
            &mut caps,
            &mut did,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );

        let cea = caps
            .get_extension_data::<crate::capabilities::cea861::Cea861Capabilities>(0x02)
            .expect("merged Cea861Capabilities must remain at tag 0x02");
        // Both VICs preserved, in observation order.
        assert!(cea.vics.contains(&(16, true)));
        assert!(cea.vics.contains(&(1, false)));
        assert_eq!(cea.vics.len(), 2);
    }

    #[test]
    fn test_v2_cta_displayid_dedupes_modes_against_supported_modes() {
        // Pre-populate supported_modes with VIC 1's resolution.
        let mut caps = DisplayCapabilities::default();
        caps.supported_modes
            .push(crate::model::capabilities::VideoMode::new(
                640, 480, 60u32, false,
            ));

        let collection = cta_video_data_block(&[1]);
        let payload = make_v2_cta_displayid_block(0, &collection);
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &payload,
            DISPLAYID_V2,
            &mut caps,
            &mut did,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );

        let count_640x480 = caps
            .supported_modes
            .iter()
            .filter(|m| m.width == 640 && m.height == 480)
            .count();
        assert_eq!(count_640x480, 1, "duplicate VIC mode must not be appended");
    }

    #[test]
    fn test_v2_cta_displayid_warns_on_nonzero_revision_and_parses_anyway() {
        let collection = cta_video_data_block(&[1]);
        let payload = make_v2_cta_displayid_block(0x05, &collection);
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        let mut warnings: Vec<ParseWarning> = Vec::new();
        scan_all_metadata_blocks(
            &payload,
            DISPLAYID_V2,
            &mut caps,
            &mut did,
            &mut warnings,
            &DecodeOptions::new(),
        );

        assert_eq!(warnings.len(), 1);
        let msg = warnings[0].to_string();
        assert!(
            msg.contains("0x81"),
            "warning text must reference tag 0x81: {msg}"
        );
        // Payload still parsed.
        let cea = caps
            .get_extension_data::<crate::capabilities::cea861::Cea861Capabilities>(0x02)
            .expect("payload still decoded under revision-0 wire format");
        assert_eq!(cea.vics, [(1, false)]);
    }

    #[test]
    fn test_v2_cta_displayid_multiple_blocks_accumulate() {
        // Two 0x81 blocks in a single payload — both should contribute.
        let mut payload = make_v2_cta_displayid_block(0, &cta_video_data_block(&[1]));
        payload.extend(make_v2_cta_displayid_block(0, &cta_video_data_block(&[16])));
        let mut caps = DisplayCapabilities::default();
        let mut did = DisplayIdCapabilities::new(0x20, 0);
        scan_all_metadata_blocks(
            &payload,
            DISPLAYID_V2,
            &mut caps,
            &mut did,
            &mut Vec::new(),
            &DecodeOptions::new(),
        );

        let cea = caps
            .get_extension_data::<crate::capabilities::cea861::Cea861Capabilities>(0x02)
            .expect("Cea861Capabilities must accumulate across multiple 0x81 blocks");
        assert!(cea.vics.contains(&(1, false)));
        assert!(cea.vics.contains(&(16, false)));
    }
}
