use crate::capabilities::cea861::{dmt_to_mode, vic_to_mode};
use crate::model::capabilities::{ModeSink, VideoMode};

/// Resolves a `(code_type, code)` pair against the DMT, VIC, or HDMI VIC tables.
///
/// `code_type` follows the DisplayID Type IV / Type VIII encoding (revision byte bits 7:6).
/// Codes outside the addressable range of the chosen table (e.g. > 255 for VIC/HDMI VIC)
/// are treated as unknown and return `None`.
fn resolve_coded_timing(code_type: u8, code: u16) -> Option<VideoMode> {
    match code_type {
        0 => dmt_to_mode(code),
        1 => u8::try_from(code).ok().and_then(vic_to_mode),
        2 => u8::try_from(code).ok().and_then(hdmi_vic_to_mode),
        _ => None,
    }
}

/// Decodes a Type IV DMT/VIC Code Block payload and pushes resolved modes to `sink`.
///
/// The code space is determined by the revision byte's upper 2 bits, passed in as `code_type`:
/// - `0` — VESA DMT IDs (1-byte each); resolved via the DMT table
/// - `1` — CTA-861 VIC codes (1-byte each); resolved via the VIC table
/// - `2` — HDMI VIC codes (1-byte each); 4 codes defined: 1–4
/// - `3` — reserved; descriptors silently skipped
///
/// Codes not found in the respective table are silently skipped.
pub(super) fn decode_type_iv_block(payload: &[u8], code_type: u8, sink: &mut dyn ModeSink) {
    for &code in payload {
        if let Some(m) = resolve_coded_timing(code_type, code as u16) {
            sink.push_mode(m);
        }
    }
}

/// Decodes a Type VIII Enumerated Timing Code block payload and pushes resolved modes to `sink`.
///
/// The block's revision byte selects the code space and the per-code byte width:
/// - bits 7:6 = code type (`0` DMT, `1` VIC, `2` HDMI VIC, `3` reserved)
/// - bit 3    = `TCS` (Two-byte Code Support): `0` = 1-byte codes, `1` = 2-byte little-endian codes
///
/// Codes outside the addressable range of the chosen table or absent from the lookup are
/// silently skipped. A trailing odd byte in 2-byte mode is ignored.
pub(super) fn decode_type_viii_block(payload: &[u8], revision: u8, sink: &mut dyn ModeSink) {
    let code_type = (revision >> 6) & 0x03;
    let two_byte = (revision >> 3) & 1 != 0;
    let stride = if two_byte { 2 } else { 1 };

    let mut i = 0;
    while i + stride <= payload.len() {
        let code = if two_byte {
            u16::from_le_bytes([payload[i], payload[i + 1]])
        } else {
            payload[i] as u16
        };
        if let Some(m) = resolve_coded_timing(code_type, code) {
            sink.push_mode(m);
        }
        i += stride;
    }
}

/// Returns the `VideoMode` for an HDMI VIC code (1–4), or `None`.
///
/// HDMI 1.4 defines four extended resolution codes for 4K/UHD modes, each the same timing
/// as a CTA-861 VIC, from which the full timing (pixel clock, porches, sync, `h_blank`) is
/// taken:
/// - 1: 3840×2160@30 Hz (CTA VIC 95)
/// - 2: 3840×2160@25 Hz (CTA VIC 94)
/// - 3: 3840×2160@24 Hz (CTA VIC 93)
/// - 4: 4096×2160@24 Hz (CTA VIC 98)
///
/// The mode's `source` is left `None`: it is not the CTA VIC it borrows the timing from.
fn hdmi_vic_to_mode(code: u8) -> Option<VideoMode> {
    let cta_vic = match code {
        1 => 95,
        2 => 94,
        3 => 93,
        4 => 98,
        _ => return None,
    };
    let mut mode = vic_to_mode(cta_vic)?;
    mode.source = None;
    Some(mode)
}

/// Iterates the VESA Video Timing Block presence bitmap and pushes supported modes to `sink`.
///
/// The payload is a bitmap of up to 10 bytes (80 bits). Bit `i` (0-indexed, LSB-first within
/// each byte) corresponds to DMT ID `i + 1`. A set bit means the display supports that mode.
/// Bytes beyond the first 10 are ignored; unknown or unset bits produce no output.
///
/// Source: edid-decode (timvideos/edid-decode); confirmed against `DATA_BLOCK_VESA_TIMING`
/// in the Linux kernel `drivers/gpu/drm/drm_displayid_internal.h`.
pub(super) fn decode_vesa_video_timing_block(payload: &[u8], sink: &mut dyn ModeSink) {
    for (byte_idx, &byte) in payload.iter().take(10).enumerate() {
        for bit in 0u16..8 {
            if byte & (1 << bit) != 0 {
                let dmt_id = (byte_idx as u16) * 8 + bit + 1;
                if let Some(mode) = dmt_to_mode(dmt_id) {
                    sink.push_mode(mode);
                }
            }
        }
    }
}

/// Iterates the CTA-861 Video Timing Block presence bitmap and pushes supported modes to `sink`.
///
/// The payload is a bitmap of up to 8 bytes (64 bits). Bit `i` (0-indexed, LSB-first within
/// each byte) corresponds to VIC `i + 1`. A set bit means the display supports that mode.
/// Bytes beyond the first 8 are ignored; unset bits and unrecognised VICs produce no output.
///
/// Source: edid-decode (swick/edid-decode), `parse-displayid-block.cpp`.
pub(super) fn decode_cta_video_timing_block(payload: &[u8], sink: &mut dyn ModeSink) {
    for (byte_idx, &byte) in payload.iter().take(8).enumerate() {
        for bit in 0u8..8 {
            if byte & (1 << bit) != 0 {
                let vic = (byte_idx as u8) * 8 + bit + 1;
                if let Some(mode) = vic_to_mode(vic) {
                    sink.push_mode(mode);
                }
            }
        }
    }
}

#[cfg(test)]
#[cfg(any(feature = "alloc", feature = "std"))]
mod tests {
    use super::*;
    use crate::model::capabilities::{DisplayCapabilities, RefreshRate, SyncDefinition};

    // -----------------------------------------------------------------------
    // Type IV DMT/VIC Code Block (tag 0x06)
    // -----------------------------------------------------------------------

    #[test]
    fn test_type_iv_dmt_decoded() {
        // DMT 0x52 = 1920×1080@60 Hz; verify full timing detail from the DMT table.
        let mut caps = DisplayCapabilities::default();
        decode_type_iv_block(&[0x52], 0, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
        assert!(!mode.interlaced);
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
    fn test_type_iv_dmt_interlaced() {
        // DMT 0x0F = 1024×768i@43 Hz (interlaced)
        let mut caps = DisplayCapabilities::default();
        decode_type_iv_block(&[0x0F], 0, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 1024);
        assert_eq!(mode.height, 768);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(43)));
        assert!(mode.interlaced);
    }

    #[test]
    fn test_type_iv_vic_decoded() {
        // VIC 1 = 640×480@60 Hz
        let mut caps = DisplayCapabilities::default();
        decode_type_iv_block(&[1], 1, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 640);
        assert_eq!(caps.supported_modes[0].height, 480);
        assert_eq!(
            caps.supported_modes[0].refresh_rate,
            Some(RefreshRate::integral(60))
        );
    }

    #[test]
    fn test_type_iv_hdmi_vic_decoded() {
        // HDMI VIC 1 = 3840×2160@30 Hz
        let mut caps = DisplayCapabilities::default();
        decode_type_iv_block(&[1], 2, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 3840);
        assert_eq!(caps.supported_modes[0].height, 2160);
        assert_eq!(
            caps.supported_modes[0].refresh_rate,
            Some(RefreshRate::integral(30))
        );
        // The full timing of CTA VIC 95, without claiming to be it.
        assert_eq!(caps.supported_modes[0].pixel_clock_khz, Some(297_000));
        assert_eq!(caps.supported_modes[0].h_blank, Some(560));
        assert_eq!(caps.supported_modes[0].source, None);
    }

    #[test]
    fn test_hdmi_vics_take_their_cta_timing() {
        for (hdmi_vic, cta_vic) in [(1, 95), (2, 94), (3, 93), (4, 98)] {
            let mode = hdmi_vic_to_mode(hdmi_vic).unwrap();
            let cta = vic_to_mode(cta_vic).unwrap();
            assert_eq!(
                (
                    mode.width,
                    mode.height,
                    mode.refresh_rate,
                    mode.pixel_clock_khz,
                    mode.h_blank
                ),
                (
                    cta.width,
                    cta.height,
                    cta.refresh_rate,
                    cta.pixel_clock_khz,
                    cta.h_blank
                ),
                "HDMI VIC {hdmi_vic}"
            );
        }
        assert!(hdmi_vic_to_mode(0).is_none());
        assert!(hdmi_vic_to_mode(5).is_none());
    }

    #[test]
    fn test_type_iv_unknown_dmt_skipped() {
        let mut caps = DisplayCapabilities::default();
        decode_type_iv_block(&[0xFF], 0, &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_iv_multiple_codes() {
        let mut caps = DisplayCapabilities::default();
        decode_type_iv_block(&[0x10, 0x23, 0x52], 0, &mut caps);
        assert_eq!(caps.supported_modes.len(), 3);
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 1024 && m.height == 768)
        );
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 1280 && m.height == 1024)
        );
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 1920 && m.height == 1080)
        );
    }

    #[test]
    fn test_type_iv_mixed_known_unknown_dmt_codes() {
        // Unknown codes sandwiched around a known one — only the known one yields a mode.
        let mut caps = DisplayCapabilities::default();
        decode_type_iv_block(&[0xFE, 0x23, 0xFF], 0, &mut caps); // 0x23 = 1280×1024@60
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1280);
        assert_eq!(caps.supported_modes[0].height, 1024);
        assert_eq!(
            caps.supported_modes[0].refresh_rate,
            Some(RefreshRate::integral(60))
        );
    }

    #[test]
    fn test_type_iv_reserved_code_type_skipped() {
        // Code type 3 is reserved; no mode should be produced.
        let mut caps = DisplayCapabilities::default();
        decode_type_iv_block(&[0x52], 3, &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    // -----------------------------------------------------------------------
    // VESA Video Timing Block (tag 0x07)
    // -----------------------------------------------------------------------

    #[test]
    fn test_vesa_timing_single_bit_set() {
        // Bitmap byte 0, bit 3 (0-indexed) = DMT ID 0x04 = 640×480@60 Hz.
        let mut caps = DisplayCapabilities::default();
        decode_vesa_video_timing_block(&[0b0000_1000], &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 640);
        assert_eq!(mode.height, 480);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
        assert_eq!(mode.h_front_porch, 16);
        assert_eq!(mode.h_sync_width, 96);
        assert_eq!(mode.v_front_porch, 10);
        assert_eq!(mode.v_sync_width, 2);
        assert_eq!(
            mode.sync,
            Some(SyncDefinition::DigitalSeparate {
                h_sync_positive: false,
                v_sync_positive: false,
            })
        );
    }

    #[test]
    fn test_vesa_timing_multiple_modes() {
        // Byte 0 bit 0 = DMT 0x01 (640×350@85), byte 1 bit 0 = DMT 0x09 (800×600@60).
        let mut caps = DisplayCapabilities::default();
        decode_vesa_video_timing_block(&[0x01, 0x01], &mut caps);
        assert_eq!(caps.supported_modes.len(), 2);
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 640 && m.height == 350)
        );
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 800 && m.height == 600)
        );
    }

    #[test]
    fn test_vesa_timing_empty_payload() {
        let mut caps = DisplayCapabilities::default();
        decode_vesa_video_timing_block(&[], &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_vesa_timing_payload_truncated_at_10_bytes() {
        // 11-byte payload; the 11th byte has bits set but must be ignored.
        let mut bitmap = vec![0u8; 11];
        bitmap[10] = 0xFF; // outside the 10-byte window
        bitmap[0] = 0x01; // DMT 0x01 = 640×350@85 — should be decoded
        let mut caps = DisplayCapabilities::default();
        decode_vesa_video_timing_block(&bitmap, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 640);
        assert_eq!(caps.supported_modes[0].height, 350);
    }

    // -----------------------------------------------------------------------
    // CTA-861 Video Timing Block (tag 0x08)
    // -----------------------------------------------------------------------

    #[test]
    fn test_cta_timing_single_bit_set() {
        // Byte 0 bit 0 = VIC 1 = 640×480@60 Hz.
        let mut caps = DisplayCapabilities::default();
        decode_cta_video_timing_block(&[0x01], &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 640);
        assert_eq!(mode.height, 480);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
        assert!(mode.h_front_porch != 0 || mode.h_sync_width != 0);
    }

    #[test]
    fn test_cta_timing_multiple_modes() {
        // Byte 0 = 0x03 → bits 0 and 1 set → VIC 1 (640×480@60) and VIC 2 (480p@60).
        let mut caps = DisplayCapabilities::default();
        decode_cta_video_timing_block(&[0x03], &mut caps);
        assert_eq!(caps.supported_modes.len(), 2);
    }

    #[test]
    fn test_cta_timing_empty_payload() {
        let mut caps = DisplayCapabilities::default();
        decode_cta_video_timing_block(&[], &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_cta_timing_payload_truncated_at_8_bytes() {
        // 9-byte payload; the 9th byte is outside the 8-byte window and must be ignored.
        let mut bitmap = vec![0u8; 9];
        bitmap[8] = 0xFF; // outside the limit
        bitmap[0] = 0x01; // VIC 1 — should be decoded
        let mut caps = DisplayCapabilities::default();
        decode_cta_video_timing_block(&bitmap, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 640);
    }

    // -----------------------------------------------------------------------
    // Type VIII Enumerated Timing Code (DisplayID 2.x tag 0x23)
    // -----------------------------------------------------------------------

    #[test]
    fn test_type_viii_dmt_one_byte() {
        // revision: code_type=DMT(0), TCS=0 (1-byte codes); payload [0x52] → 1920×1080@60.
        let mut caps = DisplayCapabilities::default();
        decode_type_viii_block(&[0x52], 0x00, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1920);
        assert_eq!(caps.supported_modes[0].height, 1080);
    }

    #[test]
    fn test_type_viii_dmt_two_byte() {
        // revision bit 3 = TCS=1; same DMT code expressed as little-endian u16.
        let mut caps = DisplayCapabilities::default();
        decode_type_viii_block(&[0x52, 0x00], 0x08, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1920);
        assert_eq!(caps.supported_modes[0].height, 1080);
    }

    #[test]
    fn test_type_viii_vic_one_byte() {
        // code_type=VIC(1); VIC 1 = 640×480@60.
        let mut caps = DisplayCapabilities::default();
        decode_type_viii_block(&[0x01], 0x40, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 640);
        assert_eq!(caps.supported_modes[0].height, 480);
    }

    #[test]
    fn test_type_viii_hdmi_vic() {
        // code_type=HDMI VIC(2); HDMI VIC 1 = 3840×2160@30.
        let mut caps = DisplayCapabilities::default();
        decode_type_viii_block(&[0x01], 0x80, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 3840);
        assert_eq!(
            caps.supported_modes[0].refresh_rate,
            Some(RefreshRate::integral(30))
        );
    }

    #[test]
    fn test_type_viii_reserved_code_type_skipped() {
        // code_type=3 reserved; produces no mode.
        let mut caps = DisplayCapabilities::default();
        decode_type_viii_block(&[0x52], 0xC0, &mut caps);
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_viii_two_byte_vic_high_byte_nonzero_skipped() {
        // VIC space is u8 — a 2-byte code with high byte set is unaddressable; skip it.
        let mut caps = DisplayCapabilities::default();
        decode_type_viii_block(&[0x01, 0x01], 0x48, &mut caps); // code=0x0101=257
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_viii_two_byte_trailing_odd_byte_ignored() {
        // 3-byte payload in 2-byte mode: only the first pair decodes.
        let mut caps = DisplayCapabilities::default();
        decode_type_viii_block(&[0x52, 0x00, 0x01], 0x08, &mut caps);
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1920);
    }

    #[test]
    fn test_type_viii_multiple_one_byte_codes() {
        let mut caps = DisplayCapabilities::default();
        decode_type_viii_block(&[0x10, 0x23, 0x52], 0x00, &mut caps);
        assert_eq!(caps.supported_modes.len(), 3);
    }

    #[test]
    fn test_type_viii_unknown_dmt_skipped() {
        let mut caps = DisplayCapabilities::default();
        decode_type_viii_block(&[0xFF, 0x00], 0x08, &mut caps); // unknown DMT 0xFF
        assert!(caps.supported_modes.is_empty());
    }
}
