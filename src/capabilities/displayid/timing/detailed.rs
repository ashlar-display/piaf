use crate::model::capabilities::{ModeSink, RefreshRate, StereoMode, SyncDefinition, VideoMode};

/// Decodes one 20-byte Type I Video Timing descriptor and pushes a mode to `sink`.
///
/// Type I is "superseded by Type VII" (DisplayID 1.x §4.4.2 / 2.x §4.5.7) and shares
/// Type VII's wire layout exactly except for the pixel-clock unit: Type I uses 10 kHz
/// steps, Type VII uses 1 kHz. Every multi-byte field stores `value − 1` (the wire
/// encoding can't represent the all-zero descriptor as a valid mode, so the spec
/// reserves the all-zero state and shifts everything else down by one).
///
/// Descriptor layout:
/// - Bytes 0–2:   Pixel clock in 10 kHz units, 24-bit LE; stored as `value − 1`
/// - Byte  3:     Options:
///   - bits 3:0 = aspect ratio code (informational; not decoded here)
///   - bit  4   = interlaced
///   - bits 6:5 = stereo (mono / 3D / depends-on-user / reserved; not decoded here)
///   - bit  7   = preferred (not decoded here)
/// - Bytes 4–5:   H Active in pixels, LE; stored as `value − 1`
/// - Bytes 6–7:   H Blank in pixels, LE; stored as `value − 1`
/// - Bytes 8–9:   H Front Porch, bits 14:0 LE (`value − 1`); bit 15 = HSync polarity
///   (`1` = positive)
/// - Bytes 10–11: H Sync Width in pixels, LE; stored as `value − 1`
/// - Bytes 12–13: V Active in lines, LE; stored as `value − 1`
/// - Bytes 14–15: V Blank in lines, LE; stored as `value − 1`
/// - Bytes 16–17: V Front Porch, bits 14:0 LE (`value − 1`); bit 15 = VSync polarity
///   (`1` = positive)
/// - Bytes 18–19: V Sync Width in lines, LE; stored as `value − 1`
///
/// Null descriptors (pixel clock = 0 raw, i.e. encoded value 1) are silently skipped;
/// degenerate total sizes (h_active = 0 after the −1 offset, etc.) are skipped.
/// Stereo, aspect ratio, and preferred bits (byte 3) are not yet surfaced.
pub(super) fn decode_type_i_descriptor(d: &[u8; 20], sink: &mut dyn ModeSink) {
    if let Some(mode) = decode_type_1_7_descriptor_body(d, 10) {
        sink.push_mode(mode);
    }
}

/// Shared body decoder for Type I (10 kHz pixel-clock unit) and Type VII (1 kHz).
///
/// `pixel_clock_unit_khz` multiplies the 24-bit raw value (after the `+1` decode) to
/// produce the final `pixel_clock_khz`. Pass `10` for Type I, `1` for Type VII.
///
/// Returns `None` for null descriptors (raw pixel clock = 0, before the +1 decode),
/// for descriptors with zero active dimensions or zero blanking, or when the reduced
/// refresh rate doesn't fit in [`RefreshRate`].
fn decode_type_1_7_descriptor_body(d: &[u8; 20], pixel_clock_unit_khz: u32) -> Option<VideoMode> {
    let pixel_clock_raw = u32::from(d[0]) | (u32::from(d[1]) << 8) | (u32::from(d[2]) << 16);
    if pixel_clock_raw == 0 {
        return None; // null descriptor
    }
    let pixel_clock_khz = (pixel_clock_raw + 1) * pixel_clock_unit_khz;

    let options = d[3];
    let interlaced = (options & 0x10) != 0;

    let h_active = u16::from_le_bytes([d[4], d[5]]).checked_add(1)?;
    let h_blank = u16::from_le_bytes([d[6], d[7]]).checked_add(1)?;
    let h_fp_word = u16::from_le_bytes([d[8], d[9]]);
    let h_front_porch = (h_fp_word & 0x7FFF) + 1;
    let h_sync_positive = (h_fp_word >> 15) != 0;
    let h_sync_width = u16::from_le_bytes([d[10], d[11]]).checked_add(1)?;

    let v_active = u16::from_le_bytes([d[12], d[13]]).checked_add(1)?;
    let v_blank = u16::from_le_bytes([d[14], d[15]]).checked_add(1)?;
    let v_fp_word = u16::from_le_bytes([d[16], d[17]]);
    let v_front_porch = (v_fp_word & 0x7FFF) + 1;
    let v_sync_positive = (v_fp_word >> 15) != 0;
    let v_sync_width = u16::from_le_bytes([d[18], d[19]]).checked_add(1)?;

    let h_total = u64::from(h_active) + u64::from(h_blank);
    let v_total = u64::from(v_active) + u64::from(v_blank);
    let total_pixels = h_total * v_total;
    if total_pixels == 0 {
        return None;
    }

    let pixel_clock_hz = u64::from(pixel_clock_khz) * 1000;
    let refresh_rate = RefreshRate::from_ratio(pixel_clock_hz, total_pixels)?;

    Some(
        VideoMode::new(h_active, v_active, refresh_rate, interlaced)
            .with_detailed_timing(
                pixel_clock_khz,
                h_front_porch,
                h_sync_width,
                v_front_porch,
                v_sync_width,
                0,
                0,
                StereoMode::None,
                Some(SyncDefinition::DigitalSeparate {
                    v_sync_positive,
                    h_sync_positive,
                }),
            )
            .with_h_blank(h_blank),
    )
}

/// Decodes one 11-byte Type II Video Timing descriptor and pushes a mode to `sink`.
///
/// Descriptor layout (DisplayID 1.x §4.4.3):
/// - Bytes 0–2: Pixel clock in 10 kHz units (little-endian 24-bit; `actual = (raw + 1) × 10 000 Hz`)
/// - Byte 3:    Flags: [7]=preferred, [6:5]=stereo, [4]=interlaced, [3]=HS polarity (+), [2]=VS polarity (+)
/// - Byte 4:    H-active bits 7:0  (9-bit mantissa, 8-pixel granule; `h = 8 + 8 × mantissa`)
/// - Byte 5:    Bit 0 = H-active bit 8; bits 7:1 = H-blank mantissa (7-bit, same granule)
/// - Byte 6:    Bits 7:4 = H-offset mantissa (4-bit); bits 3:0 = H-sync-width mantissa (4-bit)
/// - Byte 7:    V-active bits 7:0  (12-bit mantissa; `v = 1 + mantissa`)
/// - Byte 8:    Bits 3:0 = V-active bits 11:8; bits 7:4 = reserved
/// - Byte 9:    Bits 7:4 = V-front-porch mantissa (`v_fp = 1 + nibble`, range 1–16);
///   bits 3:0 = V-sync-width mantissa (`v_sw = 1 + nibble`, range 1–16).
///   The full byte read as `1 + byte` gives `v_blank` for the timing calculation;
///   the implied back porch is `v_blank − v_fp − v_sw = 15 × (v_fp − 1) − 1`.
/// - Byte 10:   Reserved
pub(super) fn decode_type_ii_descriptor(d: &[u8; 11], sink: &mut dyn ModeSink) {
    let raw_pixel_clock = (d[0] as u32) | ((d[1] as u32) << 8) | ((d[2] as u32) << 16);
    let pixel_clock_10khz = 1u64 + raw_pixel_clock as u64;

    let flags = d[3];
    let interlaced = (flags & 0x10) != 0;
    let h_sync_positive = (flags & 0x08) != 0;
    let v_sync_positive = (flags & 0x04) != 0;

    // Horizontal: 8-pixel granule, each value = 8 + 8 × mantissa.
    let ha_raw = (d[4] as u16) | (((d[5] & 0x01) as u16) << 8);
    let h_active = 8u16 + 8 * ha_raw;

    let hb_raw = ((d[5] >> 1) & 0x7F) as u16;
    let h_blank = 8u16 + 8 * hb_raw;

    let h_front_porch = 8u16 + 8 * ((d[6] >> 4) as u16);
    let h_sync_width = 8u16 + 8 * ((d[6] & 0x0F) as u16);

    // Vertical: 1-line granule, each value = 1 + mantissa.
    let va_raw = (d[7] as u16) | (((d[8] & 0x0F) as u16) << 8);
    let v_active = 1u16 + va_raw;

    // Byte 9: upper nibble = v_fp − 1 (range 1–16); lower nibble = v_sw − 1 (range 1–16).
    // The full byte, read as 1 + byte, gives v_blank for the refresh-rate calculation;
    // back porch is implicit: v_blank − v_fp − v_sw = 15 × (v_fp − 1) − 1.
    let v_blank = 1u16 + d[9] as u16;
    let v_front_porch = 1u16 + ((d[9] >> 4) as u16);
    let v_sync_width = 1u16 + ((d[9] & 0x0F) as u16);

    let h_total = h_active as u64 + h_blank as u64;
    let v_total = v_active as u64 + v_blank as u64;
    if h_total == 0 || v_total == 0 {
        return;
    }

    let pixel_clock_hz = pixel_clock_10khz * 10_000;
    let Some(refresh_rate) = RefreshRate::from_ratio(pixel_clock_hz, h_total * v_total) else {
        return;
    };

    sink.push_mode(
        VideoMode::new(h_active, v_active, refresh_rate, interlaced)
            .with_detailed_timing(
                (pixel_clock_10khz * 10) as u32,
                h_front_porch,
                h_sync_width,
                v_front_porch,
                v_sync_width,
                0,
                0,
                StereoMode::None,
                Some(SyncDefinition::DigitalSeparate {
                    v_sync_positive,
                    h_sync_positive,
                }),
            )
            .with_h_blank(h_blank),
    );
}

/// Decodes one 20-byte Type VII Detailed Timing descriptor body and returns the mode.
///
/// `d` is the bare descriptor — the 2-byte CTA wrapper that prefixes a T7VTDB
/// (`Block_Rev` / `T7Y420` / `T7HSP` / `T7VSP`) is **not** included; callers operating on
/// a CTA T7VTDB must pass `&block_data[2..22]`.
///
/// Type VII shares its byte layout with Type I (DisplayID 1.x `0x03`); the only
/// difference is the pixel-clock unit (Type VII = 1 kHz, Type I = 10 kHz). Decoding
/// is delegated to [`decode_type_1_7_descriptor_body`]. See that function's
/// documentation for the wire-format details (every multi-byte field is `value − 1`,
/// polarity bits live in bit 15 of the H/V Front Porch words).
///
/// Returns `None` for null descriptors (raw pixel clock = 0), zero-sized active/blank
/// fields, or geometry whose reduced refresh rate doesn't fit in [`RefreshRate`].
///
/// `revision` is the DisplayID block revision byte. When `revision >= 2`, byte 3 bit 7
/// is decoded as `y420` (YCbCr 4:2:0 only). Pass `0` for CTA T7VTDB contexts where
/// Y420 is carried at the block level rather than in the descriptor.
pub(crate) fn decode_type_vii_descriptor_to_mode(d: &[u8; 20], revision: u8) -> Option<VideoMode> {
    let mode = decode_type_1_7_descriptor_body(d, 1)?;
    if revision >= 2 && (d[3] & 0x80) != 0 {
        Some(mode.with_y420(true))
    } else {
        Some(mode)
    }
}

/// Decodes one Type VII descriptor and pushes the resulting mode to `sink`.
///
/// `revision` is the DisplayID 2.x block revision byte (from the block header).
/// Thin wrapper around [`decode_type_vii_descriptor_to_mode`]; null/degenerate
/// descriptors are silently dropped.
pub(super) fn decode_type_vii_descriptor(d: &[u8; 20], revision: u8, sink: &mut dyn ModeSink) {
    if let Some(mode) = decode_type_vii_descriptor_to_mode(d, revision) {
        sink.push_mode(mode);
    }
}

/// Decodes one Type VI Video Timing descriptor from `d` and pushes a mode to `sink`.
///
/// Descriptor layout (DisplayID 1.x §4.7):
/// - Bytes 0–2:  Pixel clock in 1 kHz steps (22-bit LE, bits 21:0); bit 22 = aspect/size
///   present (bytes 14–16 included); bit 23 = preferred timing
/// - Bytes 3–4:  H-active pixels: bits 14:0 (15-bit); bit 15 = H-sync polarity (1=positive)
/// - Bytes 5–6:  V-active lines:  bits 14:0 (15-bit); bit 15 = V-sync polarity (1=positive)
/// - Bytes 7–9:  H-blank and H-front-porch, each 12-bit:
///   byte7 = H-blank LSB, byte8 = H-fp LSB, byte9[3:0] = H-blank MSB, byte9[7:4] = H-fp MSB
/// - Byte  10:   H-sync width (8-bit)
/// - Byte  11:   V-blank lines (8-bit)
/// - Byte  12:   V-front-porch lines (8-bit)
/// - Byte  13:   bits 3:0 = V-sync width; bit 7 = interlaced
/// - Bytes 14–16 (optional, present iff bit 22): aspect/size info (not decoded)
///
/// Returns the number of bytes consumed (14 or 17).
/// Returns 0 if `d` is too short for a minimal descriptor.
/// Null descriptors (pixel clock = 0) advance the cursor without emitting a mode.
pub(super) fn decode_type_vi_descriptor(d: &[u8], sink: &mut dyn ModeSink) -> usize {
    if d.len() < 14 {
        return 0;
    }

    let raw = (d[0] as u32) | ((d[1] as u32) << 8) | ((d[2] as u32) << 16);
    let pixel_clock_khz = raw & 0x003F_FFFF; // bits 21:0
    let has_aspect = (raw >> 22) & 1 != 0;
    let descriptor_size = if has_aspect { 17 } else { 14 };

    if pixel_clock_khz == 0 {
        return descriptor_size;
    }

    let h_word = u16::from_le_bytes([d[3], d[4]]);
    let h_active = h_word & 0x7FFF;
    let h_sync_positive = (h_word >> 15) != 0;

    let v_word = u16::from_le_bytes([d[5], d[6]]);
    let v_active = v_word & 0x7FFF;
    let v_sync_positive = (v_word >> 15) != 0;

    // H-blank and H-fp packed as two 12-bit values across bytes 7–9.
    let h_blank = (d[7] as u16) | (((d[9] & 0x0F) as u16) << 8);
    let h_fp = (d[8] as u16) | (((d[9] >> 4) as u16) << 8);

    let h_sync_width = d[10] as u16;
    let v_blank = d[11] as u16;
    let v_fp = d[12] as u16;
    let v_sync_width = (d[13] & 0x0F) as u16;
    let interlaced = (d[13] >> 7) != 0;

    let h_total = h_active as u64 + h_blank as u64;
    let v_total = v_active as u64 + v_blank as u64;
    if h_total == 0 || v_total == 0 {
        return descriptor_size;
    }

    let pixel_clock_hz = pixel_clock_khz as u64 * 1000;
    let Some(refresh_rate) = RefreshRate::from_ratio(pixel_clock_hz, h_total * v_total) else {
        return descriptor_size;
    };

    sink.push_mode(
        VideoMode::new(h_active, v_active, refresh_rate, interlaced)
            .with_detailed_timing(
                pixel_clock_khz,
                h_fp,
                h_sync_width,
                v_fp,
                v_sync_width,
                0,
                0,
                StereoMode::None,
                Some(SyncDefinition::DigitalSeparate {
                    h_sync_positive,
                    v_sync_positive,
                }),
            )
            .with_h_blank(h_blank),
    );

    descriptor_size
}

#[cfg(test)]
#[cfg(any(feature = "alloc", feature = "std"))]
mod tests {
    use super::*;
    use crate::model::capabilities::{DisplayCapabilities, RefreshRate, SyncDefinition};

    /// Builds a Type I descriptor with human-readable field values; the helper
    /// applies the wire-format `value − 1` encoding internally so call sites can
    /// pass true active/blanking dimensions.
    ///
    /// `options_byte3` is written verbatim to byte 3 — set bit 4 (`0x10`) for
    /// interlaced, bits 6:5 for stereo, bit 7 for preferred (rev < 2) or Y420
    /// (rev ≥ 2). Polarity bits (bit 15 of bytes 8–9 / 16–17) are not currently
    /// settable through this helper.
    #[allow(clippy::too_many_arguments)]
    fn make_type_i_descriptor(
        pixel_clock_10khz: u32,
        h_active: u16,
        h_blank: u16,
        h_fp: u16,
        h_sw: u16,
        v_active: u16,
        v_blank: u16,
        v_fp: u16,
        v_sw: u16,
        options_byte3: u8,
    ) -> [u8; 20] {
        let mut d = [0u8; 20];
        let raw_pclk = pixel_clock_10khz - 1;
        d[0] = (raw_pclk & 0xFF) as u8;
        d[1] = ((raw_pclk >> 8) & 0xFF) as u8;
        d[2] = ((raw_pclk >> 16) & 0xFF) as u8;
        d[3] = options_byte3;
        d[4..6].copy_from_slice(&(h_active - 1).to_le_bytes());
        d[6..8].copy_from_slice(&(h_blank - 1).to_le_bytes());
        d[8..10].copy_from_slice(&(h_fp - 1).to_le_bytes());
        d[10..12].copy_from_slice(&(h_sw - 1).to_le_bytes());
        d[12..14].copy_from_slice(&(v_active - 1).to_le_bytes());
        d[14..16].copy_from_slice(&(v_blank - 1).to_le_bytes());
        d[16..18].copy_from_slice(&(v_fp - 1).to_le_bytes());
        d[18..20].copy_from_slice(&(v_sw - 1).to_le_bytes());
        d
    }

    #[allow(clippy::too_many_arguments)]
    fn make_type_ii_descriptor(
        pixel_clock_10khz: u32,
        ha_raw: u16,
        hb_raw: u8,
        hfp_raw: u8,
        hsw_raw: u8,
        va_raw: u16,
        v_blank_byte: u8,
        flags: u8,
    ) -> [u8; 11] {
        let mut d = [0u8; 11];
        d[0] = (pixel_clock_10khz & 0xFF) as u8;
        d[1] = ((pixel_clock_10khz >> 8) & 0xFF) as u8;
        d[2] = ((pixel_clock_10khz >> 16) & 0xFF) as u8;
        d[3] = flags;
        d[4] = (ha_raw & 0xFF) as u8;
        d[5] = (((ha_raw >> 8) & 0x01) as u8) | ((hb_raw & 0x7F) << 1);
        d[6] = ((hfp_raw & 0x0F) << 4) | (hsw_raw & 0x0F);
        d[7] = (va_raw & 0xFF) as u8;
        d[8] = ((va_raw >> 8) & 0x0F) as u8;
        d[9] = v_blank_byte;
        d[10] = 0x00;
        d
    }

    // -----------------------------------------------------------------------
    // Type I Video Timing (tag 0x03)
    // -----------------------------------------------------------------------

    #[test]
    fn test_type_i_timing_decoded() {
        // 1920×1080@60 Hz: pixel clock ≈ 148.5 MHz = 14850 × 10 kHz
        // h_total = 2200, v_total = 1125 → 148500000 / (2200 * 1125) ≈ 60 Hz
        let d = make_type_i_descriptor(14850, 1920, 280, 88, 44, 1080, 45, 4, 5, 0x00);
        let mut caps = DisplayCapabilities::default();
        decode_type_i_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
        assert!(!mode.interlaced);
        assert_eq!(mode.h_blank, Some(280));
    }

    #[test]
    fn test_type_i_null_descriptor_skipped() {
        let null_descriptor = [0u8; 20];
        let mut caps = DisplayCapabilities::default();
        decode_type_i_descriptor(&null_descriptor, &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_i_interlaced_flag_decoded() {
        // byte 3 bit 4 = interlaced
        let d = make_type_i_descriptor(14850, 1920, 280, 88, 44, 1080, 45, 4, 5, 0x10);
        let mut caps = DisplayCapabilities::default();
        decode_type_i_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert!(caps.supported_modes[0].interlaced);
    }

    #[test]
    fn test_type_i_sync_polarity_decoded_from_fp_high_bit() {
        // Build a 1080p60 descriptor and set HSP/VSP polarity bits (bit 15 of bytes
        // 8–9 / 16–17, the H/V Front Porch fields).
        let mut d = make_type_i_descriptor(14850, 1920, 280, 88, 44, 1080, 45, 4, 5, 0x00);
        d[9] |= 0x80; // HSP = positive (H FP word bit 15)
        d[17] |= 0x80; // VSP = positive (V FP word bit 15)
        let mut caps = DisplayCapabilities::default();
        decode_type_i_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(
            caps.supported_modes[0].sync,
            Some(SyncDefinition::DigitalSeparate {
                h_sync_positive: true,
                v_sync_positive: true,
            })
        );
        // Front porch decode must mask bit 15 — the polarity bit must not bleed in.
        assert_eq!(caps.supported_modes[0].h_front_porch, 88);
        assert_eq!(caps.supported_modes[0].v_front_porch, 4);
    }

    #[test]
    fn test_type_i_negative_polarities_default() {
        // Polarity bits clear → both negative.
        let d = make_type_i_descriptor(14850, 1920, 280, 88, 44, 1080, 45, 4, 5, 0x00);
        let mut caps = DisplayCapabilities::default();
        decode_type_i_descriptor(&d, &mut caps);
        assert_eq!(
            caps.supported_modes[0].sync,
            Some(SyncDefinition::DigitalSeparate {
                h_sync_positive: false,
                v_sync_positive: false,
            })
        );
    }

    // -----------------------------------------------------------------------
    // Type II Video Timing (tag 0x04)
    // -----------------------------------------------------------------------

    #[test]
    fn test_type_ii_timing_decoded() {
        // 1920×1080@60 Hz via Type II encoding. The Type II vertical encoding constrains
        // v_blank = 16·v_fp + v_sw − 16 (back porch is implicit). Byte 9 = 0x2C → v_fp=3,
        // v_sw=13, v_blank=45, v_total=1125; pc=14850×10kHz → exact 60 Hz.
        let d = make_type_ii_descriptor(14849, 239, 34, 10, 5, 1079, 0x2C, 0x0C);
        let mut caps = DisplayCapabilities::default();
        decode_type_ii_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
        assert_eq!(mode.h_front_porch, 88);
        assert_eq!(mode.h_sync_width, 48);
        assert_eq!(mode.h_blank, Some(280)); // 8 + 8 × 34
        assert_eq!(mode.v_front_porch, 3);
        assert_eq!(mode.v_sync_width, 13);
        assert!(!mode.interlaced);
        assert_eq!(
            mode.sync,
            Some(SyncDefinition::DigitalSeparate {
                h_sync_positive: true,
                v_sync_positive: true,
            })
        );
    }

    #[test]
    fn test_type_ii_interlaced_flag() {
        // flags byte 3 bit 4 = interlaced
        let d = make_type_ii_descriptor(14849, 239, 34, 10, 5, 1079, 0x2C, 0x10);
        let mut caps = DisplayCapabilities::default();
        decode_type_ii_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert!(caps.supported_modes[0].interlaced);
    }

    #[test]
    fn test_type_ii_multiple_descriptors() {
        // Two descriptors decoded in sequence.
        // 2560×1440@60: ha_raw=(2560-8)/8=319, hb_raw=(440-8)/8=54 → h_total=3000
        // va_raw=1440-1=1439=0x59F, v_blank_byte=0x31→v_blank=50 → v_total=1490
        let desc1 = make_type_ii_descriptor(14849, 239, 34, 10, 5, 1079, 0x2C, 0x0C);
        let desc2 = make_type_ii_descriptor(26819, 319, 54, 10, 4, 1439, 0x31, 0x0C);
        let mut caps = DisplayCapabilities::default();
        decode_type_ii_descriptor(&desc1, &mut caps);
        decode_type_ii_descriptor(&desc2, &mut caps);
        assert_eq!(caps.supported_modes.len(), 2);
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 1920 && m.height == 1080)
        );
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 2560 && m.height == 1440)
        );
    }

    // -----------------------------------------------------------------------
    // Type VI Video Timing (tag 0x13)
    // -----------------------------------------------------------------------

    /// Build a 14-byte Type VI descriptor (no aspect/size bytes).
    ///
    /// pixel_clock_khz: pixel clock in kHz (max ~4 194 303 kHz)
    #[allow(clippy::too_many_arguments)]
    fn make_type_vi_descriptor(
        pixel_clock_khz: u32,
        h_active: u16,
        h_sync_positive: bool,
        v_active: u16,
        v_sync_positive: bool,
        h_blank: u16,
        h_fp: u16,
        h_sync_width: u8,
        v_blank: u8,
        v_fp: u8,
        v_sync_width: u8,
        interlaced: bool,
    ) -> [u8; 14] {
        let mut d = [0u8; 14];
        // Bytes 0–2: pixel clock (22 bits) + flags; bit 22 = 0 (no aspect bytes).
        let pc = pixel_clock_khz & 0x3F_FFFF;
        d[0] = (pc & 0xFF) as u8;
        d[1] = ((pc >> 8) & 0xFF) as u8;
        d[2] = ((pc >> 16) & 0x3F) as u8; // bit 22=0, bit 23=0
        // Bytes 3–4: H active (15 bits) + H sync polarity (bit 15).
        let h_word = (h_active & 0x7FFF) | ((h_sync_positive as u16) << 15);
        d[3..5].copy_from_slice(&h_word.to_le_bytes());
        // Bytes 5–6: V active (15 bits) + V sync polarity (bit 15).
        let v_word = (v_active & 0x7FFF) | ((v_sync_positive as u16) << 15);
        d[5..7].copy_from_slice(&v_word.to_le_bytes());
        // Bytes 7–9: H blank (12-bit) and H fp (12-bit) packed.
        d[7] = (h_blank & 0xFF) as u8;
        d[8] = (h_fp & 0xFF) as u8;
        d[9] = ((h_blank >> 8) & 0x0F) as u8 | (((h_fp >> 8) & 0x0F) << 4) as u8;
        d[10] = h_sync_width;
        d[11] = v_blank;
        d[12] = v_fp;
        d[13] = (v_sync_width & 0x0F) | ((interlaced as u8) << 7);
        d
    }

    #[test]
    fn test_type_vi_1920x1080_decoded() {
        // 1920×1080@60 Hz: pixel clock 148 500 kHz, h_total=2200, v_total=1125
        // 148_500_000 Hz / (2200 × 1125) ≈ 60 Hz
        let d = make_type_vi_descriptor(
            148_500, 1920, true, 1080, true, 280, 88, 44, 45, 4, 5, false,
        );
        let mut caps = DisplayCapabilities::default();
        let consumed = decode_type_vi_descriptor(&d, &mut caps);
        assert_eq!(consumed, 14);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
        assert!(!mode.interlaced);
        assert_eq!(mode.h_blank, Some(280));
        assert_eq!(mode.h_front_porch, 88);
        assert_eq!(mode.h_sync_width, 44);
        assert_eq!(mode.v_front_porch, 4);
        assert_eq!(mode.v_sync_width, 5);
        assert_eq!(
            mode.sync,
            Some(SyncDefinition::DigitalSeparate {
                h_sync_positive: true,
                v_sync_positive: true,
            })
        );
    }

    #[test]
    fn test_type_vi_interlaced_flag() {
        let d =
            make_type_vi_descriptor(148_500, 1920, true, 1080, true, 280, 88, 44, 45, 4, 5, true);
        let mut caps = DisplayCapabilities::default();
        decode_type_vi_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert!(caps.supported_modes[0].interlaced);
    }

    #[test]
    fn test_type_vi_null_descriptor_skipped() {
        // pixel_clock_khz = 0 → null; returns descriptor_size without pushing a mode.
        let d = [0u8; 14];
        let mut caps = DisplayCapabilities::default();
        let consumed = decode_type_vi_descriptor(&d, &mut caps);
        assert_eq!(consumed, 14);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_vi_with_aspect_bytes_returns_17() {
        let mut d = [0u8; 17];
        // Set pixel clock = 148_500 kHz and bit 22 = 1 (aspect bytes present).
        let pc = 148_500u32;
        d[0] = (pc & 0xFF) as u8;
        d[1] = ((pc >> 8) & 0xFF) as u8;
        d[2] = (((pc >> 16) & 0x3F) | 0x40) as u8; // bit 22 set
        // H active = 1920, V active = 1080, h_total = 2200, v_total = 1125.
        let h_word: u16 = 1920;
        d[3..5].copy_from_slice(&h_word.to_le_bytes());
        let v_word: u16 = 1080;
        d[5..7].copy_from_slice(&v_word.to_le_bytes());
        d[7] = 280u16 as u8;
        d[9] = ((280u16 >> 8) & 0x0F) as u8;
        d[11] = 45; // v_blank
        let mut caps = DisplayCapabilities::default();
        let consumed = decode_type_vi_descriptor(&d, &mut caps);
        assert_eq!(consumed, 17);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1920);
    }

    #[test]
    fn test_type_vi_too_short_returns_zero() {
        let d = [0u8; 13]; // one byte short
        let mut caps = DisplayCapabilities::default();
        let consumed = decode_type_vi_descriptor(&d, &mut caps);
        assert_eq!(consumed, 0);
        assert!(caps.supported_modes.is_empty());
    }

    // -----------------------------------------------------------------------
    // Type VII Detailed Timing (DisplayID 2.x tag 0x22)
    // -----------------------------------------------------------------------

    /// Build a 20-byte Type VII descriptor body (no CTA wrapper bytes). Field
    /// arguments take human-readable values; the helper applies the wire-format
    /// `value − 1` encoding internally.
    #[allow(clippy::too_many_arguments)]
    fn make_type_vii_descriptor(
        pixel_clock_khz: u32,
        h_active: u16,
        h_blank: u16,
        h_fp: u16,
        h_sync_positive: bool,
        h_sw: u16,
        v_active: u16,
        v_blank: u16,
        v_fp: u16,
        v_sync_positive: bool,
        v_sw: u16,
        interlaced: bool,
    ) -> [u8; 20] {
        let mut d = [0u8; 20];
        let raw_pclk = pixel_clock_khz - 1;
        d[0] = (raw_pclk & 0xFF) as u8;
        d[1] = ((raw_pclk >> 8) & 0xFF) as u8;
        d[2] = ((raw_pclk >> 16) & 0xFF) as u8;
        d[3] = (interlaced as u8) << 4;
        d[4..6].copy_from_slice(&(h_active - 1).to_le_bytes());
        d[6..8].copy_from_slice(&(h_blank - 1).to_le_bytes());
        let h_fp_word = ((h_fp - 1) & 0x7FFF) | ((h_sync_positive as u16) << 15);
        d[8..10].copy_from_slice(&h_fp_word.to_le_bytes());
        d[10..12].copy_from_slice(&(h_sw - 1).to_le_bytes());
        d[12..14].copy_from_slice(&(v_active - 1).to_le_bytes());
        d[14..16].copy_from_slice(&(v_blank - 1).to_le_bytes());
        let v_fp_word = ((v_fp - 1) & 0x7FFF) | ((v_sync_positive as u16) << 15);
        d[16..18].copy_from_slice(&v_fp_word.to_le_bytes());
        d[18..20].copy_from_slice(&(v_sw - 1).to_le_bytes());
        d
    }

    #[test]
    fn test_type_vii_1080p60_decoded() {
        // 1920×1080@60: pc=148_500 kHz, h_total=2200, v_total=1125 → exact 60 Hz.
        let d = make_type_vii_descriptor(
            148_500, 1920, 280, 88, true, 44, 1080, 45, 4, true, 5, false,
        );
        let mut caps = DisplayCapabilities::default();
        decode_type_vii_descriptor(&d, 0, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
        assert!(!mode.interlaced);
        assert_eq!(mode.h_blank, Some(280));
        assert_eq!(mode.h_front_porch, 88);
        assert_eq!(mode.h_sync_width, 44);
        assert_eq!(mode.v_front_porch, 4);
        assert_eq!(mode.v_sync_width, 5);
        assert_eq!(
            mode.sync,
            Some(SyncDefinition::DigitalSeparate {
                h_sync_positive: true,
                v_sync_positive: true,
            })
        );
    }

    #[test]
    fn test_type_vii_rational_rate_preserved() {
        // The kHz-precision pixel clock can't represent 60_000/1001 exactly, but the
        // decoder must preserve whatever rational ratio falls out of the inputs.
        // Synthesize an exact case: pc = 120_120 kHz, h_total = 1001, v_total = 2
        // → 120_120_000 / 2002 = 60_000 Hz exact. Porch/sync values are irrelevant
        // to the refresh derivation; pass `1` (the minimum representable on the wire).
        let d = make_type_vii_descriptor(120_120, 1000, 1, 1, false, 1, 1, 1, 1, false, 1, false);
        let mut caps = DisplayCapabilities::default();
        decode_type_vii_descriptor(&d, 0, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(
            caps.supported_modes[0].refresh_rate,
            Some(RefreshRate::integral(60_000))
        );
    }

    #[test]
    fn test_type_vii_polarity_negative() {
        let d = make_type_vii_descriptor(
            148_500, 1920, 280, 88, false, 44, 1080, 45, 4, false, 5, false,
        );
        let mut caps = DisplayCapabilities::default();
        decode_type_vii_descriptor(&d, 0, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(
            caps.supported_modes[0].sync,
            Some(SyncDefinition::DigitalSeparate {
                h_sync_positive: false,
                v_sync_positive: false,
            })
        );
    }

    #[test]
    fn test_type_vii_interlaced_flag() {
        let d =
            make_type_vii_descriptor(148_500, 1920, 280, 88, true, 44, 1080, 45, 4, true, 5, true);
        let mut caps = DisplayCapabilities::default();
        decode_type_vii_descriptor(&d, 0, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert!(caps.supported_modes[0].interlaced);
    }

    #[test]
    fn test_type_vii_null_descriptor_skipped() {
        let d = [0u8; 20];
        let mut caps = DisplayCapabilities::default();
        decode_type_vii_descriptor(&d, 0, &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_vii_h_active_overflow_skipped() {
        // Pixel clock non-zero but raw h_active = 0xFFFF would decode to 0x10000
        // (overflow on +1) → descriptor must be skipped. (The wire encoding cannot
        // represent value 0; the previous "zero active" semantics no longer apply.)
        let mut d = make_type_vii_descriptor(
            148_500, 1920, 280, 88, true, 44, 1080, 45, 4, true, 5, false,
        );
        d[4] = 0xFF;
        d[5] = 0xFF;
        let mut caps = DisplayCapabilities::default();
        decode_type_vii_descriptor(&d, 0, &mut caps);
        assert!(caps.supported_modes.is_empty());
    }
}
