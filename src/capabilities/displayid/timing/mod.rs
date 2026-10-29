mod coded;
mod detailed;
mod short;

use super::{
    DISPLAYID_V2, TAG_CTA_VIDEO_TIMING, TAG_TYPE_I_TIMING, TAG_TYPE_II_TIMING, TAG_TYPE_III_TIMING,
    TAG_TYPE_IV_TIMING, TAG_TYPE_V_TIMING, TAG_TYPE_VI_TIMING, TAG_V2_CTA_DISPLAYID,
    TAG_V2_TYPE_IX_TIMING, TAG_V2_TYPE_VII_TIMING, TAG_V2_TYPE_VIII_TIMING, TAG_VESA_VIDEO_TIMING,
    for_each_data_block,
};
use crate::capabilities::cea861::cea861_collection_into_sink;
use crate::model::capabilities::ModeSink;
use crate::model::options::DecodeOptions;
use coded::{
    decode_cta_video_timing_block, decode_type_iv_block, decode_type_viii_block,
    decode_vesa_video_timing_block,
};
use detailed::{
    decode_type_i_descriptor, decode_type_ii_descriptor, decode_type_vi_descriptor,
    decode_type_vii_descriptor,
};
use short::{decode_type_iii_descriptor, decode_type_ix_descriptor, decode_type_v_descriptor};

pub(crate) use detailed::decode_type_vii_descriptor_to_mode;

/// Iterates DisplayID data blocks within a fragment's payload region and pushes
/// decoded modes to `sink`.
///
/// `payload` must be the data-block region: bytes `block[4..4+section_byte_count]`,
/// clamped to `block[4..127]` to exclude the checksum byte.
///
/// `version` is the DisplayID version byte from the section header. V1 timing decoders
/// run only when `version` falls in the 1.x range; V2 sections do not yet decode any
/// timing blocks.
pub(super) fn process_data_blocks(
    payload: &[u8],
    version: u8,
    sink: &mut dyn ModeSink,
    options: &DecodeOptions,
) {
    let is_v2 = version == DISPLAYID_V2;
    for_each_data_block(payload, |tag, revision, block_payload| {
        if is_v2 {
            match tag {
                TAG_V2_TYPE_VII_TIMING => {
                    let mut i = 0;
                    while i + 20 <= block_payload.len() {
                        let descriptor: &[u8; 20] = block_payload[i..i + 20]
                            .try_into()
                            .expect("slice length guaranteed by loop condition");
                        decode_type_vii_descriptor(descriptor, revision, sink);
                        i += 20;
                    }
                }
                TAG_V2_TYPE_VIII_TIMING => {
                    decode_type_viii_block(block_payload, revision, sink);
                }
                TAG_V2_TYPE_IX_TIMING => {
                    let mut i = 0;
                    while i + 6 <= block_payload.len() {
                        let descriptor: &[u8; 6] = block_payload[i..i + 6]
                            .try_into()
                            .expect("slice length guaranteed by loop condition");
                        decode_type_ix_descriptor(descriptor, sink);
                        i += 6;
                    }
                }
                TAG_V2_CTA_DISPLAYID => {
                    // CTA DisplayID Block — payload is a CTA-861 data block collection.
                    // Mode-producing entries (VICs, VTB-EXT, T7/T8/T10VTDB, Y420) are
                    // emitted; CTA metadata is dropped on the static path.
                    cea861_collection_into_sink(block_payload, sink, options);
                }
                _ => {}
            }
            return;
        }
        if tag == TAG_TYPE_I_TIMING {
            let mut i = 0;
            while i + 20 <= block_payload.len() {
                let descriptor: &[u8; 20] = block_payload[i..i + 20]
                    .try_into()
                    .expect("slice length guaranteed by loop condition");
                decode_type_i_descriptor(descriptor, sink);
                i += 20;
            }
        } else if tag == TAG_TYPE_II_TIMING {
            let mut i = 0;
            while i + 11 <= block_payload.len() {
                let descriptor: &[u8; 11] = block_payload[i..i + 11]
                    .try_into()
                    .expect("slice length guaranteed by loop condition");
                decode_type_ii_descriptor(descriptor, sink);
                i += 11;
            }
        } else if tag == TAG_TYPE_III_TIMING {
            let mut i = 0;
            while i + 3 <= block_payload.len() {
                let descriptor: &[u8; 3] = block_payload[i..i + 3]
                    .try_into()
                    .expect("slice length guaranteed by loop condition");
                decode_type_iii_descriptor(descriptor, sink);
                i += 3;
            }
        } else if tag == TAG_TYPE_IV_TIMING {
            // Revision byte bits 7:6 select the code space (0=DMT, 1=VIC, 2=HDMI VIC).
            let code_type = (revision >> 6) & 0x03;
            decode_type_iv_block(block_payload, code_type, sink);
        } else if tag == TAG_VESA_VIDEO_TIMING {
            decode_vesa_video_timing_block(block_payload, sink);
        } else if tag == TAG_CTA_VIDEO_TIMING {
            decode_cta_video_timing_block(block_payload, sink);
        } else if tag == TAG_TYPE_V_TIMING {
            let mut i = 0;
            while i + 7 <= block_payload.len() {
                let descriptor: &[u8; 7] = block_payload[i..i + 7]
                    .try_into()
                    .expect("slice length guaranteed by loop condition");
                decode_type_v_descriptor(descriptor, sink);
                i += 7;
            }
        } else if tag == TAG_TYPE_VI_TIMING {
            let mut i = 0;
            while i + 14 <= block_payload.len() {
                let consumed = decode_type_vi_descriptor(&block_payload[i..], sink);
                if consumed == 0 {
                    break;
                }
                i += consumed;
            }
        }
        // Unknown block tags are silently skipped.
    });
}

#[cfg(test)]
#[cfg(any(feature = "alloc", feature = "std"))]
mod tests {
    use super::*;
    use crate::model::capabilities::{
        DisplayCapabilities, RefreshRate, StaticContext, StaticDisplayCapabilities,
    };

    #[allow(clippy::too_many_arguments)] // mirrors the wire-format field list
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

    // -----------------------------------------------------------------------
    // Dispatcher behaviour
    // -----------------------------------------------------------------------

    #[test]
    fn test_multiple_descriptors_in_block() {
        let desc1 = make_type_i_descriptor(14850, 1920, 280, 88, 44, 1080, 45, 4, 5, 0x00);
        let desc2 = make_type_i_descriptor(22118, 2560, 440, 80, 32, 1440, 41, 4, 5, 0x00);
        let mut payload = vec![TAG_TYPE_I_TIMING, 0x00, 40];
        payload.extend_from_slice(&desc1);
        payload.extend_from_slice(&desc2);
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, 0x10, &mut caps, &DecodeOptions::new());
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

    #[test]
    fn test_malformed_data_block_stops_iteration() {
        // A data block that claims a length extending past the payload boundary.
        let payload = [TAG_TYPE_I_TIMING, 0x00, 50]; // claims 50 bytes; 0 remain
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, 0x10, &mut caps, &DecodeOptions::new());
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_eos_sentinel_stops_iteration() {
        // A valid Type I descriptor followed by an EOS sentinel; a second descriptor must not decode.
        let desc = make_type_i_descriptor(14850, 1920, 280, 88, 44, 1080, 45, 4, 5, 0x00);
        let desc2 = make_type_i_descriptor(22118, 2560, 440, 80, 32, 1440, 41, 4, 5, 0x00);
        let mut payload = vec![TAG_TYPE_I_TIMING, 0x00, 20];
        payload.extend_from_slice(&desc);
        payload.extend_from_slice(&[0x00, 0x00, 0x00]); // EOS sentinel
        payload.extend_from_slice(&[TAG_TYPE_I_TIMING, 0x00, 20]);
        payload.extend_from_slice(&desc2);
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, 0x10, &mut caps, &DecodeOptions::new());
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1920);
    }

    #[test]
    fn test_unknown_data_block_tag_skipped() {
        // An unknown data block (tag 0xFF, 10-byte payload) followed by a valid Type I block.
        let desc = make_type_i_descriptor(14850, 1920, 280, 88, 44, 1080, 45, 4, 5, 0x00);
        let mut payload = vec![0xFFu8, 0x00, 10];
        payload.extend_from_slice(&[0u8; 10]);
        payload.extend_from_slice(&[TAG_TYPE_I_TIMING, 0x00, 20]);
        payload.extend_from_slice(&desc);
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, 0x10, &mut caps, &DecodeOptions::new());
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1920);
    }

    #[test]
    fn test_partial_descriptor_ignored() {
        // A Type II block with only 10 bytes of payload — one short of a full descriptor.
        let mut payload = vec![TAG_TYPE_II_TIMING, 0x00, 10];
        payload.extend_from_slice(&[0u8; 10]);
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, 0x10, &mut caps, &DecodeOptions::new());
        assert!(caps.supported_modes.is_empty());
    }

    // -----------------------------------------------------------------------
    // Static pipeline (process_data_blocks with StaticContext)
    // -----------------------------------------------------------------------

    #[test]
    fn test_type_i_static_pipeline() {
        let d = make_type_i_descriptor(14850, 1920, 280, 88, 44, 1080, 45, 4, 5, 0x00);
        let mut payload = vec![TAG_TYPE_I_TIMING, 0x00, 20];
        payload.extend_from_slice(&d);
        let mut caps = StaticDisplayCapabilities::<16>::default();
        let mut ctx = StaticContext::new(&mut caps);
        process_data_blocks(&payload, 0x10, &mut ctx, &DecodeOptions::new());
        assert_eq!(caps.num_modes, 1);
        let mode = caps.supported_modes[0].as_ref().unwrap();
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
    }

    #[test]
    fn test_type_ii_static_pipeline() {
        // ha_raw=239→h=1920, va_raw=1079→v=1080, pixel_clock raw=14849→actual=14850×10kHz
        // byte 9 = 0x2C → v_blank=45, v_total=1125 → 148_500_000 / (2200 × 1125) = 60 Hz exact.
        let pixel_clock_10khz: u32 = 14849;
        let ha_raw: u16 = 239;
        let hb_raw: u8 = 34;
        let hfp_raw: u8 = 10;
        let hsw_raw: u8 = 5;
        let va_raw: u16 = 1079;
        let mut payload = vec![TAG_TYPE_II_TIMING, 0x00, 11];
        let mut d = [0u8; 11];
        d[0] = pixel_clock_10khz as u8;
        d[1] = (pixel_clock_10khz >> 8) as u8;
        d[3] = 0x0C; // H+, V+ sync
        d[4] = (ha_raw & 0xFF) as u8;
        d[5] = (((ha_raw >> 8) & 0x01) as u8) | ((hb_raw & 0x7F) << 1);
        d[6] = ((hfp_raw & 0x0F) << 4) | (hsw_raw & 0x0F);
        d[7] = (va_raw & 0xFF) as u8;
        d[8] = ((va_raw >> 8) & 0x0F) as u8;
        d[9] = 0x2C;
        payload.extend_from_slice(&d);
        let mut caps = StaticDisplayCapabilities::<16>::default();
        let mut ctx = StaticContext::new(&mut caps);
        process_data_blocks(&payload, 0x10, &mut ctx, &DecodeOptions::new());
        assert_eq!(caps.num_modes, 1);
        let mode = caps.supported_modes[0].as_ref().unwrap();
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
    }

    #[test]
    fn test_type_iii_static_pipeline() {
        // 1920×1080@60: aspect=16:9(0x04), h_raw=239→h=1920, refresh_raw=59→60 Hz
        let byte0 = 0x04u8; // aspect 16:9
        let payload = vec![TAG_TYPE_III_TIMING, 0x00, 3, byte0, 239, 59];
        let mut caps = StaticDisplayCapabilities::<16>::default();
        let mut ctx = StaticContext::new(&mut caps);
        process_data_blocks(&payload, 0x10, &mut ctx, &DecodeOptions::new());
        assert_eq!(caps.num_modes, 1);
        let mode = caps.supported_modes[0].as_ref().unwrap();
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
    }

    #[test]
    fn test_type_iv_static_pipeline() {
        // DMT 0x09 = 800×600@60 Hz; code_type=0 (DMT) → revision bits 7:6 = 0b00
        let payload = vec![TAG_TYPE_IV_TIMING, 0x00, 1, 0x09];
        let mut caps = StaticDisplayCapabilities::<16>::default();
        let mut ctx = StaticContext::new(&mut caps);
        process_data_blocks(&payload, 0x10, &mut ctx, &DecodeOptions::new());
        assert_eq!(caps.num_modes, 1);
        let mode = caps.supported_modes[0].as_ref().unwrap();
        assert_eq!(mode.width, 800);
        assert_eq!(mode.height, 600);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
    }

    #[test]
    fn test_vesa_timing_static_pipeline() {
        // Byte 1 bit 0 = DMT 0x09 = 800×600@60 Hz.
        let payload = vec![TAG_VESA_VIDEO_TIMING, 0x00, 2, 0x00, 0x01];
        let mut caps = StaticDisplayCapabilities::<16>::default();
        let mut ctx = StaticContext::new(&mut caps);
        process_data_blocks(&payload, 0x10, &mut ctx, &DecodeOptions::new());
        assert_eq!(caps.num_modes, 1);
        let mode = caps.supported_modes[0].as_ref().unwrap();
        assert_eq!(mode.width, 800);
        assert_eq!(mode.height, 600);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
    }

    #[test]
    fn test_cta_timing_static_pipeline() {
        // Byte 0 bit 0 = VIC 1 = 640×480@60 Hz.
        let payload = vec![TAG_CTA_VIDEO_TIMING, 0x00, 1, 0x01];
        let mut caps = StaticDisplayCapabilities::<16>::default();
        let mut ctx = StaticContext::new(&mut caps);
        process_data_blocks(&payload, 0x10, &mut ctx, &DecodeOptions::new());
        assert_eq!(caps.num_modes, 1);
        assert_eq!(caps.supported_modes[0].as_ref().unwrap().width, 640);
    }

    #[test]
    fn test_type_v_static_pipeline() {
        // 1920×1080@60 Hz. H/V active fields use the wire encoding `value − 1`:
        // 1920 → raw 1919, 1080 → raw 1079. refresh_raw = 59 → decoded 60 Hz.
        let mut payload = vec![TAG_TYPE_V_TIMING, 0x00, 7];
        payload.extend_from_slice(&[0x00]); // options
        payload.extend_from_slice(&(1920u16 - 1).to_le_bytes()); // h_active raw
        payload.extend_from_slice(&(1080u16 - 1).to_le_bytes()); // v_active raw
        payload.push(59); // refresh_raw
        payload.push(0x00); // reserved
        let mut caps = StaticDisplayCapabilities::<16>::default();
        let mut ctx = StaticContext::new(&mut caps);
        process_data_blocks(&payload, 0x10, &mut ctx, &DecodeOptions::new());
        assert_eq!(caps.num_modes, 1);
        let mode = caps.supported_modes[0].as_ref().unwrap();
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
    }

    fn make_type_vii_descriptor_1080p60() -> [u8; 20] {
        // 1920×1080@60: pc=148_500 kHz, h_total=2200, v_total=1125 → exact 60 Hz.
        // Type VII wire encoding stores every multi-byte field as `value − 1`.
        let mut d = [0u8; 20];
        let raw_pc: u32 = 148_500 - 1;
        d[0] = (raw_pc & 0xFF) as u8;
        d[1] = ((raw_pc >> 8) & 0xFF) as u8;
        d[2] = ((raw_pc >> 16) & 0xFF) as u8;
        d[3] = 0x00;
        d[4..6].copy_from_slice(&(1920u16 - 1).to_le_bytes());
        d[6..8].copy_from_slice(&(280u16 - 1).to_le_bytes());
        d[8..10].copy_from_slice(&(88u16 - 1).to_le_bytes());
        d[10..12].copy_from_slice(&(44u16 - 1).to_le_bytes());
        d[12..14].copy_from_slice(&(1080u16 - 1).to_le_bytes());
        d[14..16].copy_from_slice(&(45u16 - 1).to_le_bytes());
        d[16..18].copy_from_slice(&(4u16 - 1).to_le_bytes());
        d[18..20].copy_from_slice(&(5u16 - 1).to_le_bytes());
        d
    }

    #[test]
    fn test_type_vii_v2_dispatch() {
        let descriptor = make_type_vii_descriptor_1080p60();
        let mut payload = vec![TAG_V2_TYPE_VII_TIMING, 0x00, 20];
        payload.extend_from_slice(&descriptor);
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, DISPLAYID_V2, &mut caps, &DecodeOptions::new());
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
    }

    #[test]
    fn test_type_vii_v2_block_with_two_descriptors() {
        let desc_1080p60 = make_type_vii_descriptor_1080p60();
        // Second descriptor: 1280×720@60, pc=74_250 kHz, h_total=1650, v_total=750.
        // All multi-byte fields stored as `value − 1`.
        let mut desc_720p60 = [0u8; 20];
        let raw_pc: u32 = 74_250 - 1;
        desc_720p60[0] = (raw_pc & 0xFF) as u8;
        desc_720p60[1] = ((raw_pc >> 8) & 0xFF) as u8;
        desc_720p60[2] = ((raw_pc >> 16) & 0xFF) as u8;
        desc_720p60[4..6].copy_from_slice(&(1280u16 - 1).to_le_bytes());
        desc_720p60[6..8].copy_from_slice(&(370u16 - 1).to_le_bytes());
        desc_720p60[12..14].copy_from_slice(&(720u16 - 1).to_le_bytes());
        desc_720p60[14..16].copy_from_slice(&(30u16 - 1).to_le_bytes());
        let mut payload = vec![TAG_V2_TYPE_VII_TIMING, 0x00, 40];
        payload.extend_from_slice(&desc_1080p60);
        payload.extend_from_slice(&desc_720p60);
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, DISPLAYID_V2, &mut caps, &DecodeOptions::new());
        assert_eq!(caps.supported_modes.len(), 2);
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 1920 && m.height == 1080)
        );
        assert!(
            caps.supported_modes
                .iter()
                .any(|m| m.width == 1280 && m.height == 720)
        );
    }

    #[test]
    fn test_type_vii_not_decoded_on_v1_section() {
        // Same payload, V1 version byte → tag 0x22 isn't in the V1 dispatch arm and is dropped.
        let descriptor = make_type_vii_descriptor_1080p60();
        let mut payload = vec![TAG_V2_TYPE_VII_TIMING, 0x00, 20];
        payload.extend_from_slice(&descriptor);
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, 0x10, &mut caps, &DecodeOptions::new());
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_v2_section_does_not_decode_v1_timing() {
        // Type I block tag with version 0x20 → V2 dispatch ignores V1 tags.
        let desc = make_type_i_descriptor(14850, 1920, 280, 88, 44, 1080, 45, 4, 5, 0x00);
        let mut payload = vec![TAG_TYPE_I_TIMING, 0x00, 20];
        payload.extend_from_slice(&desc);
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, DISPLAYID_V2, &mut caps, &DecodeOptions::new());
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_viii_v2_dispatch_dmt() {
        // revision: code_type=DMT(0), TCS=0; payload [0x52] → 1920×1080@60.
        let payload = vec![TAG_V2_TYPE_VIII_TIMING, 0x00, 1, 0x52];
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, DISPLAYID_V2, &mut caps, &DecodeOptions::new());
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1920);
        assert_eq!(caps.supported_modes[0].height, 1080);
    }

    #[test]
    fn test_type_viii_v2_dispatch_two_byte_dmt() {
        // revision: code_type=DMT(0), TCS=1 (bit 3); payload [0x52, 0x00].
        let payload = vec![TAG_V2_TYPE_VIII_TIMING, 0x08, 2, 0x52, 0x00];
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, DISPLAYID_V2, &mut caps, &DecodeOptions::new());
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1920);
    }

    #[test]
    fn test_type_viii_not_decoded_on_v1_section() {
        let payload = vec![TAG_V2_TYPE_VIII_TIMING, 0x00, 1, 0x52];
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, 0x10, &mut caps, &DecodeOptions::new());
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_viii_v2_static_pipeline() {
        let payload = vec![TAG_V2_TYPE_VIII_TIMING, 0x40, 1, 0x01]; // VIC 1 = 640×480@60
        let mut caps = StaticDisplayCapabilities::<16>::default();
        let mut ctx = StaticContext::new(&mut caps);
        process_data_blocks(&payload, DISPLAYID_V2, &mut ctx, &DecodeOptions::new());
        assert_eq!(caps.num_modes, 1);
        let mode = caps.supported_modes[0].as_ref().unwrap();
        assert_eq!(mode.width, 640);
        assert_eq!(mode.height, 480);
    }

    #[test]
    fn test_type_ix_v2_dispatch() {
        // 1920×1080@60: refresh_raw = 59 → 60 Hz.
        let mut payload = vec![TAG_V2_TYPE_IX_TIMING, 0x00, 6];
        let mut d = [0u8; 6];
        // Type IX wire encoding stores h_active and v_active as `value − 1`.
        d[1..3].copy_from_slice(&(1920u16 - 1).to_le_bytes());
        d[3..5].copy_from_slice(&(1080u16 - 1).to_le_bytes());
        d[5] = 59;
        payload.extend_from_slice(&d);
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, DISPLAYID_V2, &mut caps, &DecodeOptions::new());
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
    }

    #[test]
    fn test_type_ix_not_decoded_on_v1_section() {
        let mut payload = vec![TAG_V2_TYPE_IX_TIMING, 0x00, 6];
        let mut d = [0u8; 6];
        // Type IX wire encoding stores h_active and v_active as `value − 1`.
        d[1..3].copy_from_slice(&(1920u16 - 1).to_le_bytes());
        d[3..5].copy_from_slice(&(1080u16 - 1).to_le_bytes());
        d[5] = 59;
        payload.extend_from_slice(&d);
        let mut caps = DisplayCapabilities::default();
        process_data_blocks(&payload, 0x10, &mut caps, &DecodeOptions::new());
        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_type_ix_v2_static_pipeline() {
        let mut payload = vec![TAG_V2_TYPE_IX_TIMING, 0x00, 6];
        let mut d = [0u8; 6];
        // Type IX wire encoding stores h_active and v_active as `value − 1`.
        d[1..3].copy_from_slice(&(1920u16 - 1).to_le_bytes());
        d[3..5].copy_from_slice(&(1080u16 - 1).to_le_bytes());
        d[5] = 59;
        payload.extend_from_slice(&d);
        let mut caps = StaticDisplayCapabilities::<16>::default();
        let mut ctx = StaticContext::new(&mut caps);
        process_data_blocks(&payload, DISPLAYID_V2, &mut ctx, &DecodeOptions::new());
        assert_eq!(caps.num_modes, 1);
        let mode = caps.supported_modes[0].as_ref().unwrap();
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
    }

    #[test]
    fn test_type_vii_v2_static_pipeline() {
        let descriptor = make_type_vii_descriptor_1080p60();
        let mut payload = vec![TAG_V2_TYPE_VII_TIMING, 0x00, 20];
        payload.extend_from_slice(&descriptor);
        let mut caps = StaticDisplayCapabilities::<16>::default();
        let mut ctx = StaticContext::new(&mut caps);
        process_data_blocks(&payload, DISPLAYID_V2, &mut ctx, &DecodeOptions::new());
        assert_eq!(caps.num_modes, 1);
        let mode = caps.supported_modes[0].as_ref().unwrap();
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
    }

    #[test]
    fn test_type_vi_static_pipeline() {
        // 1920×1080@60 Hz: pixel clock 148_500 kHz, h_total=2200, v_total=1125
        let pc: u32 = 148_500;
        let h_word: u16 = 1920 | (1 << 15); // H sync +
        let v_word: u16 = 1080 | (1 << 15); // V sync +
        let h_blank: u16 = 280;
        let v_blank: u8 = 45;
        let mut descriptor = [0u8; 14];
        descriptor[0] = (pc & 0xFF) as u8;
        descriptor[1] = ((pc >> 8) & 0xFF) as u8;
        descriptor[2] = ((pc >> 16) & 0x3F) as u8;
        descriptor[3..5].copy_from_slice(&h_word.to_le_bytes());
        descriptor[5..7].copy_from_slice(&v_word.to_le_bytes());
        descriptor[7] = (h_blank & 0xFF) as u8;
        descriptor[9] = ((h_blank >> 8) & 0x0F) as u8;
        descriptor[11] = v_blank;
        let mut payload = vec![TAG_TYPE_VI_TIMING, 0x00, 14];
        payload.extend_from_slice(&descriptor);
        let mut caps = StaticDisplayCapabilities::<16>::default();
        let mut ctx = StaticContext::new(&mut caps);
        process_data_blocks(&payload, 0x10, &mut ctx, &DecodeOptions::new());
        assert_eq!(caps.num_modes, 1);
        let mode = caps.supported_modes[0].as_ref().unwrap();
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert_eq!(mode.refresh_rate, Some(RefreshRate::integral(60)));
    }

    #[test]
    fn test_v2_cta_displayid_static_path_emits_modes() {
        // V2 section with a 0x81 block carrying VIC 1 (640x480@60).
        // Static path should extract the mode through cea861_collection_into_sink.
        let collection: [u8; 2] = [
            (0x02 << 5) | 0x01, // CTA Video Data Block: tag=2, length=1
            1,                  // VIC 1
        ];
        let mut payload = vec![TAG_V2_CTA_DISPLAYID, 0x00, collection.len() as u8];
        payload.extend_from_slice(&collection);

        let mut caps = StaticDisplayCapabilities::<16>::default();
        let mut ctx = StaticContext::new(&mut caps);
        process_data_blocks(&payload, DISPLAYID_V2, &mut ctx, &DecodeOptions::new());
        assert_eq!(caps.num_modes, 1);
        let mode = caps.supported_modes[0].as_ref().unwrap();
        assert_eq!(mode.width, 640);
        assert_eq!(mode.height, 480);
    }
}
