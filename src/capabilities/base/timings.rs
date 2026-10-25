use crate::model::capabilities::{
    DisplayCapabilities, ModeSink, ModeSource, StereoMode, SyncDefinition, VideoMode,
};
use crate::model::diagnostics::EdidWarning;

/// Decodes the established timings bitmap (bytes 0x23–0x25, 17 predefined modes).
///
/// Each bit maps to a fixed resolution and refresh rate. Byte 0x25 bits 6–0 are
/// manufacturer-specific and are not decoded here.
pub(super) fn decode_established_timings(base: &[u8; 128], sink: &mut dyn ModeSink) {
    // (byte offset, bit mask, width, height, refresh_rate)
    // Note: 1024x768@87 is interlaced in the EDID spec; stored as-is since VideoMode
    // has no interlace field.
    const TIMINGS: &[(usize, u8, u16, u16, u16)] = &[
        (0x23, 0x80, 720, 400, 70),
        (0x23, 0x40, 720, 400, 88),
        (0x23, 0x20, 640, 480, 60),
        (0x23, 0x10, 640, 480, 67),
        (0x23, 0x08, 640, 480, 72),
        (0x23, 0x04, 640, 480, 75),
        (0x23, 0x02, 800, 600, 56),
        (0x23, 0x01, 800, 600, 60),
        (0x24, 0x80, 800, 600, 72),
        (0x24, 0x40, 800, 600, 75),
        (0x24, 0x20, 832, 624, 75),
        (0x24, 0x10, 1024, 768, 87),
        (0x24, 0x08, 1024, 768, 60),
        (0x24, 0x04, 1024, 768, 70),
        (0x24, 0x02, 1024, 768, 75),
        (0x24, 0x01, 1280, 1024, 75),
        (0x25, 0x80, 1152, 870, 75), // Apple Macintosh II
    ];

    for &(byte_off, mask, w, h, rate) in TIMINGS {
        if base[byte_off] & mask != 0 {
            sink.push_mode(VideoMode::new(w, h, rate, false));
        }
    }
}

/// Decodes a single 2-byte standard timing entry into a [`VideoMode`].
///
/// Returns `None` for unused entries (`0x01 0x01` or a zero first byte).
pub(super) fn decode_standard_timing_entry(b1: u8, b2: u8) -> Option<VideoMode> {
    if (b1 == 0x01 && b2 == 0x01) || b1 == 0x00 {
        return None;
    }
    let w = (b1 as u16 + 31) * 8;
    let h = match (b2 >> 6) & 0x03 {
        0x00 => (w * 10) / 16, // 16:10
        0x01 => (w * 3) / 4,   // 4:3
        0x02 => (w * 4) / 5,   // 5:4
        _ => (w * 9) / 16,     // 16:9
    };
    Some(VideoMode::new(w, h, (b2 & 0x3F) as u16 + 60, false))
}

/// Decodes the eight standard timing descriptors (offsets 0x26–0x35, 2 bytes each).
pub(super) fn decode_standard_timings(base: &[u8; 128], sink: &mut dyn ModeSink) {
    for i in 0..8 {
        let offset = 0x26 + (i * 2);
        if let Some(mode) = decode_standard_timing_entry(base[offset], base[offset + 1]) {
            sink.push_mode(mode);
        }
    }
}

/// Parses a single 18-byte DTD slice into a `VideoMode`.
///
/// Non-fatal findings about a decoded mode (currently
/// [`EdidWarning::DtdBordersAmbiguous`]) go to `warn`; the mode is still returned.
///
/// Returns:
/// - `Ok(None)` — monitor descriptor (zero pixel clock) or undecodable geometry
/// - `Ok(Some(mode))` — valid timing; does **not** populate `preferred_image_size_mm`
/// - `Err(w)` — [`EdidWarning::DtdSlotTooShort`] or [`EdidWarning::DtdPixelClockOverflow`]
pub(crate) fn build_dtd_mode(
    dtd: &[u8],
    warn: &mut dyn FnMut(EdidWarning),
) -> Result<Option<VideoMode>, EdidWarning> {
    if dtd.len() < 18 {
        return Err(EdidWarning::DtdSlotTooShort);
    }
    if dtd[0] == 0x00 && dtd[1] == 0x00 {
        return Ok(None); // monitor descriptor, not a DTD
    }
    let pixel_clock = ((dtd[1] as u32) << 8) | (dtd[0] as u32);
    if pixel_clock == 0 {
        return Ok(None);
    }

    let hactive = (((dtd[4] as u16) & 0xF0) << 4) | (dtd[2] as u16);
    let hblank = (((dtd[4] as u16) & 0x0F) << 8) | (dtd[3] as u16);
    let vactive = (((dtd[7] as u16) & 0xF0) << 4) | (dtd[5] as u16);
    let vblank = (((dtd[7] as u16) & 0x0F) << 8) | (dtd[6] as u16);

    if hactive == 0 || vactive == 0 || hblank == 0 || vblank == 0 {
        return Ok(None);
    }
    let total_pixels = (hactive + hblank) as u32 * (vactive + vblank) as u32;
    if total_pixels == 0 {
        return Ok(None);
    }
    let Some(refresh_rate) = pixel_clock
        .checked_mul(10_000)
        .and_then(|scaled| u16::try_from(scaled / total_pixels).ok())
    else {
        return Err(EdidWarning::DtdPixelClockOverflow);
    };

    let interlaced = dtd[17] & 0x80 != 0;

    // Sync timing (bytes 8–11).
    // H values are 10-bit; V values are 6-bit. Byte 11 holds all four MSB pairs.
    let h_front_porch = (((dtd[11] as u16) >> 6) << 8) | (dtd[8] as u16);
    let h_sync_width = ((((dtd[11] as u16) >> 4) & 0x03) << 8) | (dtd[9] as u16);
    let v_front_porch = ((((dtd[11] as u16) >> 2) & 0x03) << 4) | (((dtd[10] as u16) >> 4) & 0x0F);
    let v_sync_width = (((dtd[11] as u16) & 0x03) << 4) | ((dtd[10] as u16) & 0x0F);

    // Border (bytes 15–16): pixels/lines on each side of the active area.
    let h_border = dtd[15];
    let v_border = dtd[16];

    // EDID 1.3 §3.12 counts the borders in the blanking, EDID 1.4 §3.12 does not; EDIDs in
    // practice follow 1.3, and so does the refresh rate above (line total = active +
    // blanking). `VideoMode::h_blank` excludes the borders, so both come off.
    if h_border != 0 || v_border != 0 {
        warn(EdidWarning::DtdBordersAmbiguous { h_border, v_border });
    }
    let h_blank = hblank.checked_sub(2 * u16::from(h_border));

    // Stereo (byte 17 bits 6, 5, and 0 — non-contiguous three-bit code).
    let stereo = match ((dtd[17] >> 5) & 0x03, dtd[17] & 0x01) {
        (0b00, _) => StereoMode::None,
        (0b01, 0) => StereoMode::FieldSequentialRightFirst,
        (0b10, 0) => StereoMode::FieldSequentialLeftFirst,
        (0b01, 1) => StereoMode::TwoWayInterleavedRightEven,
        (0b10, 1) => StereoMode::TwoWayInterleavedLeftEven,
        (0b11, 0) => StereoMode::FourWayInterleaved,
        _ => StereoMode::SideBySideInterleaved,
    };

    // Sync (byte 17 bits 4–1): bit 4 = digital, bit 3 = sync subtype.
    let sync = Some(if dtd[17] & 0x10 == 0 {
        let serrations = dtd[17] & 0x04 != 0;
        let sync_on_all_rgb = dtd[17] & 0x02 != 0;
        if dtd[17] & 0x08 == 0 {
            SyncDefinition::AnalogComposite {
                serrations,
                sync_on_all_rgb,
            }
        } else {
            SyncDefinition::BipolarAnalogComposite {
                serrations,
                sync_on_all_rgb,
            }
        }
    } else {
        let h_sync_positive = dtd[17] & 0x02 != 0;
        if dtd[17] & 0x08 == 0 {
            SyncDefinition::DigitalComposite {
                serrations: dtd[17] & 0x04 != 0,
                h_sync_positive,
            }
        } else {
            SyncDefinition::DigitalSeparate {
                v_sync_positive: dtd[17] & 0x04 != 0,
                h_sync_positive,
            }
        }
    });

    let mode = VideoMode::new(hactive, vactive, refresh_rate, interlaced).with_detailed_timing(
        pixel_clock * 10,
        h_front_porch,
        h_sync_width,
        v_front_porch,
        v_sync_width,
        h_border,
        v_border,
        stereo,
        sync,
    );
    Ok(Some(match h_blank {
        Some(h_blank) => mode.with_h_blank(h_blank),
        None => mode,
    }))
}

/// Decodes a single 18-byte Detailed Timing Descriptor slice into [`DisplayCapabilities`].
///
/// Returns without modifying `caps` if the slot is a monitor descriptor (zero pixel clock)
/// or contains invalid geometry. If the mode already exists in `caps.supported_modes` it is
/// upgraded in place (preserving sync timing that may have been missing from an earlier source);
/// otherwise it is appended.
///
/// This is `pub(crate)` so that the CEA-861 handler can reuse it for DTDs in extension blocks.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(crate) fn decode_dtd_slot(dtd: &[u8], caps: &mut DisplayCapabilities, dtd_index: u8) {
    let mode = match build_dtd_mode(dtd, &mut |w| caps.push_warning(w)) {
        Err(w) => {
            caps.push_warning(w);
            return;
        }
        Ok(None) => return,
        Ok(Some(m)) => m.with_source(ModeSource::DtdIndex(dtd_index)),
    };

    // Physical image area in mm: 12-bit H from byte 12 + upper nibble of byte 14,
    // 12-bit V from byte 13 + lower nibble of byte 14. Both zero = undefined.
    // `build_dtd_mode` guarantees dtd.len() >= 18.
    let h_mm = (((dtd[14] as u16) & 0xF0) << 4) | (dtd[12] as u16);
    let v_mm = (((dtd[14] as u16) & 0x0F) << 8) | (dtd[13] as u16);
    if h_mm != 0 && v_mm != 0 && caps.preferred_image_size_mm.is_none() {
        caps.preferred_image_size_mm = Some((h_mm, v_mm));
    }

    // If a mode with matching identity (w/h/rate/interlaced) was already added from a
    // non-DTD source (which has zero sync fields), upgrade it in place. Otherwise append.
    if let Some(existing) = caps.supported_modes.iter_mut().find(|m| {
        m.width == mode.width
            && m.height == mode.height
            && m.refresh_rate == mode.refresh_rate
            && m.interlaced == mode.interlaced
    }) {
        *existing = mode;
    } else {
        caps.supported_modes.push(mode);
    }
}

/// Decodes a single 18-byte Detailed Timing Descriptor slice into a [`ModeSink`].
///
/// Like [`decode_dtd_slot`] but writes into any [`ModeSink`] rather than directly into
/// [`DisplayCapabilities`]. The `preferred_image_size_mm` field is not populated — that
/// is a base-block concern handled by [`decode_dtd_slot`].
///
/// This is `pub(crate)` so that the CEA-861 handler can use it for DTDs in extension blocks
/// in no-alloc builds.
pub(crate) fn decode_dtd_slot_into_sink(dtd: &[u8], sink: &mut dyn ModeSink, dtd_index: u8) {
    let mut found = None;
    let built = build_dtd_mode(dtd, &mut |w| found = Some(w));
    if let Some(w) = found {
        sink.push_warning(w);
    }
    match built {
        Err(w) => sink.push_warning(w),
        Ok(None) => {}
        Ok(Some(mode)) => sink.push_mode(mode.with_source(ModeSource::DtdIndex(dtd_index))),
    }
}

/// Decodes the four detailed timing descriptor (DTD) slots (offsets 0x36, 0x48, 0x5A, 0x6C).
/// Slots with a zero pixel clock are monitor descriptors and are skipped here.
// Called by capabilities_from_edid_static (step 6). The alloc BaseBlockHandler uses
// decode_dtd_slot directly to preserve preferred_image_size_mm and upgrade-in-place semantics.
#[allow(dead_code)]
pub(super) fn decode_detailed_timings(base: &[u8; 128], sink: &mut dyn ModeSink) {
    for i in 0..4u8 {
        let offset = 0x36 + (i as usize * 18);
        decode_dtd_slot_into_sink(&base[offset..offset + 18], sink, i);
    }
}

#[cfg(test)]
#[cfg(any(feature = "alloc", feature = "std"))]
mod tests {
    use crate::capabilities::base::BaseBlockHandler;
    use crate::model::capabilities::{DisplayCapabilities, RefreshRate, VideoMode};
    use crate::model::diagnostics::EdidWarning;
    use crate::model::extension::ExtensionHandler;
    use crate::model::prelude::Vec;

    #[test]
    fn test_established_timings() {
        let mut base = [0u8; 128];

        // Set 640x480@60, 800x600@60, 1024x768@60, 1280x1024@75
        base[0x23] = 0x20; // 640x480@60
        base[0x23] |= 0x01; // 800x600@60
        base[0x24] = 0x08; // 1024x768@60
        base[0x24] |= 0x01; // 1280x1024@75

        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());

        assert_eq!(caps.supported_modes.len(), 4);
        assert!(
            caps.supported_modes
                .contains(&VideoMode::new(640, 480, 60u32, false))
        );
        assert!(
            caps.supported_modes
                .contains(&VideoMode::new(800, 600, 60u32, false))
        );
        assert!(
            caps.supported_modes
                .contains(&VideoMode::new(1024, 768, 60u32, false))
        );
        assert!(
            caps.supported_modes
                .contains(&VideoMode::new(1280, 1024, 75u32, false))
        );
    }

    #[test]
    fn test_standard_timings() {
        let mut base = [0u8; 128];

        // 1920x1080 @ 60Hz: width byte = 1920/8 - 31 = 209, flags = 16:9 (3<<6) | 0Hz offset
        base[0x26] = 209;
        base[0x27] = 0xC0;

        // 1280x1024 @ 75Hz: width byte = 1280/8 - 31 = 129, flags = 5:4 (2<<6) | 15Hz offset
        base[0x28] = 129;
        base[0x29] = 0x8F;

        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());

        assert_eq!(caps.supported_modes.len(), 2);
        assert_eq!(caps.supported_modes[0].width, 1920);
        assert_eq!(caps.supported_modes[0].height, 1080);
        assert_eq!(
            caps.supported_modes[0].refresh_rate,
            Some(RefreshRate::integral(60))
        );
        assert_eq!(caps.supported_modes[1].width, 1280);
        assert_eq!(caps.supported_modes[1].height, 1024);
        assert_eq!(
            caps.supported_modes[1].refresh_rate,
            Some(RefreshRate::integral(75))
        );
    }

    #[test]
    fn test_detailed_timing_and_range_limits() {
        let mut base = [0u8; 128];

        // DTD at 0x36: 1920x1080 @ 60Hz
        // Pixel clock: 14850 (units of 10kHz = 148.50 MHz)
        base[0x36] = 0x02;
        base[0x37] = 0x3A;
        // HActive=1920 (0x780), HBlank=280 (0x118): high nibbles packed into byte 4
        base[0x38] = 0x80; // HActive LSB
        base[0x39] = 0x18; // HBlank LSB
        base[0x3A] = 0x71; // HActive high (0x7) | HBlank high (0x1)
        // VActive=1080 (0x438), VBlank=45 (0x02D): high nibbles packed into byte 7
        base[0x3B] = 0x38; // VActive LSB
        base[0x3C] = 0x2D; // VBlank LSB
        base[0x3D] = 0x40; // VActive high (0x4) | VBlank high (0x0)

        // Monitor Range Limits at 0x48
        base[0x48..0x4D].copy_from_slice(&[0x00, 0x00, 0x00, 0xFD, 0x00]);
        base[0x4D] = 48; // VMin
        base[0x4E] = 75; // VMax
        base[0x4F] = 30; // HMin (kHz)
        base[0x50] = 83; // HMax (kHz)
        base[0x51] = 17; // Max pixel clock (170 MHz)

        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());

        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1920);
        assert_eq!(caps.supported_modes[0].height, 1080);
        assert_eq!(
            caps.supported_modes[0].refresh_rate,
            Some(RefreshRate::integral(60))
        );
        assert_eq!(caps.min_v_rate, Some(48));
        assert_eq!(caps.max_v_rate, Some(75));
        assert_eq!(caps.min_h_rate_khz, Some(30));
        assert_eq!(caps.max_h_rate_khz, Some(83));
        assert_eq!(caps.max_pixel_clock_mhz, Some(170));
        // Sync timing fields (bytes 8-11)
        // 1080p timing: H front porch=88px, H sync=44px, V front porch=4 lines, V sync=5 lines
        // Encode: byte8=88, byte9=44, byte10=(4<<4)|5=0x45, byte11=0 (all MSBs zero)
    }

    #[test]
    fn test_dtd_sync_timing() {
        let mut base = [0u8; 128];

        // DTD at 0x36: pixel clock 14850 (148.50 MHz), 1920x1080 active
        base[0x36] = 0x02;
        base[0x37] = 0x3A;
        base[0x38] = 0x80;
        base[0x39] = 0x18;
        base[0x3A] = 0x71; // H
        base[0x3B] = 0x38;
        base[0x3C] = 0x2D;
        base[0x3D] = 0x40; // V
        // Sync timing:
        //   H front porch = 88 px  → byte8 = 88, byte11 bits 7-6 = 0
        //   H sync width  = 44 px  → byte9 = 44, byte11 bits 5-4 = 0
        //   V front porch = 4 lines → byte10 bits 7-4 = 4, byte11 bits 3-2 = 0
        //   V sync width  = 5 lines → byte10 bits 3-0 = 5, byte11 bits 1-0 = 0
        base[0x3E] = 88; // byte 8: H front porch LSBs
        base[0x3F] = 44; // byte 9: H sync width LSBs
        base[0x40] = (4 << 4) | 5; // byte 10: V FP nibble | V SW nibble
        base[0x41] = 0x00; // byte 11: all MSBs zero

        // Test MSB extension: set byte11 = 0b10_01_11_10
        //   H FP MSBs = 0b10 → H front porch = (2 << 8) | 88 = 600
        //   H SW MSBs = 0b01 → H sync width  = (1 << 8) | 44 = 300
        //   V FP MSBs = 0b11 → V front porch = (3 << 4) | 4  = 52
        //   V SW MSBs = 0b10 → V sync width  = (2 << 4) | 5  = 37
        // Override byte11 for this test
        base[0x41] = 0b10_01_11_10;

        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());

        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.h_front_porch, 600);
        assert_eq!(mode.h_sync_width, 300);
        assert_eq!(mode.v_front_porch, 52);
        assert_eq!(mode.v_sync_width, 37);
    }

    #[test]
    fn test_dtd_upgrades_standard_timing_entry() {
        let mut base = [0u8; 128];

        // Standard timing at 0x26: 1920x1080@60 (16:9)
        // b1 = 1920/8 - 31 = 209, b2 = (3<<6) | 0 = 0xC0
        base[0x26] = 209;
        base[0x27] = 0xC0;

        // DTD at 0x36: same 1920x1080@60 with sync detail
        base[0x36] = 0x02;
        base[0x37] = 0x3A;
        base[0x38] = 0x80;
        base[0x39] = 0x18;
        base[0x3A] = 0x71;
        base[0x3B] = 0x38;
        base[0x3C] = 0x2D;
        base[0x3D] = 0x40;
        base[0x3E] = 88;
        base[0x3F] = 44;
        base[0x40] = (4 << 4) | 5;
        base[0x41] = 0x00;

        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());

        // Should be only one entry — the DTD upgraded the standard timing entry
        assert_eq!(
            caps.supported_modes
                .iter()
                .filter(|m| m.width == 1920
                    && m.height == 1080
                    && m.refresh_rate == Some(RefreshRate::integral(60)))
                .count(),
            1
        );
        let mode = caps
            .supported_modes
            .iter()
            .find(|m| m.width == 1920 && m.height == 1080)
            .unwrap();
        assert_eq!(mode.h_front_porch, 88);
        assert_eq!(mode.v_front_porch, 4);
    }

    #[test]
    fn test_dtd_interlace_and_image_size() {
        let mut base = [0u8; 128];

        // DTD at 0x36: 1920x1080i @ 60Hz (interlaced) with image size 527x296 mm
        // Pixel clock 7425 × 10kHz = 74.25 MHz (halved for interlace field rate)
        base[0x36] = 0x29; // pixel clock LSB (0x1D11 >> 8 = ... actually let's just set blanking)
        base[0x37] = 0x1D; // pixel clock = 0x1D29 = 7465
        // HActive=1920 (0x780), HBlank=280 (0x118)
        base[0x38] = 0x80;
        base[0x39] = 0x18;
        base[0x3A] = 0x71;
        // VActive=540 (0x21C) (field height for 1080i), VBlank=22 (0x016)
        base[0x3B] = 0x1C;
        base[0x3C] = 0x16;
        base[0x3D] = 0x20;
        // Sync/border bytes (arbitrary non-zero)
        base[0x3E] = 0x00;
        base[0x3F] = 0x00;
        base[0x40] = 0x00;
        base[0x41] = 0x00;
        // Image size: H=527mm (0x20F), V=296mm (0x128)
        // byte 12 = H LSB = 0x0F
        // byte 13 = V LSB = 0x28
        // byte 14 = (H MSN << 4) | V MSN = (0x2 << 4) | 0x1 = 0x21
        base[0x42] = 0x0F; // H image size LSB
        base[0x43] = 0x28; // V image size LSB
        base[0x44] = 0x21; // H MSN=2, V MSN=1
        // Border
        base[0x45] = 0x00;
        base[0x46] = 0x00;
        // Byte 17: bit 7 = interlaced
        base[0x47] = 0x80;

        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());

        assert_eq!(caps.supported_modes.len(), 1);
        assert!(caps.supported_modes[0].interlaced);
        assert_eq!(caps.preferred_image_size_mm, Some((527, 296)));
    }

    #[test]
    fn test_dtd_progressive_no_image_size() {
        let mut base = [0u8; 128];

        // DTD at 0x36: 1920x1080 @ ~60Hz, progressive, image size bytes zeroed
        base[0x36] = 0x02;
        base[0x37] = 0x3A;
        base[0x38] = 0x80;
        base[0x39] = 0x18;
        base[0x3A] = 0x71;
        base[0x3B] = 0x38;
        base[0x3C] = 0x2D;
        base[0x3D] = 0x40;
        // bytes 12-14: all zero → no image size
        // byte 17: 0 → progressive
        base[0x47] = 0x00;

        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());

        assert!(!caps.supported_modes[0].interlaced);
        assert_eq!(caps.preferred_image_size_mm, None);
    }

    // Helper: build a minimal valid 1920x1080@60 DTD with byte 17 set to `flags`.
    #[cfg(any(feature = "alloc", feature = "std"))]
    fn dtd_with_byte17(flags: u8) -> VideoMode {
        let mut base = [0u8; 128];
        base[0x36] = 0x02;
        base[0x37] = 0x3A;
        base[0x38] = 0x80;
        base[0x39] = 0x18;
        base[0x3A] = 0x71;
        base[0x3B] = 0x38;
        base[0x3C] = 0x2D;
        base[0x3D] = 0x40;
        base[0x45] = 3; // h_border
        base[0x46] = 2; // v_border
        base[0x47] = flags;
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[&base], &mut caps, &mut Vec::new());
        caps.supported_modes.into_iter().next().unwrap()
    }

    /// A base block with one DTD: 1920×1080, 148.50 MHz, blanking 280 × 45, and the given
    /// borders.
    fn base_with_bordered_1080p(h_border: u8, v_border: u8) -> [u8; 128] {
        let mut base = [0u8; 128];
        base[0x36] = 0x02;
        base[0x37] = 0x3A; // 14850 × 10 kHz
        base[0x38] = 0x80; // H active LSB (1920)
        base[0x39] = 0x18; // H blank LSB (280)
        base[0x3A] = 0x71; // H active MSB 7, H blank MSB 1
        base[0x3B] = 0x38; // V active LSB (1080)
        base[0x3C] = 0x2D; // V blank LSB (45)
        base[0x3D] = 0x40; // V active MSB 4, V blank MSB 0
        base[0x3E] = 88;
        base[0x3F] = 44;
        base[0x40] = (4 << 4) | 5;
        base[0x45] = h_border;
        base[0x46] = v_border;
        base[0x47] = 0x1E;
        base
    }

    fn decode_base(base: &[u8; 128]) -> DisplayCapabilities {
        let mut caps = DisplayCapabilities::default();
        BaseBlockHandler.process(&[base], &mut caps, &mut Vec::new());
        caps
    }

    fn borders_warnings(caps: &DisplayCapabilities) -> Vec<EdidWarning> {
        caps.iter_warnings()
            .filter_map(|w| (**w).downcast_ref::<EdidWarning>().copied())
            .filter(|w| matches!(w, EdidWarning::DtdBordersAmbiguous { .. }))
            .collect()
    }

    #[test]
    fn test_dtd_h_blank_without_borders() {
        let caps = decode_base(&base_with_bordered_1080p(0, 0));
        assert_eq!(caps.supported_modes[0].h_blank, Some(280));
        assert!(borders_warnings(&caps).is_empty());
    }

    #[test]
    fn test_dtd_h_blank_with_borders_reads_edid_1_3() {
        // EDID 1.3: the 280 of blanking include 8 + 8 pixels of border.
        let caps = decode_base(&base_with_bordered_1080p(8, 2));
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.h_blank, Some(264));
        assert_eq!(mode.h_border, 8);
        // The refresh rate takes the same reading: 148.5 MHz / (2200 × 1125) = 60 Hz.
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
        assert_eq!(
            borders_warnings(&caps),
            [EdidWarning::DtdBordersAmbiguous {
                h_border: 8,
                v_border: 2
            }]
        );
    }

    #[test]
    fn test_dtd_borders_wider_than_blanking_leave_h_blank_unset() {
        // 2 × 150 > 280: no reading fits.
        let caps = decode_base(&base_with_bordered_1080p(150, 0));
        assert_eq!(caps.supported_modes[0].h_blank, None);
        assert_eq!(borders_warnings(&caps).len(), 1);
    }

    #[test]
    fn test_dtd_border() {
        let mode = dtd_with_byte17(0x1E); // progressive, digital separate, positive/positive
        assert_eq!(mode.h_border, 3);
        assert_eq!(mode.v_border, 2);
    }

    #[test]
    fn test_dtd_stereo() {
        use crate::model::capabilities::StereoMode;
        // bits 6-5 = 00: no stereo
        assert_eq!(dtd_with_byte17(0x1E).stereo, StereoMode::None);
        // bits 6-5 = 01, bit 0 = 0: field sequential right first
        assert_eq!(
            dtd_with_byte17(0x3E).stereo,
            StereoMode::FieldSequentialRightFirst
        );
        // bits 6-5 = 10, bit 0 = 0: field sequential left first
        assert_eq!(
            dtd_with_byte17(0x5E).stereo,
            StereoMode::FieldSequentialLeftFirst
        );
        // bits 6-5 = 01, bit 0 = 1: 2-way interleaved right even
        assert_eq!(
            dtd_with_byte17(0x3F).stereo,
            StereoMode::TwoWayInterleavedRightEven
        );
        // bits 6-5 = 10, bit 0 = 1: 2-way interleaved left even
        assert_eq!(
            dtd_with_byte17(0x5F).stereo,
            StereoMode::TwoWayInterleavedLeftEven
        );
        // bits 6-5 = 11, bit 0 = 0: 4-way interleaved
        assert_eq!(dtd_with_byte17(0x7E).stereo, StereoMode::FourWayInterleaved);
        // bits 6-5 = 11, bit 0 = 1: side-by-side
        assert_eq!(
            dtd_with_byte17(0x7F).stereo,
            StereoMode::SideBySideInterleaved
        );
    }

    #[test]
    fn test_dtd_sync_types() {
        use crate::model::capabilities::SyncDefinition;
        // Analog composite, no serrations, sync on green: bits 4-1 = 0b0000
        assert_eq!(
            dtd_with_byte17(0x00).sync,
            Some(SyncDefinition::AnalogComposite {
                serrations: false,
                sync_on_all_rgb: false
            })
        );
        // Analog composite, serrations, sync on all RGB: bits 4-1 = 0b0011
        assert_eq!(
            dtd_with_byte17(0x06).sync,
            Some(SyncDefinition::AnalogComposite {
                serrations: true,
                sync_on_all_rgb: true
            })
        );
        // Bipolar analog composite, no serrations, sync on all RGB: bits 4-1 = 0b0101 (0x0A)
        assert_eq!(
            dtd_with_byte17(0x0A).sync,
            Some(SyncDefinition::BipolarAnalogComposite {
                serrations: false,
                sync_on_all_rgb: true
            })
        );
        // Digital composite, no serrations, H-sync negative: bits 4-1 = 0b1000
        assert_eq!(
            dtd_with_byte17(0x10).sync,
            Some(SyncDefinition::DigitalComposite {
                serrations: false,
                h_sync_positive: false
            })
        );
        // Digital composite, serrations, H-sync positive: bits 4-1 = 0b1011
        assert_eq!(
            dtd_with_byte17(0x16).sync,
            Some(SyncDefinition::DigitalComposite {
                serrations: true,
                h_sync_positive: true
            })
        );
        // Digital separate, V-sync negative, H-sync negative: bits 4-1 = 0b1100
        assert_eq!(
            dtd_with_byte17(0x18).sync,
            Some(SyncDefinition::DigitalSeparate {
                v_sync_positive: false,
                h_sync_positive: false
            })
        );
        // Digital separate, V-sync positive, H-sync positive: bits 4-1 = 0b1111
        assert_eq!(
            dtd_with_byte17(0x1E).sync,
            Some(SyncDefinition::DigitalSeparate {
                v_sync_positive: true,
                h_sync_positive: true
            })
        );
    }
}
