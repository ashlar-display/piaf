#[cfg(any(feature = "alloc", feature = "std"))]
mod audio;
#[cfg(any(feature = "alloc", feature = "std"))]
mod extended_blocks;
#[cfg(any(feature = "alloc", feature = "std"))]
mod hdmi_vsdb;

#[cfg(any(feature = "alloc", feature = "std"))]
pub use audio::{AudioFormat, AudioFormatInfo, AudioSampleRates, ShortAudioDescriptor};
pub use display_types::cea861::Cea861Flags;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use display_types::cea861::{
    Audio3dChannels, Audio3dSpeakerAllocation, Cea861Capabilities, HdmiAudioBlock,
};
#[cfg(any(feature = "alloc", feature = "std"))]
pub use extended_blocks::{
    ColorimetryBlock, ColorimetryFlags, DtcPointEncoding, HdmiDscMaxSlices, HdmiForumDsc,
    HdmiForumFrl, HdmiForumSinkCap, HdrDynamicMetadataDescriptor, HdrEotf, HdrStaticMetadata,
    InfoFrameDescriptor, RoomConfigurationBlock, SpeakerAllocation, SpeakerAllocationFlags,
    SpeakerAllocationFlags2, SpeakerAllocationFlags3, SpeakerLocationEntry, T7VtdbBlock,
    T8VtdbBlock, T10VtdbBlock, T10VtdbEntry, VendorSpecificBlock, VesaDisplayDeviceBlock,
    VesaTransferCharacteristic, VideoCapability, VideoCapabilityFlags, VtbExtBlock, infoframe_type,
};
#[cfg(any(feature = "alloc", feature = "std"))]
pub use hdmi_vsdb::{HdmiVsdb, HdmiVsdbFlags};

pub(crate) use display_types::cea861::{dmt_to_mode, vic_to_mode};

#[cfg(any(feature = "alloc", feature = "std"))]
use crate::capabilities::base::timings::decode_dtd_slot;
use crate::capabilities::base::timings::decode_dtd_slot_into_sink;
use crate::model::capabilities::DisplayCapabilities;
use crate::model::capabilities::ModeSink;
use crate::model::capabilities::StaticContext;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::capabilities::VideoMode;
use crate::model::diagnostics::EdidWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::diagnostics::ParseWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::extension::ExtensionHandler;
use crate::model::extension::StaticExtensionHandler;
use crate::model::options::DecodeOptions;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::prelude::{Arc, Vec};
#[cfg(any(feature = "alloc", feature = "std"))]
use audio::parse_audio_data_block;
#[cfg(any(feature = "alloc", feature = "std"))]
use audio::parse_hdmi_audio_block;
#[cfg(any(feature = "alloc", feature = "std"))]
use extended_blocks::{
    EXT_TAG_COLORIMETRY, EXT_TAG_HDMI_AUDIO, EXT_TAG_HDR_DYNAMIC_METADATA,
    EXT_TAG_HDR_STATIC_METADATA, EXT_TAG_HF_EEODB, EXT_TAG_HF_SCDB, EXT_TAG_INFOFRAME,
    EXT_TAG_ROOM_CONFIGURATION, EXT_TAG_SPEAKER_LOCATION, EXT_TAG_T7VTDB, EXT_TAG_T8VTDB,
    EXT_TAG_T10VTDB, EXT_TAG_VESA_DDDB, EXT_TAG_VIDEO_CAPABILITY, EXT_TAG_VIDEO_FORMAT_PREFERENCE,
    EXT_TAG_VSADB, EXT_TAG_VSVDB, EXT_TAG_VTB_EXT, EXT_TAG_Y420_CAPABILITY_MAP, EXT_TAG_Y420_VIDEO,
    parse_colorimetry, parse_hdr_dynamic_metadata, parse_hdr_static_metadata, parse_hf_eeodb,
    parse_hf_scdb, parse_hf_vsdb, parse_infoframe_db, parse_room_configuration,
    parse_speaker_allocation, parse_speaker_location, parse_t7vtdb, parse_t8vtdb, parse_t10vtdb,
    parse_vendor_specific_block, parse_vesa_display_device, parse_vesa_transfer_characteristic,
    parse_video_capability, parse_video_format_preferences, parse_vtb_ext,
    parse_y420_capability_map, parse_y420_vdb,
};
#[cfg(any(feature = "alloc", feature = "std"))]
use hdmi_vsdb::parse_hdmi_vsdb;
/// Processes a CEA-861 extension block (tag `0x02`).
#[derive(Debug)]
pub struct Cea861Handler;

#[cfg(any(feature = "alloc", feature = "std"))]
impl ExtensionHandler for Cea861Handler {
    fn process(
        &self,
        blocks: &[&[u8; 128]],
        caps: &mut DisplayCapabilities,
        warnings: &mut Vec<ParseWarning>,
    ) {
        self.process_with_options(blocks, caps, warnings, &DecodeOptions::new());
    }

    fn process_with_options(
        &self,
        blocks: &[&[u8; 128]],
        caps: &mut DisplayCapabilities,
        warnings: &mut Vec<ParseWarning>,
        options: &DecodeOptions,
    ) {
        let Some(first) = blocks.first() else { return };
        let flags = Cea861Flags::from_bits_truncate(first[3]);

        // Take-merge: another source (e.g. DisplayID 2.x block 0x81) may have already
        // populated CTA-861 caps for this sink. Merge into that struct rather than
        // overwriting; OR-in this extension's flags onto whatever was there.
        let mut cea_caps = caps
            .take_extension_data::<Cea861Capabilities>(0x02)
            .unwrap_or_else(|| Cea861Capabilities::new(flags));
        cea_caps.flags |= flags;

        for ext in blocks {
            // Parse the data block collection: bytes 4 through dtd_offset-1.
            // When dtd_offset == 0 the spec says no DTDs are present, so data blocks
            // may fill the rest of the block (bytes 4–127).
            let dtd_offset = ext[2] as usize;
            let collection_end = if dtd_offset == 0 {
                128
            } else {
                dtd_offset.min(128)
            };

            if collection_end > 4 {
                parse_cea861_data_block_collection(
                    &ext[4..collection_end],
                    caps,
                    &mut cea_caps,
                    warnings,
                    options,
                );
            }

            // Parse Detailed Timing Descriptors from byte dtd_offset through byte 126.
            // When dtd_offset == 0 there are no DTDs (the spec says no timing data present).
            if (4..=110).contains(&dtd_offset) {
                let mut offset = dtd_offset;
                let mut dtd_index: u8 = 0;
                while offset + 18 <= 127 {
                    decode_dtd_slot(&ext[offset..offset + 18], caps, dtd_index, options);
                    offset += 18;
                    dtd_index = dtd_index.saturating_add(1);
                }
            }
        } // end for ext in blocks

        caps.set_extension_data(0x02, cea_caps);
    }
}

/// Parses a CTA-861 data block collection into `caps` and `cea_caps`.
///
/// `collection` is the raw byte sequence of CTA data blocks: each starts with a
/// 1-byte header (`tag << 5 | length`) followed by `length` payload bytes. A zero
/// header byte ends scanning. This is the body of a CEA-861 extension block (between
/// byte 4 and `dtd_offset`) but is also reused for the DisplayID 2.x CTA DisplayID
/// block (tag `0x81`), which wraps the same structure.
///
/// Modes extracted from VICs and timing data blocks are appended to
/// `caps.supported_modes` (with dedup against existing entries). All CTA-specific
/// state — audio descriptors, vendor-specific blocks, HDR metadata, etc. — is
/// merged into `cea_caps`.
#[cfg(any(feature = "alloc", feature = "std"))]
pub(crate) fn parse_cea861_data_block_collection(
    collection: &[u8],
    caps: &mut DisplayCapabilities,
    cea_caps: &mut Cea861Capabilities,
    warnings: &mut Vec<ParseWarning>,
    options: &DecodeOptions,
) {
    let mut i = 0;
    while i < collection.len() {
        let header = collection[i];

        // A zero header byte is padding — stop scanning.
        if header == 0 {
            break;
        }

        let tag = (header >> 5) & 0x07;
        let length = (header & 0x1F) as usize;
        i += 1;

        if i + length > collection.len() {
            warnings.push(Arc::new(EdidWarning::MalformedDataBlock));
            break;
        }

        let block_data = &collection[i..i + length];

        match tag {
            0x01 => {
                // Audio Data Block: 3-byte Short Audio Descriptors.
                cea_caps
                    .audio_descriptors
                    .extend(parse_audio_data_block(block_data));
            }
            0x02 => handle_video_data_block(block_data, caps, cea_caps),
            0x03 => {
                // Vendor-Specific Data Block: dispatch by OUI.
                if cea_caps.hdmi_vsdb.is_none() {
                    cea_caps.hdmi_vsdb = parse_hdmi_vsdb(block_data);
                }
                if cea_caps.hf_vsdb.is_none() {
                    cea_caps.hf_vsdb = parse_hf_vsdb(block_data);
                }
            }
            0x04
                // Speaker Allocation Data Block.
                if cea_caps.speaker_allocation.is_none() => {
                cea_caps.speaker_allocation = parse_speaker_allocation(block_data);
            }
            0x05
                // VESA Display Transfer Characteristic Data Block.
                if cea_caps.vesa_transfer_characteristic.is_none() => {
                cea_caps.vesa_transfer_characteristic =
                    parse_vesa_transfer_characteristic(block_data);
            }
            0x07 => handle_extended_tag_block(block_data, caps, cea_caps, options),
            _ => {}
        }

        i += length;
    }
}

/// Pushes `mode` to `modes` only if no entry with the same
/// `(width, height, refresh_rate, interlaced)` already exists.
#[cfg(any(feature = "alloc", feature = "std"))]
fn push_mode_dedup(modes: &mut Vec<VideoMode>, mode: VideoMode) {
    let already_present = modes.iter().any(|m| {
        m.width == mode.width
            && m.height == mode.height
            && m.refresh_rate == mode.refresh_rate
            && m.interlaced == mode.interlaced
    });
    if !already_present {
        modes.push(mode);
    }
}

/// Decodes a CTA-861 Video Data Block (`tag = 0x02`): a sequence of Short Video Descriptors.
///
/// Standard SVD: 1 byte; bits 6:0 = VIC (1–127), bit 7 = native flag.
/// Extended SVD (CTA-861-G+): if bits 6:0 == 0, the *next* byte is the full VIC number
/// (128–255); bit 7 of the first byte = native flag.
#[cfg(any(feature = "alloc", feature = "std"))]
fn handle_video_data_block(
    block_data: &[u8],
    caps: &mut DisplayCapabilities,
    cea_caps: &mut Cea861Capabilities,
) {
    let mut j = 0;
    while j < block_data.len() {
        let b = block_data[j];
        let native = (b & 0x80) != 0;
        let vic_low = b & 0x7F;

        let vic = if vic_low == 0 {
            // Extended SVD — consume one more byte.
            j += 1;
            match block_data.get(j).copied() {
                Some(0) | None => {
                    j += 1;
                    continue; // VIC 0 reserved
                }
                Some(v) => v,
            }
        } else {
            vic_low
        };
        j += 1;

        cea_caps.vics.push((vic, native));

        if let Some(mode) = vic_to_mode(vic) {
            push_mode_dedup(&mut caps.supported_modes, mode);
        }
    }
}

/// Decodes a CTA-861 Extended Tag Data Block (`tag = 0x07`): the first payload byte is
/// the extended tag, dispatched against the `EXT_TAG_*` constants.
#[cfg(any(feature = "alloc", feature = "std"))]
fn handle_extended_tag_block(
    block_data: &[u8],
    caps: &mut DisplayCapabilities,
    cea_caps: &mut Cea861Capabilities,
    options: &DecodeOptions,
) {
    match block_data.first().copied() {
        Some(EXT_TAG_VSVDB) => {
            if let Some(b) = parse_vendor_specific_block(&block_data[1..]) {
                cea_caps.vendor_specific_video.push(b);
            }
        }
        Some(EXT_TAG_VSADB) => {
            if let Some(b) = parse_vendor_specific_block(&block_data[1..]) {
                cea_caps.vendor_specific_audio.push(b);
            }
        }
        Some(EXT_TAG_VESA_DDDB) if cea_caps.vesa_display_device.is_none() => {
            cea_caps.vesa_display_device = parse_vesa_display_device(&block_data[1..]);
        }
        Some(EXT_TAG_VTB_EXT) => {
            if let Some(vtb) =
                parse_vtb_ext(&block_data[1..], options, &mut |w| caps.push_warning(w))
            {
                for timing in &vtb.timings {
                    push_mode_dedup(&mut caps.supported_modes, timing.clone());
                }
                cea_caps.vtb_ext.push(vtb);
            }
        }
        Some(EXT_TAG_VIDEO_CAPABILITY) if cea_caps.video_capability.is_none() => {
            cea_caps.video_capability = parse_video_capability(block_data);
        }
        Some(EXT_TAG_COLORIMETRY) if cea_caps.colorimetry.is_none() => {
            cea_caps.colorimetry = parse_colorimetry(block_data);
        }
        Some(EXT_TAG_HDR_STATIC_METADATA) if cea_caps.hdr_static_metadata.is_none() => {
            cea_caps.hdr_static_metadata = parse_hdr_static_metadata(block_data);
        }
        Some(EXT_TAG_HDR_DYNAMIC_METADATA) => {
            cea_caps
                .hdr_dynamic_metadata
                .extend(parse_hdr_dynamic_metadata(block_data));
        }
        Some(EXT_TAG_VIDEO_FORMAT_PREFERENCE) if cea_caps.video_format_preferences.is_empty() => {
            cea_caps.video_format_preferences = parse_video_format_preferences(block_data);
        }
        Some(EXT_TAG_Y420_VIDEO) => {
            let vics = parse_y420_vdb(block_data);
            for &vic in &vics {
                if let Some(mode) = vic_to_mode(vic) {
                    push_mode_dedup(&mut caps.supported_modes, mode);
                }
            }
            cea_caps.y420_vics.extend(vics);
        }
        Some(EXT_TAG_Y420_CAPABILITY_MAP) if cea_caps.y420_capability_map.is_empty() => {
            cea_caps.y420_capability_map = parse_y420_capability_map(block_data);
        }
        Some(EXT_TAG_INFOFRAME) => {
            cea_caps
                .infoframe_descriptors
                .extend(parse_infoframe_db(block_data));
        }
        Some(EXT_TAG_ROOM_CONFIGURATION) if cea_caps.room_configuration.is_none() => {
            cea_caps.room_configuration = parse_room_configuration(block_data);
        }
        Some(EXT_TAG_SPEAKER_LOCATION) => {
            cea_caps
                .speaker_locations
                .extend(parse_speaker_location(block_data));
        }
        Some(EXT_TAG_T7VTDB) => {
            if let Some(t7) = parse_t7vtdb(&block_data[1..]) {
                push_mode_dedup(&mut caps.supported_modes, t7.mode.clone());
                cea_caps.t7_vtdb.push(t7);
            }
        }
        Some(EXT_TAG_T8VTDB) => {
            if let Some(t8) = parse_t8vtdb(&block_data[1..]) {
                for mode in &t8.timings {
                    push_mode_dedup(&mut caps.supported_modes, mode.clone());
                }
                cea_caps.t8_vtdb.push(t8);
            }
        }
        Some(EXT_TAG_T10VTDB) => {
            if let Some(t10) = parse_t10vtdb(&block_data[1..]) {
                for entry in &t10.entries {
                    let mode = VideoMode::new(entry.width, entry.height, entry.refresh_hz, false);
                    push_mode_dedup(&mut caps.supported_modes, mode);
                }
                cea_caps.t10_vtdb.push(t10);
            }
        }
        Some(EXT_TAG_HF_EEODB) if cea_caps.hf_eeodb_extension_count.is_none() => {
            cea_caps.hf_eeodb_extension_count = parse_hf_eeodb(&block_data[1..]);
        }
        Some(EXT_TAG_HF_SCDB) if cea_caps.hf_scdb.is_none() => {
            cea_caps.hf_scdb = parse_hf_scdb(&block_data[1..]);
        }
        Some(EXT_TAG_HDMI_AUDIO) if cea_caps.hdmi_audio.is_none() => {
            cea_caps.hdmi_audio = Some(parse_hdmi_audio_block(&block_data[1..], &mut |w| {
                caps.push_warning(w)
            }));
        }
        _ => {}
    }
}

/// Extracts video modes from a CTA-861 data block collection into `sink`.
///
/// Allocation-free counterpart to [`parse_cea861_data_block_collection`]: emits
/// [`VideoMode`][crate::VideoMode] entries from VICs and timing data blocks. Reused
/// from both the CEA-861 extension static path and the DisplayID 2.x CTA DisplayID
/// block (tag `0x81`) static path.
pub(crate) fn cea861_collection_into_sink(
    collection: &[u8],
    sink: &mut dyn ModeSink,
    options: &DecodeOptions,
) {
    // Only VTB-EXT carries DTDs here, and it is decoded in alloc builds only.
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    let _ = options;
    let mut i = 0;
    while i < collection.len() {
        let header = collection[i];
        if header == 0 {
            break;
        }

        let tag = (header >> 5) & 0x07;
        let length = (header & 0x1F) as usize;
        i += 1;

        if i + length > collection.len() {
            sink.push_warning(EdidWarning::MalformedDataBlock);
            break;
        }

        let block_data = &collection[i..i + length];

        if tag == 0x02 {
            // Video Data Block: Short Video Descriptors.
            let mut j = 0;
            while j < block_data.len() {
                let b = block_data[j];
                let vic_low = b & 0x7F;

                let vic = if vic_low == 0 {
                    j += 1;
                    match block_data.get(j).copied() {
                        Some(0) | None => {
                            j += 1;
                            continue;
                        }
                        Some(v) => v,
                    }
                } else {
                    vic_low
                };
                j += 1;

                if let Some(mode) = vic_to_mode(vic) {
                    sink.push_mode(mode);
                }
            }
        } else if tag == 0x07 {
            // Extended Tag Data Block — mode-producing sub-types.
            #[cfg(any(feature = "alloc", feature = "std"))]
            match block_data.first().copied() {
                Some(EXT_TAG_VTB_EXT) => {
                    let mut found = Vec::new();
                    let vtb = parse_vtb_ext(&block_data[1..], options, &mut |w| found.push(w));
                    for w in found {
                        sink.push_warning(w);
                    }
                    if let Some(vtb) = vtb {
                        for timing in vtb.timings {
                            sink.push_mode(timing);
                        }
                    }
                }
                Some(EXT_TAG_Y420_VIDEO) => {
                    for &vic in &parse_y420_vdb(block_data) {
                        if let Some(mode) = vic_to_mode(vic) {
                            sink.push_mode(mode);
                        }
                    }
                }
                Some(EXT_TAG_T7VTDB) => {
                    if let Some(t7) = parse_t7vtdb(&block_data[1..]) {
                        sink.push_mode(t7.mode);
                    }
                }
                Some(EXT_TAG_T8VTDB) => {
                    if let Some(t8) = parse_t8vtdb(&block_data[1..]) {
                        for mode in t8.timings {
                            sink.push_mode(mode);
                        }
                    }
                }
                Some(EXT_TAG_T10VTDB) => {
                    if let Some(t10) = parse_t10vtdb(&block_data[1..]) {
                        for entry in &t10.entries {
                            sink.push_mode(VideoMode::new(
                                entry.width,
                                entry.height,
                                entry.refresh_hz,
                                false,
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        i += length;
    }
}

/// Extracts video modes from a CEA-861 extension block into `sink`.
///
/// Allocation-free counterpart to [`Cea861Handler::process`]: produces
/// [`VideoMode`][crate::VideoMode] entries (SVDs, DTDs, and — in `alloc`/`std` builds —
/// VTB-EXT/T7VTDB/T8VTDB/T10VTDB/Y420 VDB entries) without requiring heap allocation.
pub(crate) fn cea861_process_into_sink(
    ext: &[u8; 128],
    sink: &mut dyn ModeSink,
    options: &DecodeOptions,
) {
    let dtd_offset = ext[2] as usize;
    let collection_end = if dtd_offset == 0 {
        128
    } else {
        dtd_offset.min(128)
    };

    if collection_end > 4 {
        cea861_collection_into_sink(&ext[4..collection_end], sink, options);
    }

    // Parse Detailed Timing Descriptors.
    if (4..=110).contains(&dtd_offset) {
        let mut offset = dtd_offset;
        let mut dtd_index: u8 = 0;
        while offset + 18 <= 127 {
            decode_dtd_slot_into_sink(&ext[offset..offset + 18], sink, dtd_index, options);
            offset += 18;
            dtd_index = dtd_index.saturating_add(1);
        }
    }
}

impl StaticExtensionHandler for Cea861Handler {
    fn tag(&self) -> u8 {
        0x02
    }

    fn process(&self, blocks: &[&[u8; 128]], ctx: &mut StaticContext<'_>) {
        let options = *ctx.options();
        for block in blocks {
            cea861_process_into_sink(block, ctx, &options);
        }
    }
}

/// Pre-built static reference to the built-in CEA-861 handler.
///
/// Suitable for passing to [`parse_edid`][crate::parse_edid] as the `KnownExtensions`
/// argument, and to [`capabilities_from_edid_static`][crate::capabilities_from_edid_static]
/// as a single-element slice.
pub static CEA861_HANDLER: &dyn StaticExtensionHandler = &Cea861Handler;

#[cfg(test)]
mod test_static {
    use super::*;
    use crate::model::extension::StaticExtensionHandler;

    #[test]
    fn test_static_handler_tag() {
        assert_eq!(StaticExtensionHandler::tag(&Cea861Handler), 0x02);
    }
}

#[cfg(test)]
#[cfg(any(feature = "alloc", feature = "std"))]
mod tests {
    use super::*;
    use crate::model::capabilities::{DisplayCapabilities, RefreshRate};

    #[test]
    fn test_audio_flag() {
        let mut ext = [0u8; 128];
        ext[3] = Cea861Flags::BASIC_AUDIO.bits();

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        let cea = caps.get_extension_data::<Cea861Capabilities>(0x02).unwrap();
        assert!(cea.flags.contains(Cea861Flags::BASIC_AUDIO));
    }

    #[test]
    fn test_no_audio_flag() {
        let ext = [0u8; 128];

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        let cea = caps.get_extension_data::<Cea861Capabilities>(0x02).unwrap();
        assert!(!cea.flags.contains(Cea861Flags::BASIC_AUDIO));
    }

    #[test]
    fn test_flags_parsing() {
        let flags = Cea861Flags::from_bits_truncate(0xF0);
        assert!(flags.contains(Cea861Flags::UNDERSCAN));
        assert!(flags.contains(Cea861Flags::BASIC_AUDIO));
        assert!(flags.contains(Cea861Flags::YCBCR_444));
        assert!(flags.contains(Cea861Flags::YCBCR_422));
    }

    #[test]
    fn test_malformed_data_block_warns() {
        let mut ext = [0u8; 128];
        // dtd_offset = 10: data block collection occupies bytes 4–9 (6 bytes).
        ext[2] = 10;
        // Write a block header claiming length = 8, which extends past the 6-byte collection.
        // tag = 1 (Audio), length = 8 → header = (1 << 5) | 8 = 0x28
        ext[4] = (1 << 5) | 8;

        let mut caps = DisplayCapabilities::default();
        let mut warnings = Vec::new();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut warnings);

        assert!(
            warnings
                .iter()
                .any(|w| (**w).downcast_ref::<EdidWarning>()
                    == Some(&EdidWarning::MalformedDataBlock))
        );
    }

    #[test]
    fn test_svd_parsing() {
        let mut ext = [0u8; 128];
        // dtd_offset = 8: data block collection occupies bytes 4–7
        ext[2] = 8;
        ext[3] = Cea861Flags::BASIC_AUDIO.bits();
        // Video Data Block: tag=2 (010), length=3  →  header = (2 << 5) | 3 = 0x43
        ext[4] = (2 << 5) | 3;
        ext[5] = 0x90; // VIC 16, native flag set  (0x80 | 16)
        ext[6] = 0x04; // VIC 4
        ext[7] = 0x01; // VIC 1

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        let cea = caps.get_extension_data::<Cea861Capabilities>(0x02).unwrap();
        assert_eq!(cea.vics, vec![(16, true), (4, false), (1, false)]);

        // All three VICs should produce entries in supported_modes
        assert_eq!(caps.supported_modes.len(), 3);
        assert!(caps.supported_modes.iter().any(|m| m.width == 1920
            && m.height == 1080
            && m.refresh_rate == Some(RefreshRate::integral(60))
            && !m.interlaced));
        assert!(caps.supported_modes.iter().any(|m| m.width == 1280
            && m.height == 720
            && m.refresh_rate == Some(RefreshRate::integral(60))));
        assert!(caps.supported_modes.iter().any(|m| m.width == 640
            && m.height == 480
            && m.refresh_rate == Some(RefreshRate::integral(60))));
    }

    #[test]
    fn test_svd_dedup() {
        // If a mode already appears in supported_modes it should not be added again.
        let mut ext = [0u8; 128];
        // dtd_offset = 7: collection = ext[4..7] = 3 bytes (1 header + 2 SVDs)
        ext[2] = 7;
        // Video Data Block with two SVDs for the same VIC
        ext[4] = (2 << 5) | 2; // tag=2, length=2
        ext[5] = 0x10; // VIC 16
        ext[6] = 0x90; // VIC 16, native — duplicate

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        let cea = caps.get_extension_data::<Cea861Capabilities>(0x02).unwrap();
        // Both SVD bytes are recorded in vics…
        assert_eq!(cea.vics.len(), 2);
        // …but supported_modes has only one entry for 1920×1080p60
        assert_eq!(
            caps.supported_modes
                .iter()
                .filter(|m| m.width == 1920
                    && m.height == 1080
                    && m.refresh_rate == Some(RefreshRate::integral(60)))
                .count(),
            1
        );
    }

    #[test]
    fn test_svd_byte_0x80_is_skipped() {
        // A standard SVD byte uses bit 7 as the native flag and bits 6-0 as the
        // VIC number. Byte 0x80 → native=true, vic=0. VIC 0 is reserved and must
        // be skipped: it should not appear in vics and must not produce a mode.
        let mut ext = [0u8; 128];
        ext[2] = 6;
        ext[4] = (2 << 5) | 1; // tag=2, length=1
        ext[5] = 0x80; // native flag set, vic=0

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        let cea = caps.get_extension_data::<Cea861Capabilities>(0x02).unwrap();
        assert!(cea.vics.is_empty());
        assert!(caps.supported_modes.is_empty());
    }

    /// Builds a minimal valid 18-byte DTD for 2560×1440@144Hz into `ext` starting at `offset`.
    ///
    /// Pixel clock = 583.92 MHz → stored as 58392 (little-endian units of 10 kHz).
    /// Htotal = 2720 (active 2560 + blank 160), Vtotal = 1481 (active 1440 + blank 41).
    /// 58392 * 10000 / (2720 * 1481) = 144.8 → rounds to 144.
    fn write_dtd_2560x1440_144(ext: &mut [u8; 128], offset: usize) {
        let clk: u16 = 58392; // 10 kHz units
        ext[offset] = (clk & 0xFF) as u8;
        ext[offset + 1] = (clk >> 8) as u8;
        // hactive=2560, hblank=160
        ext[offset + 2] = (2560 & 0xFF) as u8; // hactive low
        ext[offset + 3] = (160 & 0xFF) as u8; // hblank low
        ext[offset + 4] = ((2560 >> 4) & 0xF0) as u8; // hblank high nibble = 0 (160 < 256)
        // vactive=1440, vblank=41
        ext[offset + 5] = (1440 & 0xFF) as u8; // vactive low
        ext[offset + 6] = (41 & 0xFF) as u8; // vblank low
        ext[offset + 7] = ((1440 >> 4) & 0xF0) as u8; // vblank high nibble = 0 (41 < 256)
        // bytes 8-16: sync/image (leave zero — minimal)
        // byte 17: digital separate sync, both polarities positive
        ext[offset + 17] = 0x1E; // bit4=digital, bit3=separate, bit2=Vpos, bit1=Hpos
    }

    #[test]
    fn test_cea_dtd_parsed() {
        let mut ext = [0u8; 128];
        // No data block collection; DTDs start at offset 4.
        ext[2] = 4; // dtd_offset = 4
        write_dtd_2560x1440_144(&mut ext, 4);

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        assert!(
            caps.supported_modes.iter().any(|m| m.width == 2560
                && m.height == 1440
                && m.refresh_rate == Some(RefreshRate::integral(144))),
            "2560×1440@144 not found in supported_modes"
        );
    }

    /// Collects what the no-alloc path emits.
    #[derive(Default)]
    struct Collect(Vec<EdidWarning>, Vec<VideoMode>);
    impl ModeSink for Collect {
        fn push_mode(&mut self, mode: VideoMode) {
            self.1.push(mode);
        }
        fn push_warning(&mut self, w: EdidWarning) {
            self.0.push(w);
        }
    }

    fn edid_warnings(caps: &DisplayCapabilities) -> Vec<EdidWarning> {
        caps.iter_warnings()
            .filter_map(|w| (**w).downcast_ref::<EdidWarning>().copied())
            .collect()
    }

    #[test]
    fn test_cea_dtd_with_borders_warns_in_both_pipelines() {
        let mut ext = [0u8; 128];
        ext[2] = 4;
        write_dtd_2560x1440_144(&mut ext, 4);
        ext[4 + 15] = 8; // 8-pixel horizontal border: blanking 160 − 16 = 144
        let ambiguous = EdidWarning::DtdBordersAmbiguous {
            h_border: 8,
            v_border: 0,
        };

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());
        assert_eq!(edid_warnings(&caps), [ambiguous]);
        assert_eq!(caps.supported_modes[0].h_blank, Some(144));

        let mut sink = Collect::default();
        cea861_process_into_sink(&ext, &mut sink, &DecodeOptions::new());
        assert_eq!(sink.0, [ambiguous]);
        assert_eq!(sink.1[0].h_blank, Some(144));
    }

    #[test]
    fn test_vtb_ext_dtb_warnings_reach_both_pipelines() {
        // Data block collection with one extended-tag block (VTB-EXT, 0x03): version 1, one
        // Detailed Timing Block (1920×1080, blanking 280, border 4), no CVTs or STs.
        let mut ext = [0u8; 128];
        let payload_len = 1 + 4 + 18; // ext tag + header + DTB
        ext[4] = (7 << 5) | payload_len as u8;
        ext[5] = 0x03;
        ext[6..10].copy_from_slice(&[0x01, 0x01, 0x00, 0x00]);
        ext[10..28].copy_from_slice(&[
            0x02, 0x3A, 0x80, 0x18, 0x71, 0x38, 0x2D, 0x40, 88, 44, 0x45, 0x00, 0, 0, 0, 4, 0, 0x1E,
        ]);
        ext[2] = (4 + 1 + payload_len) as u8; // DTDs (none) start after the collection
        let ambiguous = EdidWarning::DtdBordersAmbiguous {
            h_border: 4,
            v_border: 0,
        };

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());
        assert_eq!(edid_warnings(&caps), [ambiguous]);
        assert!(caps.supported_modes.iter().any(|m| m.h_blank == Some(272)));

        let mut sink = Collect::default();
        cea861_process_into_sink(&ext, &mut sink, &DecodeOptions::new());
        assert_eq!(sink.0, [ambiguous]);
        assert!(sink.1.iter().any(|m| m.h_blank == Some(272)));
    }

    #[test]
    fn test_cea_dtd_zero_clock_skipped() {
        // A DTD slot whose first two bytes are 0x00 is a monitor descriptor and must be skipped.
        let mut ext = [0u8; 128];
        ext[2] = 4; // dtd_offset = 4 — no data blocks, DTDs start immediately
        // ext[4..22] stays all-zero → zero pixel clock → skipped

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        assert!(caps.supported_modes.is_empty());
    }

    #[test]
    fn test_speaker_allocation_parsed() {
        let mut ext = [0u8; 128];
        // Speaker Allocation Data Block: tag=4, length=3 → header = (4 << 5) | 3 = 0x83
        ext[2] = 8;
        ext[4] = (4 << 5) | 3;
        ext[5] = 0x07; // FL_FR | LFE1 | FC
        ext[6] = 0x00;
        ext[7] = 0x00;

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        let cea = caps.get_extension_data::<Cea861Capabilities>(0x02).unwrap();
        let sa = cea.speaker_allocation.unwrap();
        assert!(sa.channels.contains(SpeakerAllocationFlags::FL_FR));
        assert!(sa.channels.contains(SpeakerAllocationFlags::LFE1));
        assert!(sa.channels.contains(SpeakerAllocationFlags::FC));
    }

    #[test]
    fn test_y420_vdb_adds_modes_and_vics() {
        let mut ext = [0u8; 128];
        // Y420 VDB extended block: outer tag=7, length=2 → header = (7 << 5) | 2 = 0xE2
        // payload: extended tag 0x0E, VIC 96 (2160p50)
        ext[2] = 8;
        ext[4] = (7 << 5) | 2;
        ext[5] = 0x0E; // EXT_TAG_Y420_VIDEO
        ext[6] = 96; // VIC 96

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        let cea = caps.get_extension_data::<Cea861Capabilities>(0x02).unwrap();
        assert_eq!(cea.y420_vics, vec![96]);
        // VIC 96 = 3840×2160@50Hz — should appear in supported_modes
        assert!(caps.supported_modes.iter().any(|m| m.width == 3840
            && m.height == 2160
            && m.refresh_rate == Some(RefreshRate::integral(50))));
    }

    #[test]
    fn test_extended_svd_vic193() {
        // Extended SVD encoding for VIC 193 (5120×2160p120):
        // first byte D6:D0 = 0 (native flag in D7), second byte = 193.
        let mut ext = [0u8; 128];
        // Video Data Block: tag=2, length=2 → header = 0x42
        ext[2] = 8;
        ext[4] = (2 << 5) | 2;
        ext[5] = 0x00; // bits 6:0 = 0 → extended SVD; native = false
        ext[6] = 193; // full VIC

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        let cea = caps.get_extension_data::<Cea861Capabilities>(0x02).unwrap();
        assert_eq!(cea.vics, vec![(193, false)]);
        // VIC 193 = 5120×2160p120 — should appear in supported_modes
        assert!(caps.supported_modes.iter().any(|m| m.width == 5120
            && m.height == 2160
            && m.refresh_rate == Some(RefreshRate::integral(120))));
    }

    #[test]
    fn test_extended_svd_native_flag() {
        // Extended SVD with native flag set: first byte = 0x80 (native=true, VIC bits=0).
        let mut ext = [0u8; 128];
        ext[2] = 8;
        ext[4] = (2 << 5) | 2;
        ext[5] = 0x80; // native=true, bits 6:0 = 0 → extended SVD
        ext[6] = 196; // VIC 196

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        let cea = caps.get_extension_data::<Cea861Capabilities>(0x02).unwrap();
        assert_eq!(cea.vics, vec![(196, true)]);
    }

    #[test]
    fn test_extended_svd_truncated_is_skipped() {
        // Extended SVD marker as the very last byte — second byte missing.
        let mut ext = [0u8; 128];
        ext[2] = 6;
        ext[4] = (2 << 5) | 1; // tag=2, length=1 — only the marker byte, no VIC byte
        ext[5] = 0x00; // extended SVD marker with no following byte

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        let cea = caps.get_extension_data::<Cea861Capabilities>(0x02).unwrap();
        assert!(cea.vics.is_empty());
    }

    #[test]
    fn test_y420_capability_map_stored() {
        let mut ext = [0u8; 128];
        // Y420 Capability Map: outer tag=7, length=2 → 0xE2; ext tag 0x0F, bitmap 0b00000101
        ext[2] = 8;
        ext[4] = (7 << 5) | 2;
        ext[5] = 0x0F; // EXT_TAG_Y420_CAPABILITY_MAP
        ext[6] = 0b0000_0101;

        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[&ext], &mut caps, &mut Vec::new());

        let cea = caps.get_extension_data::<Cea861Capabilities>(0x02).unwrap();
        assert_eq!(cea.y420_capability_map, vec![0b0000_0101]);
    }

    /// An extension block whose data block collection is one HDMI Audio Data Block with
    /// `payload` after the extended tag.
    fn ext_with_hdmi_audio(payload: &[u8]) -> [u8; 128] {
        let mut ext = [0u8; 128];
        ext[4] = (7 << 5) | (1 + payload.len()) as u8;
        ext[5] = 0x12; // EXT_TAG_HDMI_AUDIO
        ext[6..6 + payload.len()].copy_from_slice(payload);
        ext[2] = (6 + payload.len()) as u8;
        ext
    }

    fn hdmi_audio(ext: &[u8; 128]) -> (HdmiAudioBlock, Vec<EdidWarning>) {
        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[ext], &mut caps, &mut Vec::new());
        let block = caps
            .get_extension_data::<Cea861Capabilities>(0x02)
            .unwrap()
            .hdmi_audio
            .clone()
            .unwrap();
        (block, edid_warnings(&caps))
    }

    #[test]
    fn test_hdmi_audio_block_multi_stream_and_3d_audio() {
        // Byte 0: max streams − 1 = 1, MS NonMixed. Byte 1: one 3D audio descriptor.
        // 3D descriptor: L-PCM (1), 10 channels (9), 48 + 96 kHz, 16 + 24 bit.
        // Speaker allocation: FL/FR + FC, layout 22.2 (2 << 4).
        let ext = ext_with_hdmi_audio(&[
            0x05, 0x01, // header bytes
            0x01, 0x09, 0x14, 0x05, // 3D audio descriptor
            0x05, 0x00, 0x00, 0x20, // 3D speaker allocation
        ]);
        let (block, warnings) = hdmi_audio(&ext);
        assert!(warnings.is_empty());
        assert_eq!((block.max_streams, block.ms_non_mixed), (2, true));
        assert!(block.multi_stream());
        let d = &block.audio_3d_descriptors[..];
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].format, d[0].max_channels), (AudioFormat::Lpcm, 10));
        assert_eq!(
            d[0].sample_rates,
            AudioSampleRates::HZ_48000 | AudioSampleRates::HZ_96000
        );
        assert_eq!(
            d[0].format_info,
            AudioFormatInfo::Lpcm {
                depth_16: true,
                depth_20: false,
                depth_24: true
            }
        );
        let sa = block.speaker_allocation_3d.unwrap();
        assert_eq!(sa.channels, display_types::cea861::Audio3dChannels::Ch22_2);
        assert_eq!(
            sa.speakers.channels,
            SpeakerAllocationFlags::FL_FR | SpeakerAllocationFlags::FC
        );
    }

    #[test]
    fn test_hdmi_audio_block_single_stream_no_3d() {
        let (block, warnings) = hdmi_audio(&ext_with_hdmi_audio(&[0x00, 0x00]));
        assert!(warnings.is_empty());
        assert_eq!(block.max_streams, 1);
        assert!(!block.multi_stream());
        assert!(block.audio_3d_descriptors.is_empty());
        assert!(block.speaker_allocation_3d.is_none());
        // A block with only the first byte is valid too.
        let (block, _) = hdmi_audio(&ext_with_hdmi_audio(&[0x03]));
        assert_eq!(block.max_streams, 4);
    }

    #[test]
    fn test_hdmi_audio_block_warnings() {
        // MS NonMixed with a single stream.
        let (_, w) = hdmi_audio(&ext_with_hdmi_audio(&[0x04, 0x00]));
        assert_eq!(w, [EdidWarning::HdmiAudioNonMixedWithoutMultiStream]);
        // Two descriptors claimed, room for one plus nothing: none taken.
        let (block, w) = hdmi_audio(&ext_with_hdmi_audio(&[0x00, 0x02, 0x01, 0x01, 0x04, 0x01]));
        assert!(block.audio_3d_descriptors.is_empty());
        assert_eq!(
            w,
            [EdidWarning::HdmiAudio3dDescriptorsExceedBlock {
                claimed: 2,
                available_bytes: 4
            }]
        );
        // A 3D descriptor in AC-3 (2): skipped; the L-PCM one is kept.
        let (block, w) = hdmi_audio(&ext_with_hdmi_audio(&[
            0x00, 0x02, 0x02, 0x05, 0x04, 0x00, 0x01, 0x01, 0x04, 0x01, 0x01, 0x00, 0x00, 0x10,
        ]));
        assert_eq!(w, [EdidWarning::HdmiAudio3dUnsupportedFormat { code: 2 }]);
        assert_eq!(block.audio_3d_descriptors.len(), 1);
        assert_eq!(
            block.speaker_allocation_3d.map(|a| a.channels),
            Some(display_types::cea861::Audio3dChannels::Ch10_2)
        );
    }
}
