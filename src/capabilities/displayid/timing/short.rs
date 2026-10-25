use crate::model::capabilities::{
    CvtAlgorithm, ModeSink, RefreshRate, StereoMode, TypeIxStereoMode, VideoMode,
};
use display_types::compute_type_ix_timing;

fn decode_type_v_ix_stereo(byte0: u8) -> TypeIxStereoMode {
    match (byte0 >> 5) & 0x03 {
        0 => TypeIxStereoMode::Mono,
        1 => TypeIxStereoMode::Stereo,
        2 => TypeIxStereoMode::MonoOrStereoByUser,
        _ => TypeIxStereoMode::Reserved,
    }
}

/// Decodes one 3-byte Type III Short Video Timing descriptor and pushes a mode to `sink`.
///
/// Descriptor layout (DisplayID 1.x §4.4.4):
/// - Byte 0:  Bit 7 = preferred; bits 6:4 = CVT algorithm (0=standard, 1=reduced blanking);
///   bits 3:0 = aspect ratio code (see table below)
/// - Byte 1:  Horizontal active = `(byte + 1) × 8` pixels
/// - Byte 2:  Bit 7 = interlaced; bits 6:0 = refresh rate = `bits + 1` Hz
///
/// Aspect ratio codes:
/// `0`=1:1, `1`=5:4, `2`=4:3, `3`=15:9, `4`=16:9, `5`=16:10, `6`=64:27, `7`=256:135, `8`=undefined
///
/// Vertical active is derived from horizontal active and the aspect ratio. Descriptors
/// with reserved or undefined aspect ratios (code > 7) are silently skipped, as are
/// descriptors where the height calculation does not yield a whole number of lines.
pub(super) fn decode_type_iii_descriptor(d: &[u8; 3], sink: &mut dyn ModeSink) {
    let aspect_code = d[0] & 0x0F;
    let h_active = ((d[1] as u16) + 1) * 8;
    let interlaced = (d[2] & 0x80) != 0;
    let refresh_rate = (d[2] & 0x7F) as u16 + 1;

    // Compute v_active from the aspect ratio (width:height → v = h × height / width).
    // Aspect code 8 = undefined; codes 9–15 = reserved; both are skipped.
    let (ar_h, ar_w): (u32, u32) = match aspect_code {
        0 => (1, 1),     // 1:1
        1 => (4, 5),     // 5:4
        2 => (3, 4),     // 4:3
        3 => (9, 15),    // 15:9
        4 => (9, 16),    // 16:9
        5 => (10, 16),   // 16:10
        6 => (27, 64),   // 64:27
        7 => (135, 256), // 256:135
        _ => return,     // undefined or reserved
    };
    let v_raw = (h_active as u32) * ar_h;
    if v_raw % ar_w != 0 {
        return; // h_active is not a valid value for this aspect ratio
    }
    let v_active = (v_raw / ar_w) as u16;
    if v_active == 0 {
        return;
    }

    sink.push_mode(VideoMode::new(h_active, v_active, refresh_rate, interlaced));
}

/// Decodes one 7-byte Type V Short Video Timing descriptor and pushes a mode to `sink`.
///
/// Descriptor layout (DisplayID 1.x §4.6):
/// - Byte 0:    Options:
///   - bits 2:0 = CVT algorithm (0 = CVT, 1 = CVT-RB, 2 = CVT-R2)
///   - bit  3   = NTSC × (1000/1001) refresh supported
///   - bits 6:5 = stereo (0 = mono, 1 = 3D, 2 = mono-or-3D-by-user, 3 = reserved)
///   - bit  7   = preferred
/// - Bytes 1–2: Horizontal active in pixels (LE; stored as `value − 1`)
/// - Bytes 3–4: Vertical active in lines (LE; stored as `value − 1`)
/// - Byte 5:    Vertical refresh rate — `byte + 1` Hz (range 1–256 Hz)
/// - Byte 6:    Reserved
///
/// Descriptors with raw `h_active = 0xFFFF` or `v_active = 0xFFFF` (which would overflow
/// on `+1`) are silently skipped. Type V is progressive-only.
pub(super) fn decode_type_v_descriptor(d: &[u8; 7], sink: &mut dyn ModeSink) {
    let Some(h_active) = u16::from_le_bytes([d[1], d[2]]).checked_add(1) else {
        return;
    };
    let Some(v_active) = u16::from_le_bytes([d[3], d[4]]).checked_add(1) else {
        return;
    };
    let refresh_rate = (d[5] as u16) + 1;

    let cvt_algorithm = CvtAlgorithm::from_bits(d[0]);
    let ntsc_fractional_refresh = (d[0] >> 3) & 1 != 0;
    let stereo = decode_type_v_ix_stereo(d[0]);

    sink.push_mode(
        VideoMode::new(h_active, v_active, refresh_rate, false)
            .with_cvt_algorithm(cvt_algorithm)
            .with_ntsc_fractional_refresh(ntsc_fractional_refresh)
            .with_type_ix_stereo(stereo),
    );
}

/// Decodes one 6-byte Type IX Formula-Based Timing descriptor and pushes a mode to `sink`.
///
/// Descriptor layout (DisplayID 2.x §4.5.9, "Video Timing Mode Type 9 — Formula-based"):
/// - Byte 0:    Options:
///   - bits 2:0 = CVT algorithm (0 = CVT, 1 = CVT-RB v1, 2 = CVT-R2; 3–7 = reserved)
///   - bit  3   = NTSC × (1000/1001) refresh supported
///   - bits 6:5 = stereo (0 = mono, 1 = 3D, 2 = mono-or-3D-by-user, 3 = reserved)
/// - Bytes 1–2: Horizontal active in pixels (stored as `value − 1`, little-endian)
/// - Bytes 3–4: Vertical active in lines (stored as `value − 1`, little-endian)
/// - Byte 5:    Vertical refresh rate — `byte + 1` Hz (range 1–256 Hz)
///
/// Like Type V, Type IX timings are progressive-only. When the algorithm is CVT-RB v1
/// or CVT-R2, the descriptor is expanded to a full timing via [`compute_type_ix_timing`]
/// and the resulting `pixel_clock_khz`, front porches, and sync widths are populated.
/// Standard CVT (`Cvt`) and reserved algorithm codes produce a minimal `VideoMode` with
/// only the algorithm metadata set.
///
/// Descriptors with raw `h_active = 0xFFFF` or `v_active = 0xFFFF` (which would
/// overflow on `+1`) are silently skipped.
pub(super) fn decode_type_ix_descriptor(d: &[u8; 6], sink: &mut dyn ModeSink) {
    let Some(h_active) = u16::from_le_bytes([d[1], d[2]]).checked_add(1) else {
        return;
    };
    let Some(v_active) = u16::from_le_bytes([d[3], d[4]]).checked_add(1) else {
        return;
    };
    let refresh_rate = (d[5] as u16) + 1;

    let cvt_algorithm = CvtAlgorithm::from_bits(d[0]);
    let ntsc_fractional_refresh = (d[0] >> 3) & 1 != 0;
    let stereo = decode_type_v_ix_stereo(d[0]);

    let mut mode = VideoMode::new(h_active, v_active, refresh_rate, false)
        .with_cvt_algorithm(cvt_algorithm)
        .with_ntsc_fractional_refresh(ntsc_fractional_refresh)
        .with_type_ix_stereo(stereo);

    if let Some(t) = compute_type_ix_timing(
        h_active,
        v_active,
        RefreshRate::integral(u32::from(refresh_rate)),
        cvt_algorithm,
    ) {
        mode = mode.with_detailed_timing(
            t.pixel_clock_khz,
            t.h_front_porch,
            t.h_sync_width,
            t.v_front_porch,
            t.v_sync_width,
            0,
            0,
            StereoMode::None,
            None,
        );
        if let Some(h_blank) = t.h_total.checked_sub(h_active) {
            mode = mode.with_h_blank(h_blank);
        }
    }

    sink.push_mode(mode);
}

#[cfg(test)]
#[cfg(any(feature = "alloc", feature = "std"))]
mod tests {
    use super::*;
    use crate::model::capabilities::{DisplayCapabilities, RefreshRate};

    fn make_type_iii_descriptor(
        preferred: bool,
        algo: u8,
        aspect: u8,
        h_raw: u8,
        interlaced: bool,
        refresh_raw: u8,
    ) -> [u8; 3] {
        let byte0 = ((preferred as u8) << 7) | ((algo & 0x07) << 4) | (aspect & 0x0F);
        let byte2 = ((interlaced as u8) << 7) | (refresh_raw & 0x7F);
        [byte0, h_raw, byte2]
    }

    #[test]
    fn test_type_iii_16_9_decoded() {
        // 1920×1080@60 Hz: aspect=16:9(0x04), h_raw=239→h=1920, refresh_raw=59→60 Hz
        let d = make_type_iii_descriptor(false, 0, 0x04, 239, false, 59);
        let mut caps = DisplayCapabilities::default();
        decode_type_iii_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
        assert!(!mode.interlaced);
    }

    #[test]
    fn test_type_iii_4_3_decoded() {
        // 1024×768@75 Hz: aspect=4:3(0x02), h_raw=127→h=1024, refresh_raw=74→75 Hz
        let d = make_type_iii_descriptor(false, 0, 0x02, 127, false, 74);
        let mut caps = DisplayCapabilities::default();
        decode_type_iii_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1024);
        assert_eq!(caps.supported_modes[0].height, 768);
        assert_eq!(
            caps.supported_modes[0].refresh_rate,
            Some(RefreshRate::integral(75))
        );
    }

    #[test]
    fn test_type_iii_interlaced_flag() {
        let d = make_type_iii_descriptor(false, 0, 0x04, 239, true, 59);
        let mut caps = DisplayCapabilities::default();
        decode_type_iii_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert!(caps.supported_modes[0].interlaced);
    }

    #[test]
    fn test_type_iii_undefined_aspect_skipped() {
        // Aspect code 0x08 = undefined; descriptor must be skipped.
        let d = make_type_iii_descriptor(false, 0, 0x08, 239, false, 59);
        let mut caps = DisplayCapabilities::default();
        decode_type_iii_descriptor(&d, &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_iii_non_integer_height_skipped() {
        // 16:9 requires h_active divisible by 16. h_raw=0→h=8, 8*9/16=4.5 — not integer; skip.
        let d = make_type_iii_descriptor(false, 0, 0x04, 0, false, 59);
        let mut caps = DisplayCapabilities::default();
        decode_type_iii_descriptor(&d, &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_iii_multiple_aspect_ratios() {
        let d1 = make_type_iii_descriptor(false, 0, 0x04, 239, false, 59); // 1920×1080@60
        let d2 = make_type_iii_descriptor(false, 0, 0x02, 159, false, 59); // 1280×960@60
        let d3 = make_type_iii_descriptor(false, 0, 0x05, 159, false, 59); // 1280×800@60
        let mut caps = DisplayCapabilities::default();
        decode_type_iii_descriptor(&d1, &mut caps);
        decode_type_iii_descriptor(&d2, &mut caps);
        decode_type_iii_descriptor(&d3, &mut caps);
        assert_eq!(caps.supported_modes.len(), 3);
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 1920 && m.height == 1080)
        );
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 1280 && m.height == 960)
        );
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 1280 && m.height == 800)
        );
    }

    // -----------------------------------------------------------------------
    // Type V Short Descriptor Video Timing (tag 0x11)
    // -----------------------------------------------------------------------

    /// `h_active` and `v_active` take human-readable values; the helper applies
    /// the wire-format `value − 1` encoding internally. `refresh_raw` is passed
    /// through verbatim (the decoder applies `+1` itself).
    fn make_type_v_descriptor(h_active: u16, v_active: u16, refresh_raw: u8) -> [u8; 7] {
        let mut d = [0u8; 7];
        d[1..3].copy_from_slice(&(h_active - 1).to_le_bytes());
        d[3..5].copy_from_slice(&(v_active - 1).to_le_bytes());
        d[5] = refresh_raw;
        d
    }

    #[test]
    fn test_type_v_1920x1080_at_60hz() {
        // refresh_raw = 59 → refresh = 60 Hz
        let d = make_type_v_descriptor(1920, 1080, 59);
        let mut caps = DisplayCapabilities::default();
        decode_type_v_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
        assert!(!mode.interlaced);
    }

    #[test]
    fn test_type_v_refresh_raw_255_yields_256hz() {
        // refresh_raw = 255 → raw + 1 = 256 Hz (exact per spec; no longer clamped)
        let d = make_type_v_descriptor(3840, 2160, 255);
        let mut caps = DisplayCapabilities::default();
        decode_type_v_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(
            caps.supported_modes[0].refresh_rate,
            Some(RefreshRate::integral(256))
        );
    }

    #[test]
    fn test_type_v_h_active_overflow_skipped() {
        // Raw `0xFFFF` would decode to 0x10000 (overflow on +1) — must be skipped.
        let mut d = [0u8; 7];
        d[1] = 0xFF;
        d[2] = 0xFF;
        d[3..5].copy_from_slice(&(1080u16 - 1).to_le_bytes());
        d[5] = 59;
        let mut caps = DisplayCapabilities::default();
        decode_type_v_descriptor(&d, &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_v_v_active_overflow_skipped() {
        let mut d = [0u8; 7];
        d[1..3].copy_from_slice(&(1920u16 - 1).to_le_bytes());
        d[3] = 0xFF;
        d[4] = 0xFF;
        d[5] = 59;
        let mut caps = DisplayCapabilities::default();
        decode_type_v_descriptor(&d, &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_v_minimum_active_dimensions() {
        // Wire raw 0 → decoded 1×1. Smallest representable, not skipped.
        let d = [0u8, 0, 0, 0, 0, 0, 0];
        let mut caps = DisplayCapabilities::default();
        decode_type_v_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1);
        assert_eq!(caps.supported_modes[0].height, 1);
        assert_eq!(
            caps.supported_modes[0].refresh_rate,
            Some(RefreshRate::integral(1))
        );
    }

    #[test]
    fn test_type_v_large_resolution() {
        // 7680×4320@120 Hz (8K@120): refresh_raw = 119 → 120 Hz
        let d = make_type_v_descriptor(7680, 4320, 119);
        let mut caps = DisplayCapabilities::default();
        decode_type_v_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 7680);
        assert_eq!(caps.supported_modes[0].height, 4320);
        assert_eq!(
            caps.supported_modes[0].refresh_rate,
            Some(RefreshRate::integral(120))
        );
    }

    // -----------------------------------------------------------------------
    // Type IX Formula-Based Timing (DisplayID 2.x tag 0x24)
    // -----------------------------------------------------------------------

    /// `h_active` and `v_active` take human-readable values; the helper applies
    /// the wire-format `value − 1` encoding internally. `refresh_raw` is passed
    /// through verbatim (the decoder applies `+1` itself).
    fn make_type_ix_descriptor(h_active: u16, v_active: u16, refresh_raw: u8) -> [u8; 6] {
        let mut d = [0u8; 6];
        d[1..3].copy_from_slice(&(h_active - 1).to_le_bytes());
        d[3..5].copy_from_slice(&(v_active - 1).to_le_bytes());
        d[5] = refresh_raw;
        d
    }

    #[test]
    fn test_type_ix_1920x1080_at_60hz() {
        // byte 0 = 0 → Cvt (standard CVT, no RB), no NTSC, mono stereo.
        let d = make_type_ix_descriptor(1920, 1080, 59);
        let mut caps = DisplayCapabilities::default();
        decode_type_ix_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
        assert!(!mode.interlaced);
        assert_eq!(mode.cvt_algorithm, Some(CvtAlgorithm::Cvt));
        assert!(!mode.ntsc_fractional_refresh);
        assert_eq!(mode.type_ix_stereo, Some(TypeIxStereoMode::Mono));
        assert_eq!(mode.pixel_clock_khz, None); // Cvt has no evaluator
    }

    #[test]
    fn test_type_ix_byte0_cvt_algorithm_decoded() {
        let cases = [
            (0b000, CvtAlgorithm::Cvt),
            (0b001, CvtAlgorithm::CvtRb),
            (0b010, CvtAlgorithm::CvtR2),
            (0b011, CvtAlgorithm::Reserved(3)),
            (0b100, CvtAlgorithm::Reserved(4)),
            (0b101, CvtAlgorithm::Reserved(5)),
            (0b110, CvtAlgorithm::Reserved(6)),
            (0b111, CvtAlgorithm::Reserved(7)),
        ];
        for (bits, expected) in cases {
            let mut d = make_type_ix_descriptor(1920, 1080, 59);
            d[0] = bits;
            let mut caps = DisplayCapabilities::default();
            decode_type_ix_descriptor(&d, &mut caps);
            assert_eq!(caps.supported_modes.len(), 1);
            assert_eq!(
                caps.supported_modes[0].cvt_algorithm,
                Some(expected),
                "byte 0 = {bits:#b}"
            );
        }
    }

    #[test]
    fn test_type_ix_byte0_ntsc_flag_decoded() {
        let mut d = make_type_ix_descriptor(3840, 2160, 59);
        d[0] = 0x08; // bit 3 set = NTSC fractional refresh supported
        let mut caps = DisplayCapabilities::default();
        decode_type_ix_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert!(caps.supported_modes[0].ntsc_fractional_refresh);
        assert!(!caps.supported_modes[0].y420);
    }

    #[test]
    fn test_type_ix_byte0_ntsc_off_when_bit_clear() {
        let mut d = make_type_ix_descriptor(3840, 2160, 59);
        d[0] = 0xF7; // every bit except bit 3; NTSC must remain off
        let mut caps = DisplayCapabilities::default();
        decode_type_ix_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert!(!caps.supported_modes[0].ntsc_fractional_refresh);
    }

    #[test]
    fn test_type_ix_byte0_stereo_decoded() {
        let cases = [
            (0b00 << 5, TypeIxStereoMode::Mono),
            (0b01 << 5, TypeIxStereoMode::Stereo),
            (0b10 << 5, TypeIxStereoMode::MonoOrStereoByUser),
            (0b11 << 5, TypeIxStereoMode::Reserved),
        ];
        for (bits, expected) in cases {
            let mut d = make_type_ix_descriptor(1920, 1080, 59);
            d[0] = bits;
            let mut caps = DisplayCapabilities::default();
            decode_type_ix_descriptor(&d, &mut caps);
            assert_eq!(caps.supported_modes[0].type_ix_stereo, Some(expected));
        }
    }

    #[test]
    fn test_type_ix_refresh_raw_255_yields_256hz() {
        let d = make_type_ix_descriptor(3840, 2160, 255);
        let mut caps = DisplayCapabilities::default();
        decode_type_ix_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(
            caps.supported_modes[0].refresh_rate,
            Some(RefreshRate::integral(256))
        );
    }

    #[test]
    fn test_type_ix_h_active_overflow_skipped() {
        // Raw `0xFFFF` would decode to 0x10000 (overflow on +1) — must be skipped.
        let mut d = [0u8; 6];
        d[1] = 0xFF;
        d[2] = 0xFF;
        d[3..5].copy_from_slice(&(1080u16 - 1).to_le_bytes());
        d[5] = 59;
        let mut caps = DisplayCapabilities::default();
        decode_type_ix_descriptor(&d, &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_ix_v_active_overflow_skipped() {
        let mut d = [0u8; 6];
        d[1..3].copy_from_slice(&(1920u16 - 1).to_le_bytes());
        d[3] = 0xFF;
        d[4] = 0xFF;
        d[5] = 59;
        let mut caps = DisplayCapabilities::default();
        decode_type_ix_descriptor(&d, &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_ix_cvt_rb_v1_populates_pixel_clock_and_blanking() {
        // CVT-RB v1 1920×1080@60: VESA reference clock 138.500 MHz.
        let mut d = make_type_ix_descriptor(1920, 1080, 59);
        d[0] = 0x01; // bits 2:0 = 1 → CvtRb (CVT-RB v1)
        let mut caps = DisplayCapabilities::default();
        decode_type_ix_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.cvt_algorithm, Some(CvtAlgorithm::CvtRb));
        assert_eq!(mode.pixel_clock_khz, Some(138_500));
        assert_eq!(mode.h_blank, Some(160)); // CVT-RB v1: h_total 2080
        assert_eq!(mode.h_front_porch, 48);
        assert_eq!(mode.h_sync_width, 32);
        assert_eq!(mode.v_front_porch, 3);
        assert_eq!(mode.v_sync_width, 4);
    }

    #[test]
    fn test_type_ix_cvt_r2_populates_pixel_clock_and_blanking() {
        // CVT-R2 1920×1080@60: 133.320 MHz pixel clock, 80 px H blanking.
        let mut d = make_type_ix_descriptor(1920, 1080, 59);
        d[0] = 0x02; // bits 2:0 = 2 → CvtR2
        let mut caps = DisplayCapabilities::default();
        decode_type_ix_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.cvt_algorithm, Some(CvtAlgorithm::CvtR2));
        assert_eq!(mode.pixel_clock_khz, Some(133_320));
        assert_eq!(mode.h_blank, Some(80));
        assert_eq!(mode.h_front_porch, 8);
        assert_eq!(mode.h_sync_width, 32);
        assert_eq!(mode.v_front_porch, 17);
        assert_eq!(mode.v_sync_width, 8);
    }

    #[test]
    fn test_type_ix_reserved_algorithm_leaves_timing_unpopulated() {
        let mut d = make_type_ix_descriptor(1920, 1080, 59);
        d[0] = 0x03; // bits 2:0 = 3 → Reserved(3)
        let mut caps = DisplayCapabilities::default();
        decode_type_ix_descriptor(&d, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.cvt_algorithm, Some(CvtAlgorithm::Reserved(3)));
        assert_eq!(mode.pixel_clock_khz, None);
    }
}
