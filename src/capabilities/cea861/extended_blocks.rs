use crate::capabilities::base::timings::build_dtd_mode;
use crate::capabilities::displayid::decode_type_vii_descriptor_to_mode;
use crate::model::capabilities::VideoMode;
use crate::model::diagnostics::EdidWarning;
use crate::model::options::DecodeOptions;
use crate::model::prelude::Vec;
pub use display_types::cea861::colorimetry::{ColorimetryBlock, ColorimetryFlags};
pub use display_types::cea861::hdmi_forum::{
    HdmiDscMaxSlices, HdmiForumDsc, HdmiForumFrl, HdmiForumSinkCap,
};
pub use display_types::cea861::hdr::{HdrDynamicMetadataDescriptor, HdrEotf, HdrStaticMetadata};
pub use display_types::cea861::misc::{InfoFrameDescriptor, VendorSpecificBlock, infoframe_type};
pub use display_types::cea861::speaker::{
    RoomConfigurationBlock, SpeakerAllocation, SpeakerAllocationFlags, SpeakerAllocationFlags2,
    SpeakerAllocationFlags3, SpeakerLocationEntry,
};
pub use display_types::cea861::vesa_dddb::VesaDisplayDeviceBlock;
pub use display_types::cea861::vesa_transfer::{DtcPointEncoding, VesaTransferCharacteristic};
pub use display_types::cea861::video_capability::{VideoCapability, VideoCapabilityFlags};
pub use display_types::cea861::vtdb::{
    T7VtdbBlock, T8VtdbBlock, T10VtdbBlock, T10VtdbEntry, VtbExtBlock,
};

/// Extended tag codes used in CEA Extended Tag Data Blocks (outer tag `0x07`).
/// The first byte of the block payload is the extended tag.
pub(super) const EXT_TAG_VSVDB: u8 = 0x01;
pub(super) const EXT_TAG_VESA_DDDB: u8 = 0x02;
pub(super) const EXT_TAG_VTB_EXT: u8 = 0x03;
pub(super) const EXT_TAG_VIDEO_CAPABILITY: u8 = 0x00;
pub(super) const EXT_TAG_VSADB: u8 = 0x11;
pub(super) const EXT_TAG_T7VTDB: u8 = 0x22;
pub(super) const EXT_TAG_T8VTDB: u8 = 0x23;
pub(super) const EXT_TAG_T10VTDB: u8 = 0x2A;
pub(super) const EXT_TAG_HF_EEODB: u8 = 0x78;
pub(super) const EXT_TAG_HF_SCDB: u8 = 0x79;
pub(super) const EXT_TAG_COLORIMETRY: u8 = 0x05;
pub(super) const EXT_TAG_HDR_STATIC_METADATA: u8 = 0x06;
pub(super) const EXT_TAG_HDR_DYNAMIC_METADATA: u8 = 0x07;
pub(super) const EXT_TAG_VIDEO_FORMAT_PREFERENCE: u8 = 0x0D;
pub(super) const EXT_TAG_Y420_VIDEO: u8 = 0x0E;
pub(super) const EXT_TAG_Y420_CAPABILITY_MAP: u8 = 0x0F;
pub(super) const EXT_TAG_HDMI_AUDIO: u8 = 0x12;
pub(super) const EXT_TAG_ROOM_CONFIGURATION: u8 = 0x13;
pub(super) const EXT_TAG_SPEAKER_LOCATION: u8 = 0x14;
pub(super) const EXT_TAG_INFOFRAME: u8 = 0x20;

/// All extended tag codes that are implemented by this library.
///
/// This list must be kept in sync with the match arms in `mod.rs`.
/// The `test_all_extended_tags_accounted_for` test uses it to verify that the
/// union of implemented and reserved tags covers every value 0x00–0xFF.
#[cfg(test)]
pub(super) const IMPLEMENTED_EXTENDED_TAGS: &[u8] = &[
    EXT_TAG_VIDEO_CAPABILITY,        // 0x00
    EXT_TAG_VSVDB,                   // 0x01
    EXT_TAG_VESA_DDDB,               // 0x02
    EXT_TAG_VTB_EXT,                 // 0x03
    EXT_TAG_COLORIMETRY,             // 0x05
    EXT_TAG_HDR_STATIC_METADATA,     // 0x06
    EXT_TAG_HDR_DYNAMIC_METADATA,    // 0x07
    EXT_TAG_VIDEO_FORMAT_PREFERENCE, // 0x0D
    EXT_TAG_Y420_VIDEO,              // 0x0E
    EXT_TAG_Y420_CAPABILITY_MAP,     // 0x0F
    EXT_TAG_VSADB,                   // 0x11
    EXT_TAG_HDMI_AUDIO,              // 0x12
    EXT_TAG_ROOM_CONFIGURATION,      // 0x13
    EXT_TAG_SPEAKER_LOCATION,        // 0x14
    EXT_TAG_INFOFRAME,               // 0x20
    EXT_TAG_T7VTDB,                  // 0x22
    EXT_TAG_T8VTDB,                  // 0x23
    EXT_TAG_T10VTDB,                 // 0x2A
    EXT_TAG_HF_EEODB,                // 0x78
    EXT_TAG_HF_SCDB,                 // 0x79
];

/// Extended tag ranges that are explicitly reserved or deferred.
///
/// Each entry is an inclusive `(first, last)` range. These are listed here so
/// that `test_all_extended_tags_accounted_for` can confirm there are no gaps.
/// When the CTA spec assigns a new tag, remove it from the appropriate range,
/// add an `EXT_TAG_*` constant, and implement the block (or add it to
/// `IMPLEMENTED_EXTENDED_TAGS` as a no-op stub).
#[cfg(test)]
pub(super) const RESERVED_EXTENDED_TAG_RANGES: &[(u8, u8)] = &[
    (0x04, 0x04), // HDMI Forum Video Data Block — no public structure without Forum membership
    (0x08, 0x0C), // Reserved for video-related blocks (CTA-861-H Table 62)
    (0x10, 0x10), // Reserved for CTA Miscellaneous Audio Fields (MAF)
    (0x15, 0x1F), // Reserved for audio-related blocks
    (0x21, 0x21), // Reserved
    (0x24, 0x29), // Reserved
    (0x2B, 0x77), // Reserved
    (0x7A, 0xFF), // Reserved (including HDMI Forum reserved range 0x7A–0x7F)
];

// ---------------------------------------------------------------------------
// Video Capability Data Block (extended tag 0x00)
// ---------------------------------------------------------------------------

pub(super) fn parse_video_capability(block_data: &[u8]) -> Option<VideoCapability> {
    // block_data[0] = extended tag; payload starts at [1].
    let b = *block_data.get(1)?;
    Some(VideoCapability::new(
        VideoCapabilityFlags::from_bits_truncate(b & 0xC0),
        (b >> 4) & 0x03,
        (b >> 2) & 0x03,
        b & 0x03,
    ))
}

// ---------------------------------------------------------------------------
// Colorimetry Data Block (extended tag 0x05)
// ---------------------------------------------------------------------------

pub(super) fn parse_colorimetry(block_data: &[u8]) -> Option<ColorimetryBlock> {
    // block_data[0] = extended tag; payload is [1] and optionally [2].
    let colorimetry = ColorimetryFlags::from_bits_truncate(*block_data.get(1)?);
    let metadata_profiles = block_data.get(2).map_or(0, |&b| b & 0x0F);
    Some(ColorimetryBlock::new(colorimetry, metadata_profiles))
}

// ---------------------------------------------------------------------------
// HDR Static Metadata Data Block (extended tag 0x06)
// ---------------------------------------------------------------------------

/// Decodes the `50 × 2^(raw/32)` luminance encoding used in the HDR block.
fn decode_luminance(raw: u8) -> f32 {
    50.0 * (raw as f32 / 32.0).exp2()
}

pub(super) fn parse_hdr_static_metadata(block_data: &[u8]) -> Option<HdrStaticMetadata> {
    // block_data[0] = extended tag; EOTF at [1], SMD at [2], luminance at [3-5].
    let eotf = HdrEotf::from_bits_truncate(*block_data.get(1)?);
    let static_metadata_descriptors = *block_data.get(2).unwrap_or(&0);

    let max_luminance = block_data.get(3).map(|&b| decode_luminance(b));
    let max_fall = block_data.get(4).map(|&b| decode_luminance(b));
    let min_luminance = block_data
        .get(5)
        .and_then(|&b| max_luminance.map(|max| max * (b as f32 / 255.0).powi(2) / 100.0));

    Some(HdrStaticMetadata::new(
        eotf,
        static_metadata_descriptors,
        max_luminance,
        max_fall,
        min_luminance,
    ))
}

// ---------------------------------------------------------------------------
// Speaker Allocation Data Block (standard tag 0x04)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// VESA Display Transfer Characteristic Data Block (standard tag 0x05)
// ---------------------------------------------------------------------------

pub(super) fn parse_vesa_transfer_characteristic(
    block_data: &[u8],
) -> Option<VesaTransferCharacteristic> {
    let first = *block_data.first()?;
    let encoding = match (first >> 6) & 0x03 {
        0x00 => DtcPointEncoding::Bits8,
        0x01 => DtcPointEncoding::Bits10,
        0x02 => DtcPointEncoding::Bits12,
        _ => return None, // reserved
    };

    let data = &block_data[1..];
    let points = match encoding {
        DtcPointEncoding::Bits8 => data.iter().map(|&b| b as f32 / 255.0).collect(),
        DtcPointEncoding::Bits10 => {
            let mut pts = Vec::new();
            let mut i = 0;
            while i + 5 <= data.len() {
                // 5 bytes encode 4 × 10-bit values, packed MSB-first.
                let [b0, b1, b2, b3, b4] = [
                    data[i] as u16,
                    data[i + 1] as u16,
                    data[i + 2] as u16,
                    data[i + 3] as u16,
                    data[i + 4] as u16,
                ];
                pts.push(((b0 << 2) | (b1 >> 6)) as f32 / 1023.0);
                pts.push((((b1 & 0x3F) << 4) | (b2 >> 4)) as f32 / 1023.0);
                pts.push((((b2 & 0x0F) << 6) | (b3 >> 2)) as f32 / 1023.0);
                pts.push((((b3 & 0x03) << 8) | b4) as f32 / 1023.0);
                i += 5;
            }
            pts
        }
        DtcPointEncoding::Bits12 => {
            let mut pts = Vec::new();
            let mut i = 0;
            while i + 3 <= data.len() {
                // 3 bytes encode 2 × 12-bit values, packed MSB-first.
                let [b0, b1, b2] = [data[i] as u16, data[i + 1] as u16, data[i + 2] as u16];
                pts.push(((b0 << 4) | (b1 >> 4)) as f32 / 4095.0);
                pts.push((((b1 & 0x0F) << 8) | b2) as f32 / 4095.0);
                i += 3;
            }
            pts
        }
        // Reserved encoding filtered out above; unreachable in practice.
        _ => return None,
    };

    Some(VesaTransferCharacteristic::new(encoding, points))
}

pub(super) fn parse_speaker_allocation(block_data: &[u8]) -> Option<SpeakerAllocation> {
    let channels = SpeakerAllocationFlags::from_bits_truncate(*block_data.first()?);
    let channels_2 =
        SpeakerAllocationFlags2::from_bits_truncate(block_data.get(1).copied().unwrap_or(0));
    let channels_3 =
        SpeakerAllocationFlags3::from_bits_truncate(block_data.get(2).copied().unwrap_or(0));
    Some(SpeakerAllocation::new(channels, channels_2, channels_3))
}

// ---------------------------------------------------------------------------
// HDR Dynamic Metadata Data Block (extended tag 0x07)
// ---------------------------------------------------------------------------

pub(super) fn parse_hdr_dynamic_metadata(block_data: &[u8]) -> Vec<HdrDynamicMetadataDescriptor> {
    // block_data[0] = extended tag; descriptors start at [1].
    // Each descriptor is one byte (type + version); type-specific trailing
    // bytes are not parsed — we advance one byte at a time.
    let mut out = Vec::new();
    // Each descriptor may have additional application-specific bytes.
    // Without type-specific knowledge of their length we cannot skip them,
    // so we parse only the first descriptor rather than risk misalignment.
    if let Some(&b) = block_data.get(1) {
        out.push(HdrDynamicMetadataDescriptor::new(b & 0x3F, (b >> 6) & 0x03));
    }
    out
}

// ---------------------------------------------------------------------------
// Video Format Preference Data Block (extended tag 0x0D)
// ---------------------------------------------------------------------------

/// Returns the raw Short Video References (SVRs) from a Video Format Preference
/// Data Block (extended tag `0x0D`).
///
/// Each byte encodes a preferred video format:
/// - `1`–`127`: references a VIC in the Video Data Block.
/// - `129`–`144`: references a Detailed Timing Descriptor (`DTD[n-128]`).
/// - `145`–`160`: references an entry in the YCbCr 4:2:0 Video Data Block.
/// - All other values are reserved.
pub(super) fn parse_video_format_preferences(block_data: &[u8]) -> Vec<u8> {
    // block_data[0] = extended tag; SVRs start at [1].
    block_data[1..].to_vec()
}

// ---------------------------------------------------------------------------
// YCbCr 4:2:0 Video Data Block (extended tag 0x0E)
// ---------------------------------------------------------------------------

/// Returns VIC numbers from a YCbCr 4:2:0 Video Data Block (extended tag `0x0E`).
///
/// These VICs are **only** supported in the YCbCr 4:2:0 colour format.
/// Uses the same Short Video Descriptor encoding as the standard Video Data Block,
/// including the CTA-861-G extended SVD format for VICs 128–255 (a byte with
/// bits 6:0 == 0 followed by a byte with the full VIC number). VIC 0 is reserved
/// and is excluded from the returned list.
pub(super) fn parse_y420_vdb(block_data: &[u8]) -> Vec<u8> {
    // block_data[0] = extended tag; SVDs start at [1].
    let svds = &block_data[1..];
    let mut out = Vec::new();
    let mut j = 0;
    while j < svds.len() {
        let vic_low = svds[j] & 0x7F;
        let vic = if vic_low == 0 {
            j += 1;
            match svds.get(j).copied() {
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
        out.push(vic);
    }
    out
}

// ---------------------------------------------------------------------------
// YCbCr 4:2:0 Capability Map Data Block (extended tag 0x0F)
// ---------------------------------------------------------------------------

/// Raw bitmap from a YCbCr 4:2:0 Capability Map Data Block (extended tag `0x0F`).
///
/// Bit `n` (0-indexed across bytes, LSB-first within each byte) corresponds to
/// the `(n+1)`-th Short Video Descriptor in the standard Video Data Block.
/// A set bit means that mode **also** supports YCbCr 4:2:0.
///
/// An empty `Vec` means all modes in the Video Data Block support 4:2:0
/// (per the CTA-861 spec when this block is absent).
pub(super) fn parse_y420_capability_map(block_data: &[u8]) -> Vec<u8> {
    // block_data[0] = extended tag; bitmap bytes start at [1].
    block_data[1..].to_vec()
}

// ---------------------------------------------------------------------------
// Room Configuration Data Block (extended tag 0x13)
// ---------------------------------------------------------------------------

pub(super) fn parse_room_configuration(block_data: &[u8]) -> Option<RoomConfigurationBlock> {
    // block_data[0] = extended tag; payload byte at [1].
    let b = *block_data.get(1)?;
    Some(RoomConfigurationBlock::new(b & 0x1F, b & 0x40 != 0))
}

// ---------------------------------------------------------------------------
// Speaker Location Data Block (extended tag 0x14)
// ---------------------------------------------------------------------------

pub(super) fn parse_speaker_location(block_data: &[u8]) -> Vec<SpeakerLocationEntry> {
    // block_data[0] = extended tag; entries start at [1], each 2 bytes.
    block_data[1..]
        .chunks_exact(2)
        .map(|c| SpeakerLocationEntry::new(c[0], c[1]))
        .collect()
}

// ---------------------------------------------------------------------------
// InfoFrame Data Block (extended tag 0x20)
// ---------------------------------------------------------------------------

pub(super) fn parse_infoframe_db(block_data: &[u8]) -> Vec<InfoFrameDescriptor> {
    // block_data[0] = extended tag; SIDs start at [1].
    // Each SID: bits 7:5 = additional bytes after this byte, bits 4:0 = type.
    let mut out = Vec::new();
    let payload = &block_data[1..];
    let mut i = 0;

    while i < payload.len() {
        let b = payload[i];
        let type_code = b & 0x1F;
        let extra = ((b >> 5) & 0x07) as usize;
        i += 1;

        let vendor_oui = if type_code == infoframe_type::VENDOR_SPECIFIC && extra >= 3 {
            let oui = ((payload.get(i).copied().unwrap_or(0) as u32) << 16)
                | ((payload.get(i + 1).copied().unwrap_or(0) as u32) << 8)
                | (payload.get(i + 2).copied().unwrap_or(0) as u32);
            Some(oui)
        } else {
            None
        };

        out.push(InfoFrameDescriptor::new(type_code, vendor_oui));

        // Advance past the extra bytes, clamping to remaining payload.
        i += extra.min(payload.len().saturating_sub(i));
    }

    out
}

// ---------------------------------------------------------------------------
// Vendor-Specific Video Data Block (extended tag 0x01)
// Vendor-Specific Audio Data Block (extended tag 0x11)
// ---------------------------------------------------------------------------

/// Parse a VSVDB or VSADB payload (`block_data` starts after the extended tag byte).
///
/// Returns `None` if the payload is shorter than the 3-byte OUI minimum.
pub(super) fn parse_vendor_specific_block(block_data: &[u8]) -> Option<VendorSpecificBlock> {
    if block_data.len() < 3 {
        return None;
    }
    let oui =
        ((block_data[2] as u32) << 16) | ((block_data[1] as u32) << 8) | (block_data[0] as u32);
    let payload = block_data[3..].to_vec();
    Some(VendorSpecificBlock::new(oui, payload))
}

// ---------------------------------------------------------------------------
// DisplayID Type VII Video Timing Data Block (extended tag 0x22)
// ---------------------------------------------------------------------------

/// Parse a T7VTDB payload (`block_data` starts after the extended tag byte, at the descriptor
/// header). Returns `None` for payloads shorter than 22 bytes, null/degenerate descriptors,
/// or geometry whose reduced refresh rate doesn't fit in `RefreshRate`.
///
/// The CTA wrapper occupies the first two bytes; the 20-byte body that follows is decoded
/// by the shared DisplayID 2.x Type VII descriptor decoder.
///
/// CTA wrapper layout:
/// - `[0]`: `T7_M[7:5] | DSC_PT[4] | reserved[3] | Block_Rev[2:0]`
/// - `[1]`: `F37[7]=0 | T7Y420[6] | T7HSP[5] | T7VSP[4] | reserved[3:0]`
///
/// `T7HSP` / `T7VSP` mirror the polarity bits already encoded in the descriptor body
/// (bit 15 of the H/V Front Porch fields) and are not read here — the descriptor is the
/// authoritative source.
pub(super) fn parse_t7vtdb(block_data: &[u8]) -> Option<T7VtdbBlock> {
    if block_data.len() < 22 {
        return None;
    }

    let version = block_data[0] & 0x07;
    let y420 = (block_data[1] >> 6) & 1 != 0;

    let descriptor: &[u8; 20] = block_data[2..22]
        .try_into()
        .expect("slice length checked above");
    let mode = decode_type_vii_descriptor_to_mode(descriptor, 0)?;

    Some(T7VtdbBlock::new(version, mode, y420))
}

// ---------------------------------------------------------------------------
// DisplayID Type VIII Video Timing Data Block (extended tag 0x23)
// ---------------------------------------------------------------------------

use display_types::cea861::dmt_to_mode;

/// Parse a T8VTDB payload (`block_data` starts after the extended tag byte).
///
/// Returns `None` for an empty payload or a non-DMT `Code_Type` (CTA-861 only
/// defines `Code_Type = 0x00` for DMT).
pub(super) fn parse_t8vtdb(block_data: &[u8]) -> Option<T8VtdbBlock> {
    // block_data[0]: Code_Type[7:6] | T8Y420[5] | F34[4]=0 | TCS[3] | Block_Rev[2:0]
    let header = *block_data.first()?;
    let code_type = (header >> 6) & 0x03;
    if code_type != 0x00 {
        return None; // only DMT is defined for CTA-861
    }
    let y420 = (header >> 5) & 1 != 0;
    let tcs = (header >> 3) & 1 != 0; // false = 1-byte codes, true = 2-byte codes
    let version = header & 0x07;

    let payload = &block_data[1..];
    let mut codes: Vec<u16> = Vec::new();
    let mut timings: Vec<VideoMode> = Vec::new();

    if tcs {
        // 2-byte codes stored LSB-first; trailing incomplete pairs are ignored.
        let mut i = 0;
        while i + 2 <= payload.len() {
            let code = u16::from_le_bytes([payload[i], payload[i + 1]]);
            codes.push(code);
            if let Some(mode) = dmt_to_mode(code) {
                timings.push(mode);
            }
            i += 2;
        }
    } else {
        for &byte in payload {
            let code = byte as u16;
            codes.push(code);
            if let Some(mode) = dmt_to_mode(code) {
                timings.push(mode);
            }
        }
    }

    Some(T8VtdbBlock::new(version, y420, codes, timings))
}

// ---------------------------------------------------------------------------
// DisplayID Type X Video Timing Data Block (extended tag 0x2A)
// ---------------------------------------------------------------------------

/// Parse a T10VTDB payload (`block_data` starts after the extended tag byte).
///
/// Returns `None` for an empty payload or an invalid descriptor-size extension
/// (`M > 2`). Descriptors that do not divide evenly into the payload are ignored.
///
/// Layout (CTA-861-H Table 109–110 / DisplayID 2.x Type X):
/// - `block_data[0]`: `rev` byte — bits[6:4] = M (0→6 B, 1→7 B, 2→8 B per descriptor)
/// - `block_data[1..]`: array of N descriptors of `6 + M` bytes each
///
/// Per-descriptor layout:
/// - `[0]`: flags — `YCC420[7]`, `Stereo[6:5]`, `VR_HB[4]`, `EVS[3]`, `Formula[2:0]`
/// - `[1..2]`: H active − 1 (LE u16)
/// - `[3..4]`: V active − 1 (LE u16)
/// - `[5]`: refresh rate LSB (stored − 1, i.e. actual = `x[5] + 1`)
/// - `[6]` (M≥1): bits[1:0] = refresh rate high 2 bits
pub(super) fn parse_t10vtdb(block_data: &[u8]) -> Option<T10VtdbBlock> {
    let rev = *block_data.first()?;
    let m = (rev >> 4) & 0x07;
    if m > 2 {
        return None; // invalid per spec
    }
    let sz = 6 + m as usize;

    let payload = &block_data[1..];
    let mut entries = Vec::new();
    let mut i = 0;

    while i + sz <= payload.len() {
        let d = &payload[i..i + sz];

        let flags = d[0];
        let y420 = (flags >> 7) & 1 != 0;

        let width = u16::from_le_bytes([d[1], d[2]]).saturating_add(1);
        let height = u16::from_le_bytes([d[3], d[4]]).saturating_add(1);

        let refresh_lsb = d[5] as u16;
        let refresh_hz = if m >= 1 {
            let msb = (d[6] & 0x03) as u16;
            (refresh_lsb | (msb << 8)) + 1
        } else {
            refresh_lsb + 1
        };

        entries.push(T10VtdbEntry::new(width, height, refresh_hz, y420));

        i += sz;
    }

    Some(T10VtdbBlock::new(entries))
}

// ---------------------------------------------------------------------------
// HDMI Forum EDID Extension Override Data Block (extended tag 0x78)
// ---------------------------------------------------------------------------

/// Parse the HF-EEODB payload (`block_data` starts after the extended tag byte).
///
/// Returns the EDID extension block count override, or `None` if the payload
/// is empty or the count is zero (meaningless).
///
/// The HF-EEODB is defined in HDMI 2.1 section 10.3.6. It must be the first
/// data block of Block 1 and overrides the extension count in the base EDID
/// header for HDMI 2.1 sinks whose full E-EDID exceeds what the 1-byte base
/// count can represent.
pub(super) fn parse_hf_eeodb(block_data: &[u8]) -> Option<u8> {
    let count = *block_data.first()?;
    if count == 0 {
        return None;
    }
    Some(count)
}

// ---------------------------------------------------------------------------
// HDMI Forum Sink Capability Data Block (extended tag 0x79)
// ---------------------------------------------------------------------------

/// Parses an HDMI Forum Sink Capability Data Structure (SCDS) from a byte slice
/// where `scds[0]` is the Version byte.
///
/// Used by both [`parse_hf_scdb`] (which skips two reserved prefix bytes first)
/// and [`parse_hf_vsdb`] (where the SCDS starts immediately after the OUI).
///
/// Returns `None` if `scds` is shorter than the 4 mandatory bytes.
pub(super) fn parse_hdmi_scds(scds: &[u8]) -> Option<HdmiForumSinkCap> {
    if scds.len() < 4 {
        return None;
    }

    let version = scds[0];
    let max_tmds_rate_mhz = (scds[1] as u16) * 5;

    let scdc = scds[2];
    let scdc_present = (scdc >> 7) & 1 != 0;
    let rr_capable = (scdc >> 6) & 1 != 0;
    let cable_status = (scdc >> 5) & 1 != 0;
    let ccbpci = (scdc >> 4) & 1 != 0;
    let lte_340mcsc_scramble = (scdc >> 3) & 1 != 0;
    let independent_view_3d = (scdc >> 2) & 1 != 0;
    let dual_view_3d = (scdc >> 1) & 1 != 0;
    let osd_disparity_3d = scdc & 1 != 0;

    let frl_dc = scds[3];
    let max_frl_rate = HdmiForumFrl::from_raw((frl_dc >> 4) & 0x0F);
    let uhd_vic = (frl_dc >> 3) & 1 != 0;
    let dc_48bit_420 = (frl_dc >> 2) & 1 != 0;
    let dc_36bit_420 = (frl_dc >> 1) & 1 != 0;
    let dc_30bit_420 = frl_dc & 1 != 0;

    // Optional extended section: feature flags (byte 4) + VRR range (bytes 5–6).
    let (
        fapa_end_extended,
        qms,
        m_delta,
        fva,
        allm,
        fapa_start_location,
        neg_mvrr,
        vrr_min_hz,
        vrr_max_hz,
    ) = if let Some(&ext) = scds.get(4) {
        let fapa_end_extended = (ext >> 7) & 1 != 0;
        let qms = (ext >> 6) & 1 != 0;
        let m_delta = (ext >> 5) & 1 != 0;
        // bit 4 = CinemaVRR (deprecated, ignored)
        let neg_mvrr = (ext >> 3) & 1 != 0;
        let fva = (ext >> 2) & 1 != 0;
        let allm = (ext >> 1) & 1 != 0;
        let fapa_start_location = ext & 1 != 0;

        let vrr = scds.get(5).and_then(|&b5| {
            scds.get(6).map(|&b6| {
                let vrr_min = b5 & 0x3F;
                let vrr_max = ((b5 >> 6) as u16) << 8 | b6 as u16;
                (vrr_min, vrr_max)
            })
        });
        let (vrr_min_hz, vrr_max_hz) = match vrr {
            Some((min, max)) => (Some(min), Some(max)),
            None => (None, None),
        };
        (
            fapa_end_extended,
            qms,
            m_delta,
            fva,
            allm,
            fapa_start_location,
            neg_mvrr,
            vrr_min_hz,
            vrr_max_hz,
        )
    } else {
        (false, false, false, false, false, false, false, None, None)
    };

    // Optional DSC section (bytes 7–9).
    let dsc = scds.get(7).and_then(|&dsc_flags| {
        let dsc_frl_slices = *scds.get(8)?;
        let chunk_raw = scds.get(9).map_or(0, |&b| b & 0x3F);

        let max_chunk_bytes = if chunk_raw == 0 {
            0
        } else {
            1024 * (1 + chunk_raw as u32)
        };

        Some(HdmiForumDsc::new(
            (dsc_flags >> 7) & 1 != 0,
            (dsc_flags >> 6) & 1 != 0,
            (dsc_flags >> 5) & 1 != 0,
            (dsc_flags >> 4) & 1 != 0,
            (dsc_flags >> 3) & 1 != 0,
            (dsc_flags >> 1) & 1 != 0,
            dsc_flags & 1 != 0,
            HdmiForumFrl::from_raw((dsc_frl_slices >> 4) & 0x0F),
            HdmiDscMaxSlices::from_raw(dsc_frl_slices & 0x0F),
            max_chunk_bytes,
        ))
    });

    Some(HdmiForumSinkCap::new(
        version,
        max_tmds_rate_mhz,
        scdc_present,
        rr_capable,
        cable_status,
        ccbpci,
        lte_340mcsc_scramble,
        independent_view_3d,
        dual_view_3d,
        osd_disparity_3d,
        max_frl_rate,
        uhd_vic,
        dc_48bit_420,
        dc_36bit_420,
        dc_30bit_420,
        fapa_end_extended,
        qms,
        m_delta,
        fva,
        allm,
        fapa_start_location,
        neg_mvrr,
        vrr_min_hz,
        vrr_max_hz,
        dsc,
    ))
}

/// Parses an HF-SCDB payload where `block_data` starts after the extended tag byte.
///
/// Layout (offsets from start of `block_data`):
/// `[0..1]` = Reserved; `[2]` = Version; `[3]` = Max_TMDS_Character_Rate × 5 MHz;
/// `[4]` = SCDC/3D flags; `[5]` = FRL/DC/UHD_VIC; `[6+]` = optional sections.
///
/// Returns `None` if fewer than 6 bytes (2 reserved + 4 mandatory SCDS bytes).
pub(super) fn parse_hf_scdb(block_data: &[u8]) -> Option<HdmiForumSinkCap> {
    if block_data.len() < 6 {
        return None;
    }
    parse_hdmi_scds(&block_data[2..])
}

/// IEEE OUI for the HDMI Forum (`0xC45DD8`), in wire order (little-endian).
pub(super) const HF_VSDB_OUI: [u8; 3] = {
    let v = display_types::cea861::oui::HDMI_FORUM;
    [
        (v & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        ((v >> 16) & 0xFF) as u8,
    ]
};

/// Parses `block_data` (the full VSDB payload, starting with the 3-byte OUI)
/// as an HDMI Forum VSDB (HF-VSDB, OUI `0xC45DD8`).
///
/// The HF-VSDB carries the SCDS immediately after the OUI, with no reserved
/// prefix bytes.  The minimum valid block is 7 bytes: 3 OUI + 4 mandatory SCDS
/// bytes (version, max TMDS rate, SCDC flags, FRL/DC flags).
///
/// Returns `None` if the OUI does not match or the block is too short.
pub(super) fn parse_hf_vsdb(block_data: &[u8]) -> Option<HdmiForumSinkCap> {
    if block_data.len() < 7 {
        return None;
    }
    if block_data[0..3] != HF_VSDB_OUI {
        return None;
    }
    parse_hdmi_scds(&block_data[3..])
}

// ---------------------------------------------------------------------------
// VESA Display Device Data Block (extended tag 0x02)
// ---------------------------------------------------------------------------

pub(super) fn parse_vesa_display_device(block_data: &[u8]) -> Option<VesaDisplayDeviceBlock> {
    // Fixed 30-byte payload after the extended tag byte.
    if block_data.len() < 30 {
        return None;
    }

    let interface_type = block_data[0] >> 4;
    let num_links = block_data[0] & 0x0F;
    let interface_version = block_data[1] >> 4;
    let interface_release = block_data[1] & 0x0F;
    let content_protection = block_data[2];

    // Clock frequency:
    //   Byte 0x05: [m5 m4 m3 m2 m1 m0 M9 M8]  → min = bits 7:2, max hi = bits 1:0
    //   Byte 0x06: [M7 M6 M5 M4 M3 M2 M1 M0]  → max lo
    let min_clock_mhz = block_data[3] >> 2;
    let max_clock_mhz = ((block_data[3] as u16 & 0x03) << 8) | block_data[4] as u16;

    // Native pixel format: 16-bit little-endian pairs, stored as (count - 1).
    // All-zero means no fixed pixel format.
    let h_raw = u16::from_le_bytes([block_data[5], block_data[6]]);
    let v_raw = u16::from_le_bytes([block_data[7], block_data[8]]);
    let (native_width, native_height) = if h_raw == 0 && v_raw == 0 {
        (None, None)
    } else {
        (Some(h_raw.saturating_add(1)), Some(v_raw.saturating_add(1)))
    };

    let aspect_ratio_raw = block_data[9];
    let default_orientation = (block_data[10] >> 6) & 0x03;
    let rotation_capability = (block_data[10] >> 4) & 0x03;
    let zero_pixel_location = (block_data[10] >> 2) & 0x03;
    let scan_direction = block_data[10] & 0x03;

    let subpixel_layout = block_data[11];
    let h_pitch_hundredths_mm = block_data[12];
    let v_pitch_hundredths_mm = block_data[13];

    let misc = block_data[14];
    let dithering = (misc >> 6) & 0x03;
    let direct_drive = misc & 0x20 != 0;
    let overdrive_not_recommended = misc & 0x10 != 0;
    let deinterlacing = misc & 0x08 != 0;

    let audio = block_data[15];
    let audio_on_video_interface = audio & 0x80 != 0;
    let separate_audio_inputs = audio & 0x40 != 0;
    let audio_input_override = audio & 0x20 != 0;

    // Audio delay: bit 7 = sign (1 = audio late / positive), bits 6:0 * 2 = ms.
    // Value 0x00 = no delay information provided.
    let delay_raw = block_data[16];
    let audio_delay_ms = if delay_raw == 0 {
        None
    } else {
        let mag = (delay_raw & 0x7F) as i16 * 2;
        Some(if delay_raw & 0x80 != 0 { mag } else { -mag })
    };

    let frame_rate_byte = block_data[17];
    let frame_rate_conversion = (frame_rate_byte >> 6) & 0x03;
    let frame_rate_range = frame_rate_byte & 0x3F;
    let native_frame_rate = block_data[18];

    // Color bit depth: bits 7:4 = interface (value + 1), bits 3:0 = display (value + 1).
    let cbd = block_data[19];
    let interface_color_depth = (cbd >> 4) + 1;
    let display_color_depth = (cbd & 0x0F) + 1;

    let mut additional_chromaticities = [0u8; 8];
    additional_chromaticities.copy_from_slice(&block_data[20..28]);

    let rt = block_data[28];
    let response_time_ms = rt & 0x7F;
    let response_time_white_to_black = rt & 0x80 != 0;

    let overscan = block_data[29];
    let h_overscan_pct = (overscan >> 4) & 0x0F;
    let v_overscan_pct = overscan & 0x0F;

    Some(VesaDisplayDeviceBlock::new(
        interface_type,
        num_links,
        interface_version,
        interface_release,
        content_protection,
        min_clock_mhz,
        max_clock_mhz,
        native_width,
        native_height,
        aspect_ratio_raw,
        default_orientation,
        rotation_capability,
        zero_pixel_location,
        scan_direction,
        subpixel_layout,
        h_pitch_hundredths_mm,
        v_pitch_hundredths_mm,
        dithering,
        direct_drive,
        overdrive_not_recommended,
        deinterlacing,
        audio_on_video_interface,
        separate_audio_inputs,
        audio_input_override,
        audio_delay_ms,
        frame_rate_conversion,
        frame_rate_range,
        native_frame_rate,
        interface_color_depth,
        display_color_depth,
        additional_chromaticities,
        response_time_ms,
        response_time_white_to_black,
        h_overscan_pct,
        v_overscan_pct,
    ))
}

// ---------------------------------------------------------------------------
// VESA Video Timing Block Extension (extended tag 0x03)
// ---------------------------------------------------------------------------

/// Parses a VESA Video Timing Block Extension (extended tag 0x03).
///
/// The Detailed Timing Blocks are decoded like any other DTD; their warnings go to `warn`.
pub(super) fn parse_vtb_ext(
    block_data: &[u8],
    options: &DecodeOptions,
    warn: &mut dyn FnMut(EdidWarning),
) -> Option<VtbExtBlock> {
    // Payload layout (after the extended tag byte):
    //   [0] version (expected 0x01)
    //   [1] w = number of 18-byte DTBs  (0–6)
    //   [2] y = number of 3-byte CVTs   (0–40)
    //   [3] z = number of 2-byte STs    (0–61)
    //   [4..] timing data
    if block_data.len() < 4 {
        return None;
    }
    let version = block_data[0];
    let w = block_data[1] as usize;
    let y = block_data[2] as usize;
    let z = block_data[3] as usize;
    let data = &block_data[4..];

    let required = w * 18 + y * 3 + z * 2;
    if data.len() < required {
        return None;
    }

    let mut timings = Vec::new();

    // Detailed Timing Blocks (18 bytes each)
    for i in 0..w {
        match build_dtd_mode(&data[i * 18..(i + 1) * 18], options.dtd_border_policy, warn) {
            Ok(Some(mode)) if !timings.contains(&mode) => timings.push(mode),
            Ok(_) => {}
            Err(w) => warn(w),
        }
    }

    // CVT 3-byte descriptors
    let cvt_base = w * 18;
    for i in 0..y {
        let off = cvt_base + i * 3;
        let (b0, b1, b2) = (data[off], data[off + 1], data[off + 2]);
        if b0 == 0 {
            continue; // unused slot
        }
        let lines_raw = (((b1 as u16) & 0xF0) << 4) | (b0 as u16);
        let v_add = (lines_raw + 1) * 2;
        let h_add = {
            let v = v_add as u32;
            let h = match (b1 >> 2) & 0x03 {
                0b00 => v * 4 / 3,
                0b01 => v * 16 / 9,
                0b10 => v * 16 / 10,
                _ => continue, // undefined aspect ratio
            };
            ((h / 8) * 8) as u16
        };
        for (mask, rate) in [
            (0x10u8, 50u16),
            (0x08, 60),
            (0x04, 75),
            (0x02, 85),
            (0x01, 60),
        ] {
            if b2 & mask != 0 {
                let mode = VideoMode::new(h_add, v_add, rate, false);
                if !timings.contains(&mode) {
                    timings.push(mode);
                }
            }
        }
    }

    // Standard Timing 2-byte descriptors
    let st_base = w * 18 + y * 3;
    for i in 0..z {
        let off = st_base + i * 2;
        let (b1, b2) = (data[off], data[off + 1]);
        if (b1 == 0x01 && b2 == 0x01) || b1 == 0x00 {
            continue; // unused slot
        }
        let width = (b1 as u16 + 31) * 8;
        let height = match (b2 >> 6) & 0x03 {
            0x00 => (width * 10) / 16, // 16:10
            0x01 => (width * 3) / 4,   // 4:3
            0x02 => (width * 4) / 5,   // 5:4
            _ => (width * 9) / 16,     // 16:9
        };
        let mode = VideoMode::new(width, height, (b2 & 0x3F) as u16 + 60, false);
        if !timings.contains(&mode) {
            timings.push(mode);
        }
    }

    Some(VtbExtBlock::new(version, timings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::capabilities::RefreshRate;

    #[test]
    fn test_video_capability() {
        // QS set, PT=1, IT=0, CE=3
        let data = [EXT_TAG_VIDEO_CAPABILITY, 0b0101_0011];
        let vc = parse_video_capability(&data).unwrap();
        assert!(vc.flags.contains(VideoCapabilityFlags::QS));
        assert!(!vc.flags.contains(VideoCapabilityFlags::QY));
        assert_eq!(vc.pt_behaviour, 1);
        assert_eq!(vc.it_behaviour, 0);
        assert_eq!(vc.ce_behaviour, 3);
    }

    #[test]
    fn test_colorimetry_bt2020() {
        // BT2020RGB | BT2020YCC, metadata profile 0x05
        let data = [EXT_TAG_COLORIMETRY, 0xC0, 0x05];
        let cb = parse_colorimetry(&data).unwrap();
        assert!(cb.colorimetry.contains(ColorimetryFlags::BT2020RGB));
        assert!(cb.colorimetry.contains(ColorimetryFlags::BT2020YCC));
        assert!(!cb.colorimetry.contains(ColorimetryFlags::XVYCC601));
        assert_eq!(cb.metadata_profiles, 5);
    }

    #[test]
    fn test_colorimetry_no_metadata_byte() {
        let data = [EXT_TAG_COLORIMETRY, 0x03];
        let cb = parse_colorimetry(&data).unwrap();
        assert_eq!(cb.metadata_profiles, 0);
    }

    #[test]
    fn test_hdr_metadata_basic() {
        // SDR + ST2084, SMD type 1, no luminance bytes
        let data = [EXT_TAG_HDR_STATIC_METADATA, 0x05, 0x01];
        let hdr = parse_hdr_static_metadata(&data).unwrap();
        assert!(hdr.eotf.contains(HdrEotf::SDR));
        assert!(hdr.eotf.contains(HdrEotf::ST2084));
        assert!(!hdr.eotf.contains(HdrEotf::HLG));
        assert_eq!(hdr.static_metadata_descriptors, 1);
        assert!(hdr.max_luminance.is_none());
        assert!(hdr.min_luminance.is_none());
    }

    #[test]
    fn test_hdr_metadata_luminance() {
        // raw=96 → 50 * 2^(96/32) = 50 * 8 = 400 cd/m²
        // min raw=128 → 400 * (128/255)^2 / 100 ≈ 1.006 cd/m²
        let data = [EXT_TAG_HDR_STATIC_METADATA, 0x04, 0x01, 96, 96, 128];
        let hdr = parse_hdr_static_metadata(&data).unwrap();
        let max = hdr.max_luminance.unwrap();
        assert!((max - 400.0).abs() < 0.1, "max={max}");
        let min = hdr.min_luminance.unwrap();
        assert!(min > 0.9 && min < 1.1, "min={min}");
    }

    #[test]
    fn test_hdr_too_short_returns_none() {
        // Missing the EOTF byte
        let data = [EXT_TAG_HDR_STATIC_METADATA];
        assert!(parse_hdr_static_metadata(&data).is_none());
    }

    #[test]
    fn test_speaker_allocation_basic() {
        // FL/FR + LFE1 + FC set; bytes 2 and 3 absent → zero
        let data = [0x07u8]; // FL_FR | LFE1 | FC
        let sa = parse_speaker_allocation(&data).unwrap();
        assert!(sa.channels.contains(SpeakerAllocationFlags::FL_FR));
        assert!(sa.channels.contains(SpeakerAllocationFlags::LFE1));
        assert!(sa.channels.contains(SpeakerAllocationFlags::FC));
        assert!(!sa.channels.contains(SpeakerAllocationFlags::BL_BR));
        assert_eq!(sa.channels_2, SpeakerAllocationFlags2::empty());
        assert_eq!(sa.channels_3, SpeakerAllocationFlags3::empty());
    }

    #[test]
    fn test_speaker_allocation_extended_bytes() {
        // All three bytes present: FL/FR, TpFL/TpFR, TpBL/TpBR
        let data = [0x01u8, 0x01u8, 0x01u8];
        let sa = parse_speaker_allocation(&data).unwrap();
        assert!(sa.channels.contains(SpeakerAllocationFlags::FL_FR));
        assert!(sa.channels_2.contains(SpeakerAllocationFlags2::TP_FL_FR));
        assert!(sa.channels_3.contains(SpeakerAllocationFlags3::TP_BL_TP_BR));
    }

    #[test]
    fn test_speaker_allocation_empty_returns_none() {
        assert!(parse_speaker_allocation(&[]).is_none());
    }

    #[test]
    fn test_hdr_dynamic_metadata_hdr10_plus() {
        // Extended tag + one descriptor: version=0, type=1 (HDR10+)
        let data = [EXT_TAG_HDR_DYNAMIC_METADATA, 0x01];
        let descs = parse_hdr_dynamic_metadata(&data);
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].application_type, 1);
        assert_eq!(descs[0].application_version, 0);
    }

    #[test]
    fn test_hdr_dynamic_metadata_empty_block() {
        let data = [EXT_TAG_HDR_DYNAMIC_METADATA];
        let descs = parse_hdr_dynamic_metadata(&data);
        assert!(descs.is_empty());
    }

    #[test]
    fn test_video_format_preferences() {
        // Extended tag + three SVRs: VIC 16, DTD ref 129, Y420 ref 145
        let data = [EXT_TAG_VIDEO_FORMAT_PREFERENCE, 16, 129, 145];
        let prefs = parse_video_format_preferences(&data);
        assert_eq!(prefs, vec![16, 129, 145]);
    }

    #[test]
    fn test_y420_vdb_filters_vic0() {
        // Extended tag + VIC 93 (native, bit 7 set), VIC 0 reserved (skipped)
        let data = [EXT_TAG_Y420_VIDEO, 0x80 | 93, 0x80];
        let vics = parse_y420_vdb(&data);
        assert_eq!(vics, vec![93]);
    }

    #[test]
    fn test_y420_capability_map() {
        let data = [EXT_TAG_Y420_CAPABILITY_MAP, 0b0000_0101, 0xFF];
        let bitmap = parse_y420_capability_map(&data);
        assert_eq!(bitmap, vec![0b0000_0101, 0xFF]);
    }

    // --- VESA Display Transfer Characteristic tests ---

    #[test]
    fn test_vesa_dtc_8bit() {
        // 8-bit encoding: bits 7:6 = 0b00; payload = [0x00, 0x80, 0xFF] (black, mid, white)
        let data = [0x00u8, 0x00, 0x80, 0xFF];
        let dtc = parse_vesa_transfer_characteristic(&data).unwrap();
        assert_eq!(dtc.encoding, DtcPointEncoding::Bits8);
        assert_eq!(dtc.points.len(), 3);
        assert!((dtc.points[0] - 0.0).abs() < 0.001);
        assert!((dtc.points[1] - 0x80 as f32 / 255.0).abs() < 0.001);
        assert!((dtc.points[2] - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_vesa_dtc_10bit() {
        // 10-bit encoding: bits 7:6 = 0b01; encode 4 points packed in 5 bytes.
        // Point 0 = 0x3FF (max), Point 1 = 0x000, Point 2 = 0x200, Point 3 = 0x155
        // Pack: b0[9:2]=0xFF b1[9:8|7:4]=0xC0|0x00=0xC0 b2[3:0|9:6]=0x08 b3[5:0|9:8]=0x00|0x01 b4[7:0]=0x55
        // Simpler: use 0x000 and 0x3FF for easy verification.
        // Pack 0x3FF, 0x000, 0x000, 0x000:
        // bits: 11_1111_1111 00_0000_0000 00_0000_0000 00_0000_0000
        // byte0 = 1111_1111 = 0xFF
        // byte1 = 11_000000 = 0xC0
        // byte2 = 00_0000_00 = 0x00
        // byte3 = 00_000000 = 0x00
        // byte4 = 0000_0000 = 0x00
        let data = [0x40u8, 0xFF, 0xC0, 0x00, 0x00, 0x00]; // 0x40 = 01_000000 = 10-bit
        let dtc = parse_vesa_transfer_characteristic(&data).unwrap();
        assert_eq!(dtc.encoding, DtcPointEncoding::Bits10);
        assert_eq!(dtc.points.len(), 4);
        assert!(
            (dtc.points[0] - 1.0).abs() < 0.001,
            "point0={}",
            dtc.points[0]
        );
        assert!((dtc.points[1] - 0.0).abs() < 0.001);
        assert!((dtc.points[2] - 0.0).abs() < 0.001);
        assert!((dtc.points[3] - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_vesa_dtc_12bit() {
        // 12-bit encoding: bits 7:6 = 0b10; encode 2 points packed in 3 bytes.
        // Point 0 = 0xFFF, Point 1 = 0x000
        // byte0 = 0xFF, byte1 = 0xF0, byte2 = 0x00
        let data = [0x80u8, 0xFF, 0xF0, 0x00]; // 0x80 = 10_000000 = 12-bit
        let dtc = parse_vesa_transfer_characteristic(&data).unwrap();
        assert_eq!(dtc.encoding, DtcPointEncoding::Bits12);
        assert_eq!(dtc.points.len(), 2);
        assert!(
            (dtc.points[0] - 1.0).abs() < 0.001,
            "point0={}",
            dtc.points[0]
        );
        assert!((dtc.points[1] - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_vesa_dtc_reserved_encoding_returns_none() {
        // bits 7:6 = 0b11 → reserved → None
        let data = [0xC0u8, 0x00];
        assert!(parse_vesa_transfer_characteristic(&data).is_none());
    }

    #[test]
    fn test_vesa_dtc_empty_returns_none() {
        assert!(parse_vesa_transfer_characteristic(&[]).is_none());
    }

    // --- Room Configuration Data Block tests ---

    #[test]
    fn test_room_configuration_basic() {
        // 5 speakers, speaker location entries present
        let data = [EXT_TAG_ROOM_CONFIGURATION, 0x45]; // 0b0100_0101: bit6=1, bits4:0=5
        let rc = parse_room_configuration(&data).unwrap();
        assert_eq!(rc.speaker_count, 5);
        assert!(rc.has_speaker_locations);
    }

    #[test]
    fn test_room_configuration_no_locations() {
        // 7 speakers, no location data
        let data = [EXT_TAG_ROOM_CONFIGURATION, 0x07]; // bit6=0, count=7
        let rc = parse_room_configuration(&data).unwrap();
        assert_eq!(rc.speaker_count, 7);
        assert!(!rc.has_speaker_locations);
    }

    #[test]
    fn test_room_configuration_too_short_returns_none() {
        let data = [EXT_TAG_ROOM_CONFIGURATION];
        assert!(parse_room_configuration(&data).is_none());
    }

    // --- Speaker Location Data Block tests ---

    #[test]
    fn test_speaker_location_entries() {
        // Two entries: FL/FR at distance 100, LFE1 at distance 80
        let data = [EXT_TAG_SPEAKER_LOCATION, 0x00, 100, 0x01, 80];
        let entries = parse_speaker_location(&data);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].channel_assignment, 0x00);
        assert_eq!(entries[0].distance, 100);
        assert_eq!(entries[1].channel_assignment, 0x01);
        assert_eq!(entries[1].distance, 80);
    }

    #[test]
    fn test_speaker_location_odd_trailing_byte_ignored() {
        // Three bytes after tag = one complete entry + one orphan byte → 1 entry
        let data = [EXT_TAG_SPEAKER_LOCATION, 0x02, 50, 0xFF];
        let entries = parse_speaker_location(&data);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].channel_assignment, 0x02);
        assert_eq!(entries[0].distance, 50);
    }

    #[test]
    fn test_speaker_location_empty_block() {
        let data = [EXT_TAG_SPEAKER_LOCATION];
        assert!(parse_speaker_location(&data).is_empty());
    }

    // --- InfoFrame Data Block tests ---

    #[test]
    fn test_infoframe_avi_and_audio() {
        // Two standard SIDs (no extra bytes): AVI (0x02) and Audio (0x04).
        // SID byte format: bits 7:5 = extra byte count (0), bits 4:0 = type.
        let data = [EXT_TAG_INFOFRAME, 0x02, 0x04];
        let descs = parse_infoframe_db(&data);
        assert_eq!(descs.len(), 2);
        assert_eq!(descs[0].type_code, infoframe_type::AVI);
        assert!(descs[0].vendor_oui.is_none());
        assert_eq!(descs[1].type_code, infoframe_type::AUDIO);
        assert!(descs[1].vendor_oui.is_none());
    }

    #[test]
    fn test_infoframe_vendor_specific_with_oui() {
        // VSI SID: type=0x01, extra=3 → byte = (3 << 5) | 0x01 = 0x61; OUI = 0x000C03 (HDMI)
        let data = [EXT_TAG_INFOFRAME, 0x61, 0x00, 0x0C, 0x03];
        let descs = parse_infoframe_db(&data);
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].type_code, infoframe_type::VENDOR_SPECIFIC);
        assert_eq!(descs[0].vendor_oui, Some(0x000C03));
    }

    #[test]
    fn test_infoframe_mixed_vsi_and_standard() {
        // VSI (3 extra bytes) followed by HDR (0 extra bytes).
        let data = [EXT_TAG_INFOFRAME, 0x61, 0x00, 0x0C, 0x03, 0x07];
        let descs = parse_infoframe_db(&data);
        assert_eq!(descs.len(), 2);
        assert_eq!(descs[0].type_code, infoframe_type::VENDOR_SPECIFIC);
        assert_eq!(descs[0].vendor_oui, Some(0x000C03));
        assert_eq!(descs[1].type_code, infoframe_type::DYNAMIC_RANGE_MASTERING);
        assert!(descs[1].vendor_oui.is_none());
    }

    #[test]
    fn test_infoframe_empty_block() {
        let data = [EXT_TAG_INFOFRAME];
        assert!(parse_infoframe_db(&data).is_empty());
    }

    // --- VESA Display Device Data Block tests ---

    fn minimal_dddb_payload() -> [u8; 30] {
        let mut d = [0u8; 30];
        // interface_type=0x0A (DisplayPort), num_links=2 → byte[0] = 0xA2
        d[0] = 0xA2;
        // interface_version=1, interface_release=0 → byte[1] = 0x10
        d[1] = 0x10;
        // content_protection = 0x01 (HDCP)
        d[2] = 0x01;
        // min_clock=40 MHz (40<<2=0xA0), max_clock hi bits=0 → byte[3] = 0xA0
        // max_clock lo = 0xF0 (240 dec, so max = 240 MHz) → byte[4] = 0xF0
        d[3] = 0xA0;
        d[4] = 0xF0;
        // native pixel: 1920x1080 → stored as 1919 and 1079 (LE)
        let w: u16 = 1919;
        let h: u16 = 1079;
        d[5] = (w & 0xFF) as u8;
        d[6] = (w >> 8) as u8;
        d[7] = (h & 0xFF) as u8;
        d[8] = (h >> 8) as u8;
        // aspect_ratio_raw = 0x09 (16:9)
        d[9] = 0x09;
        // orientation/rotation/zero_pixel/scan: default_orientation=0, rest 0
        d[10] = 0x00;
        // subpixel=1 (RGB), pitch defaults to 0
        d[11] = 0x01;
        // audio: separate audio inputs set → byte[15] bit6 = 0x40
        d[15] = 0x40;
        // audio_delay = 0x82 → sign bit set (audio late), mag = (0x02 * 2) = 4 ms
        d[16] = 0x82;
        // color bit depth: interface=8 (stored as 7), display=6 (stored as 5) → 0x75
        d[19] = 0x75;
        // response time: 5 ms, black-to-white (bit7=0) → 0x05
        d[28] = 0x05;
        // overscan: h=2%, v=3% → 0x23
        d[29] = 0x23;
        d
    }

    #[test]
    fn test_dddb_basic_fields() {
        let payload = minimal_dddb_payload();
        let block = parse_vesa_display_device(&payload).unwrap();
        assert_eq!(block.interface_type, 0x0A);
        assert_eq!(block.num_links, 2);
        assert_eq!(block.interface_version, 1);
        assert_eq!(block.interface_release, 0);
        assert_eq!(block.content_protection, 0x01);
    }

    #[test]
    fn test_dddb_clock_frequency() {
        let payload = minimal_dddb_payload();
        let block = parse_vesa_display_device(&payload).unwrap();
        assert_eq!(block.min_clock_mhz, 40);
        assert_eq!(block.max_clock_mhz, 0xF0);
    }

    #[test]
    fn test_dddb_native_resolution() {
        let payload = minimal_dddb_payload();
        let block = parse_vesa_display_device(&payload).unwrap();
        assert_eq!(block.native_width, Some(1920));
        assert_eq!(block.native_height, Some(1080));
    }

    #[test]
    fn test_dddb_native_resolution_none_when_zero() {
        let mut payload = minimal_dddb_payload();
        payload[5] = 0;
        payload[6] = 0;
        payload[7] = 0;
        payload[8] = 0;
        let block = parse_vesa_display_device(&payload).unwrap();
        assert_eq!(block.native_width, None);
        assert_eq!(block.native_height, None);
    }

    #[test]
    fn test_dddb_audio_and_delay() {
        let payload = minimal_dddb_payload();
        let block = parse_vesa_display_device(&payload).unwrap();
        assert!(!block.audio_on_video_interface);
        assert!(block.separate_audio_inputs);
        assert!(!block.audio_input_override);
        // delay_raw = 0x82: sign bit set → positive (audio late), mag = 2*2 = 4 ms
        assert_eq!(block.audio_delay_ms, Some(4));
    }

    #[test]
    fn test_dddb_audio_delay_none_when_zero() {
        let mut payload = minimal_dddb_payload();
        payload[16] = 0;
        let block = parse_vesa_display_device(&payload).unwrap();
        assert!(block.audio_delay_ms.is_none());
    }

    #[test]
    fn test_dddb_audio_delay_negative() {
        // delay_raw = 0x02: sign bit clear → negative, mag = 2*2 = 4 → -4 ms
        let mut payload = minimal_dddb_payload();
        payload[16] = 0x02;
        let block = parse_vesa_display_device(&payload).unwrap();
        assert_eq!(block.audio_delay_ms, Some(-4));
    }

    #[test]
    fn test_dddb_color_depth() {
        let payload = minimal_dddb_payload();
        let block = parse_vesa_display_device(&payload).unwrap();
        // 0x75 → interface = (7 + 1) = 8, display = (5 + 1) = 6
        assert_eq!(block.interface_color_depth, 8);
        assert_eq!(block.display_color_depth, 6);
    }

    #[test]
    fn test_dddb_response_time_and_overscan() {
        let payload = minimal_dddb_payload();
        let block = parse_vesa_display_device(&payload).unwrap();
        assert_eq!(block.response_time_ms, 5);
        assert!(!block.response_time_white_to_black);
        assert_eq!(block.h_overscan_pct, 2);
        assert_eq!(block.v_overscan_pct, 3);
    }

    #[test]
    fn test_dddb_too_short_returns_none() {
        let payload = [0u8; 29];
        assert!(parse_vesa_display_device(&payload).is_none());
    }

    // --- VTB-EXT tests ---

    #[test]
    fn test_vtb_ext_empty_version_only() {
        // version=1, 0 DTBs, 0 CVTs, 0 STs
        let data = [0x01u8, 0x00, 0x00, 0x00];
        let vtb = parse_vtb_ext(&data, &DecodeOptions::new(), &mut |_| {}).unwrap();
        assert_eq!(vtb.version, 1);
        assert!(vtb.timings.is_empty());
    }

    #[test]
    fn test_vtb_ext_too_short_returns_none() {
        let data = [0x01u8, 0x00, 0x00];
        assert!(parse_vtb_ext(&data, &DecodeOptions::new(), &mut |_| {}).is_none());
    }

    /// A VTB-EXT payload with one Detailed Timing Block: 1920×1080, 148.50 MHz, blanking
    /// 280 × 45, the given horizontal border.
    fn vtb_ext_with_1080p_dtb(h_border: u8) -> [u8; 22] {
        let mut data = [0u8; 22];
        data[..4].copy_from_slice(&[0x01, 0x01, 0x00, 0x00]); // version 1, 1 DTB
        // Bytes 8–10: H front porch 88, H sync 44, V front porch 4 | V sync 5.
        data[4..22].copy_from_slice(&[
            0x02, 0x3A, 0x80, 0x18, 0x71, 0x38, 0x2D, 0x40, 88, 44, 0x45, 0x00, 0, 0, 0, h_border,
            0, 0x1E,
        ]);
        data
    }

    #[test]
    fn test_vtb_ext_dtb_keeps_detailed_timing() {
        // The DTBs used a decoder of their own that dropped everything but the identity.
        let mut warnings = Vec::new();
        let vtb = parse_vtb_ext(
            &vtb_ext_with_1080p_dtb(0),
            &DecodeOptions::new(),
            &mut |w| warnings.push(w),
        )
        .unwrap();
        let m = &vtb.timings[0];
        assert_eq!((m.width, m.height), (1920, 1080));
        assert_eq!(m.refresh_rate, Some(RefreshRate::integral(60)));
        assert_eq!(m.pixel_clock_khz, Some(148_500));
        assert_eq!((m.h_front_porch, m.h_sync_width), (88, 44));
        assert_eq!(m.h_blank, Some(280));
        assert!(m.sync.is_some());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_vtb_ext_dtb_border_warning_reaches_caller() {
        let mut warnings = Vec::new();
        let vtb = parse_vtb_ext(
            &vtb_ext_with_1080p_dtb(4),
            &DecodeOptions::new(),
            &mut |w| warnings.push(w),
        )
        .unwrap();
        assert_eq!(vtb.timings[0].h_blank, Some(272));
        assert_eq!(
            warnings,
            [EdidWarning::DtdBordersAmbiguous {
                h_border: 4,
                v_border: 0
            }]
        );
    }

    #[test]
    fn test_vtb_ext_standard_timings() {
        // version=1, 0 DTBs, 0 CVTs, 1 ST
        // width=1280: (b1+31)*8=1280 → b1=129=0x81; b2=0x00 → 16:10 ratio, rate=60+0=60
        let data = [0x01u8, 0x00, 0x00, 0x01, 0x81, 0x00];
        let vtb = parse_vtb_ext(&data, &DecodeOptions::new(), &mut |_| {}).unwrap();
        assert_eq!(vtb.timings.len(), 1);
        assert_eq!(vtb.timings[0].width, 1280);
        assert_eq!(vtb.timings[0].height, 800); // 1280 * 10 / 16 = 800
        assert_eq!(vtb.timings[0].refresh_rate, Some(RefreshRate::integral(60)));
    }

    #[test]
    fn test_vtb_ext_cvt_descriptor_16_9() {
        // version=1, 0 DTBs, 1 CVT, 0 STs
        // CVT encoding: 1080 lines → lines_raw = (1080/2 - 1) = 539 = 0x21B
        //   b0 = 0x1B, b1 = (0x20) | aspect_16_9=(0x01<<2) = 0x24
        //   b2: 60Hz bit set = 0x08
        let data = [0x01u8, 0x00, 0x01, 0x00, 0x1B, 0x24, 0x08];
        let vtb = parse_vtb_ext(&data, &DecodeOptions::new(), &mut |_| {}).unwrap();
        assert!(!vtb.timings.is_empty());
        let t = vtb
            .timings
            .iter()
            .find(|m| m.refresh_rate == Some(RefreshRate::integral(60)))
            .unwrap();
        assert_eq!(t.height, 1080);
        // 16:9: h = (1080 * 16 / 9) rounded down to multiple of 8 = 1920
        assert_eq!(t.width, 1920);
    }

    #[test]
    fn test_vtb_ext_insufficient_data_for_dtb_returns_none() {
        // version=1, 1 DTB, 0 CVTs, 0 STs — but only 4 bytes, no room for 18-byte DTB
        let data = [0x01u8, 0x01, 0x00, 0x00];
        assert!(parse_vtb_ext(&data, &DecodeOptions::new(), &mut |_| {}).is_none());
    }

    // --- Vendor-Specific Video / Audio Data Block tests ---

    #[test]
    fn test_vsvdb_basic() {
        // OUI = 0x00D046 (Dolby), stored LSB-first: [0x46, 0xD0, 0x00], payload = [0xAB, 0xCD]
        let payload = [0x46u8, 0xD0, 0x00, 0xAB, 0xCD];
        let b = parse_vendor_specific_block(&payload).unwrap();
        assert_eq!(b.oui, 0x00D046);
        assert_eq!(b.payload, vec![0xAB, 0xCD]);
    }

    #[test]
    fn test_vsvdb_oui_only_no_payload() {
        let payload = [0x03u8, 0x0C, 0x00]; // OUI = 0x000C03 (HDMI), no payload
        let b = parse_vendor_specific_block(&payload).unwrap();
        assert_eq!(b.oui, 0x000C03);
        assert!(b.payload.is_empty());
    }

    #[test]
    fn test_vsvdb_too_short_returns_none() {
        assert!(parse_vendor_specific_block(&[0x46u8, 0xD0]).is_none());
    }

    #[test]
    fn test_vsadb_same_structure() {
        // VSADB has identical wire format to VSVDB; reuse the same parser.
        let payload = [0x8Bu8, 0x84, 0x90, 0x01]; // OUI = 0x90848B (HDR10+), payload = [0x01]
        let b = parse_vendor_specific_block(&payload).unwrap();
        assert_eq!(b.oui, 0x90848B);
        assert_eq!(b.payload, vec![0x01]);
    }

    // --- DisplayID Type VII Video Timing Data Block (T7VTDB) tests ---

    /// Build a minimal valid 22-byte T7VTDB descriptor payload (after ext tag).
    ///
    /// Encodes 1920x1080@60Hz:
    ///   pixel_clock = 148500 kHz = 0x024414 → LE bytes [0x14, 0x44, 0x02]
    ///   H active = 1920 = 0x0780 → [0x80, 0x07]
    ///   H blank  =  280 = 0x0118 → [0x18, 0x01]
    ///   V active = 1080 = 0x0438 → [0x38, 0x04]
    ///   V blank  =   45 = 0x002D → [0x2D, 0x00]
    ///   refresh  = 148500000 / (2200 × 1125) = 60 Hz
    fn t7_1080p60() -> [u8; 22] {
        let mut d = [0u8; 22];
        d[0] = 0x02; // Block_Rev = 010b
        d[1] = 0x00; // T7Y420=0, T7HSP=0, T7VSP=0
        // All multi-byte timing fields use the wire encoding `value − 1`.
        // Pixel clock 148500 kHz → raw 148499 = 0x024413 → LE [0x13, 0x44, 0x02].
        d[2] = 0x13;
        d[3] = 0x44;
        d[4] = 0x02;
        d[5] = 0x00; // 3D_Support=00, T7IL=0, T7_Aspect_Ratio=0
        // H Active 1920 → raw 1919 = 0x077F → LE [0x7F, 0x07].
        d[6] = 0x7F;
        d[7] = 0x07;
        // H Blank 280 → raw 279 = 0x0117 → LE [0x17, 0x01].
        d[8] = 0x17;
        d[9] = 0x01;
        // H Offset + H Sync: leave as raw 0 (decoded = 1; doesn't affect refresh).
        // V Active 1080 → raw 1079 = 0x0437 → LE [0x37, 0x04].
        d[14] = 0x37;
        d[15] = 0x04;
        // V Blank 45 → raw 44 = 0x002C → LE [0x2C, 0x00].
        d[16] = 0x2C;
        d[17] = 0x00;
        d
    }

    #[test]
    fn test_t7vtdb_1080p60() {
        let d = t7_1080p60();
        let t7 = parse_t7vtdb(&d).unwrap();
        assert_eq!(t7.version, 2);
        assert_eq!(t7.mode.width, 1920);
        assert_eq!(t7.mode.height, 1080);
        assert_eq!(t7.mode.refresh_rate, Some(RefreshRate::integral(60)));
        assert!(!t7.mode.interlaced);
        assert!(!t7.y420);
    }

    #[test]
    fn test_t7vtdb_y420_flag() {
        let mut d = t7_1080p60();
        d[1] = 0x40; // T7Y420 = bit 6
        let t7 = parse_t7vtdb(&d).unwrap();
        assert!(t7.y420);
    }

    #[test]
    fn test_t7vtdb_interlaced_flag() {
        let mut d = t7_1080p60();
        d[5] = 0x10; // T7IL = bit 4
        let t7 = parse_t7vtdb(&d).unwrap();
        assert!(t7.mode.interlaced);
    }

    #[test]
    fn test_t7vtdb_too_short_returns_none() {
        let d = [0u8; 21];
        assert!(parse_t7vtdb(&d).is_none());
    }

    #[test]
    fn test_t7vtdb_zero_pixel_clock_returns_none() {
        let mut d = t7_1080p60();
        d[2] = 0;
        d[3] = 0;
        d[4] = 0;
        assert!(parse_t7vtdb(&d).is_none());
    }

    #[test]
    fn test_t7vtdb_h_active_overflow_returns_none() {
        // Raw 0xFFFF on H active overflows the +1 decode → descriptor must be skipped.
        // (The wire encoding cannot represent value 0; the previous "zero hactive"
        // semantics no longer apply.)
        let mut d = t7_1080p60();
        d[6] = 0xFF;
        d[7] = 0xFF;
        assert!(parse_t7vtdb(&d).is_none());
    }

    #[test]
    fn test_t7vtdb_720p60() {
        // 1280×720@60: pixel_clock = 74250 kHz, H blank = 370, V blank = 30,
        // h_total = 1650, v_total = 750 → 74250000 / 1237500 = 60 Hz.
        // All multi-byte fields use the `value − 1` wire encoding.
        // pclk 74250 → raw 74249 = 0x012209 → LE [0x09, 0x22, 0x01].
        // h_active 1280 → raw 1279 = 0x04FF → LE [0xFF, 0x04].
        // h_blank  370  → raw 369  = 0x0171 → LE [0x71, 0x01].
        // v_active 720  → raw 719  = 0x02CF → LE [0xCF, 0x02].
        // v_blank  30   → raw 29   = 0x001D → LE [0x1D, 0x00].
        let mut d = [0u8; 22];
        d[0] = 0x02;
        d[2] = 0x09;
        d[3] = 0x22;
        d[4] = 0x01;
        d[6] = 0xFF;
        d[7] = 0x04;
        d[8] = 0x71;
        d[9] = 0x01;
        d[14] = 0xCF;
        d[15] = 0x02;
        d[16] = 0x1D;
        let t7 = parse_t7vtdb(&d).unwrap();
        assert_eq!(t7.mode.width, 1280);
        assert_eq!(t7.mode.height, 720);
        assert_eq!(t7.mode.refresh_rate, Some(RefreshRate::integral(60)));
    }

    // T8VTDB tests

    #[test]
    fn test_t8vtdb_single_1080p60() {
        // header: Code_Type=0, y420=0, TCS=0, Rev=0 → 0x00; code 0x52 = 1920×1080@60
        let data = [0x00u8, 0x52];
        let t8 = parse_t8vtdb(&data).unwrap();
        assert_eq!(t8.version, 0);
        assert!(!t8.y420);
        assert_eq!(t8.codes, vec![0x52]);
        assert_eq!(t8.timings.len(), 1);
        assert_eq!(t8.timings[0].width, 1920);
        assert_eq!(t8.timings[0].height, 1080);
        assert_eq!(t8.timings[0].refresh_rate, Some(RefreshRate::integral(60)));
        assert!(!t8.timings[0].interlaced);
    }

    #[test]
    fn test_t8vtdb_multiple_codes() {
        // codes 0x04 (640×480@60), 0x09 (800×600@60), 0x10 (1024×768@60)
        let data = [0x00u8, 0x04, 0x09, 0x10];
        let t8 = parse_t8vtdb(&data).unwrap();
        assert_eq!(t8.codes, vec![0x04, 0x09, 0x10]);
        assert_eq!(t8.timings.len(), 3);
        assert_eq!(t8.timings[0].width, 640);
        assert_eq!(t8.timings[1].width, 800);
        assert_eq!(t8.timings[2].width, 1024);
    }

    #[test]
    fn test_t8vtdb_interlaced_0x0f() {
        // 0x0F = 1024×768@43 Hz, interlaced
        let data = [0x00u8, 0x0F];
        let t8 = parse_t8vtdb(&data).unwrap();
        assert_eq!(t8.timings[0].width, 1024);
        assert_eq!(t8.timings[0].height, 768);
        assert_eq!(t8.timings[0].refresh_rate, Some(RefreshRate::integral(43)));
        assert!(t8.timings[0].interlaced);
    }

    #[test]
    fn test_t8vtdb_two_byte_codes() {
        // TCS=1: header bit 3 set → 0x08; code 0x0052 = 1920×1080@60 stored LSB-first
        let data = [0x08u8, 0x52, 0x00];
        let t8 = parse_t8vtdb(&data).unwrap();
        assert_eq!(t8.codes, vec![0x0052]);
        assert_eq!(t8.timings[0].width, 1920);
        assert_eq!(t8.timings[0].refresh_rate, Some(RefreshRate::integral(60)));
    }

    #[test]
    fn test_t8vtdb_two_byte_codes_odd_trailing_byte_ignored() {
        // TCS=1; 3 payload bytes → one complete 2-byte code plus one orphan byte
        let data = [0x08u8, 0x52, 0x00, 0x55];
        let t8 = parse_t8vtdb(&data).unwrap();
        assert_eq!(t8.codes, vec![0x0052]); // trailing 0x55 ignored
    }

    #[test]
    fn test_t8vtdb_unknown_code_in_codes_but_not_timings() {
        // 0xFF is not a defined DMT ID
        let data = [0x00u8, 0xFF];
        let t8 = parse_t8vtdb(&data).unwrap();
        assert_eq!(t8.codes, vec![0xFF]);
        assert!(t8.timings.is_empty());
    }

    #[test]
    fn test_t8vtdb_y420_flag() {
        // T8Y420 = bit 5 of header → 0x20
        let data = [0x20u8, 0x52];
        let t8 = parse_t8vtdb(&data).unwrap();
        assert!(t8.y420);
    }

    #[test]
    fn test_t8vtdb_non_dmt_returns_none() {
        // Code_Type = 0x01 (bits 7:6 = 01) → not DMT → None
        let data = [0x40u8, 0x52];
        assert!(parse_t8vtdb(&data).is_none());
    }

    #[test]
    fn test_t8vtdb_empty_returns_none() {
        assert!(parse_t8vtdb(&[]).is_none());
    }

    #[test]
    fn test_dmt_to_mode_spot_checks() {
        let m = dmt_to_mode(0x55).unwrap();
        assert_eq!(m.width, 1280);
        assert_eq!(m.height, 720);
        assert_eq!(m.refresh_rate, Some(RefreshRate::integral(60)));
        assert!(!m.interlaced);

        let m = dmt_to_mode(0x0F).unwrap();
        assert!(m.interlaced);
        assert_eq!(m.refresh_rate, Some(RefreshRate::integral(43)));

        assert!(dmt_to_mode(0x00).is_none());
        assert!(dmt_to_mode(0x59).is_none());
    }

    // T10VTDB tests

    /// Build a 6-byte T10 descriptor for the given width, height, refresh (M=0).
    fn t10_desc_6(width: u16, height: u16, refresh: u16, y420: bool) -> [u8; 6] {
        let flags = if y420 { 0x80 } else { 0x00 };
        let h = (width - 1).to_le_bytes();
        let v = (height - 1).to_le_bytes();
        [flags, h[0], h[1], v[0], v[1], (refresh - 1) as u8]
    }

    #[test]
    fn test_t10vtdb_6byte_single() {
        // M=0 (rev=0x00); 640×480@60
        let desc = t10_desc_6(640, 480, 60, false);
        let mut data = vec![0x00u8]; // rev byte (M=0)
        data.extend_from_slice(&desc);
        let t10 = parse_t10vtdb(&data).unwrap();
        assert_eq!(t10.entries.len(), 1);
        assert_eq!(t10.entries[0].width, 640);
        assert_eq!(t10.entries[0].height, 480);
        assert_eq!(t10.entries[0].refresh_hz, 60);
        assert!(!t10.entries[0].y420);
    }

    #[test]
    fn test_t10vtdb_7byte_high_refresh() {
        // M=1 (rev=0x10); 1920×1080@512 — refresh > 255 requires 2-byte field
        // refresh stored = 511 = 0x1FF → lsb=0xFF, msb_bits=0x01
        let flags = 0x00u8;
        let h = (1919u16).to_le_bytes(); // 1920 - 1
        let v = (1079u16).to_le_bytes(); // 1080 - 1
        let data = [
            0x10, // rev: M=1
            flags, h[0], h[1], v[0], v[1], 0xFF, // refresh lsb (511 & 0xFF)
            0x01, // refresh msb bits[1:0] = 0x01 → (0x01 << 8) | 0xFF = 511; +1 = 512
        ];
        let t10 = parse_t10vtdb(&data).unwrap();
        assert_eq!(t10.entries[0].width, 1920);
        assert_eq!(t10.entries[0].height, 1080);
        assert_eq!(t10.entries[0].refresh_hz, 512);
    }

    #[test]
    fn test_t10vtdb_multiple_descriptors() {
        // M=0; two descriptors back-to-back
        let d1 = t10_desc_6(1280, 720, 60, false);
        let d2 = t10_desc_6(1920, 1080, 60, false);
        let mut data = vec![0x00u8];
        data.extend_from_slice(&d1);
        data.extend_from_slice(&d2);
        let t10 = parse_t10vtdb(&data).unwrap();
        assert_eq!(t10.entries.len(), 2);
        assert_eq!(t10.entries[0].width, 1280);
        assert_eq!(t10.entries[1].width, 1920);
    }

    #[test]
    fn test_t10vtdb_y420_flag() {
        let desc = t10_desc_6(3840, 2160, 60, true);
        let mut data = vec![0x00u8];
        data.extend_from_slice(&desc);
        let t10 = parse_t10vtdb(&data).unwrap();
        assert!(t10.entries[0].y420);
    }

    #[test]
    fn test_t10vtdb_invalid_m_returns_none() {
        // M=3 (bits[6:4]=011 → rev=0x30) is invalid
        let data = [0x30u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(parse_t10vtdb(&data).is_none());
    }

    #[test]
    fn test_t10vtdb_empty_returns_none() {
        assert!(parse_t10vtdb(&[]).is_none());
    }

    #[test]
    fn test_t10vtdb_trailing_partial_descriptor_ignored() {
        // M=0 (6-byte descriptors); 7 payload bytes = 1 complete + 1 orphan
        let desc = t10_desc_6(800, 600, 60, false);
        let mut data = vec![0x00u8];
        data.extend_from_slice(&desc);
        data.push(0xFF); // orphan byte
        let t10 = parse_t10vtdb(&data).unwrap();
        assert_eq!(t10.entries.len(), 1); // only the complete descriptor
    }

    // HF-EEODB tests

    #[test]
    fn test_hf_eeodb_valid() {
        assert_eq!(parse_hf_eeodb(&[5]), Some(5));
        assert_eq!(parse_hf_eeodb(&[255]), Some(255));
        assert_eq!(parse_hf_eeodb(&[1]), Some(1));
    }

    #[test]
    fn test_hf_eeodb_zero_returns_none() {
        assert!(parse_hf_eeodb(&[0]).is_none());
    }

    #[test]
    fn test_hf_eeodb_empty_returns_none() {
        assert!(parse_hf_eeodb(&[]).is_none());
    }

    // HF-SCDB tests

    /// Build a minimal HF-SCDB block_data (2 reserved + 4 mandatory SCDS bytes).
    fn hf_scdb_min(version: u8, tmds_raw: u8, scdc: u8, frl_dc: u8) -> Vec<u8> {
        vec![0x00, 0x00, version, tmds_raw, scdc, frl_dc]
    }

    #[test]
    fn test_hf_scdb_basic_fields() {
        // version=1, max_tmds=600MHz (raw=120=0x78), scdc_present+rr_capable=0xC0,
        // max_frl_rate=6 (12Gbps×4), uhd_vic=1 → frl_dc = (6<<4)|0x08 = 0x68
        let data = hf_scdb_min(1, 0x78, 0xC0, 0x68);
        let cap = parse_hf_scdb(&data).unwrap();
        assert_eq!(cap.version, 1);
        assert_eq!(cap.max_tmds_rate_mhz, 600);
        assert!(cap.scdc_present);
        assert!(cap.rr_capable);
        assert!(!cap.cable_status);
        assert_eq!(cap.max_frl_rate, HdmiForumFrl::Rate12Gbps4Lanes);
        assert!(cap.uhd_vic);
        assert!(!cap.dc_30bit_420);
        assert!(cap.vrr_min_hz.is_none());
        assert!(cap.dsc.is_none());
    }

    #[test]
    fn test_hf_scdb_dc_flags() {
        // dc_48bit_420[2]=1, dc_36bit_420[1]=1, dc_30bit_420[0]=1 → frl_dc = 0x07
        let data = hf_scdb_min(1, 0x00, 0x00, 0x07);
        let cap = parse_hf_scdb(&data).unwrap();
        assert!(cap.dc_48bit_420);
        assert!(cap.dc_36bit_420);
        assert!(cap.dc_30bit_420);
    }

    #[test]
    fn test_hf_scdb_extended_flags_and_vrr() {
        // Extended section: ALLM[1]=1 → ext_byte = 0x02
        // VRRmin=48 (0x30), VRRmax=144 (0x0090 → b7=(0<<6)|0x30=0x30, b8=0x90)
        let mut data = hf_scdb_min(1, 0x78, 0xC0, 0x68);
        data.push(0x02); // ext_byte: ALLM=bit1
        data.push(0x30); // VRRmax[9:8]=0, VRRmin=48
        data.push(0x90); // VRRmax[7:0]=144
        let cap = parse_hf_scdb(&data).unwrap();
        assert!(cap.allm);
        assert!(!cap.fva);
        assert_eq!(cap.vrr_min_hz, Some(48));
        assert_eq!(cap.vrr_max_hz, Some(144));
    }

    #[test]
    fn test_hf_scdb_vrr_10bit_max() {
        // VRRmax = 0x1FF = 511 → b7 bits[7:6]=0b11 → 0xC0 | VRRmin=0 → 0xC0; b8=0xFF
        let mut data = hf_scdb_min(1, 0x00, 0x00, 0x00);
        data.push(0x00); // ext_byte
        data.push(0xC0); // VRRmax[9:8]=0b11, VRRmin=0
        data.push(0xFF); // VRRmax[7:0]=255
        let cap = parse_hf_scdb(&data).unwrap();
        assert_eq!(cap.vrr_max_hz, Some(0x03FF)); // 1023
    }

    #[test]
    fn test_hf_scdb_dsc_section() {
        // dsc_flags: DSC_1p2[7]=1, DSC_Native_420[6]=1 → 0xC0
        // dsc_frl_slices: DSC_Max_FRL_Rate=6 [7:4], DSC_MaxSlices=4 [3:0] → 0x64
        // chunk_raw=2 → max_chunk_bytes = 1024 * 3 = 3072
        let mut data = hf_scdb_min(1, 0x78, 0xC0, 0x68);
        data.extend_from_slice(&[0x00, 0x00, 0x00]); // ext + VRR (no VRR)
        data.extend_from_slice(&[0xC0, 0x64, 0x02]); // DSC bytes
        let cap = parse_hf_scdb(&data).unwrap();
        let dsc = cap.dsc.unwrap();
        assert!(dsc.dsc_1p2);
        assert!(dsc.native_420);
        assert!(!dsc.bpc12);
        assert_eq!(dsc.max_frl_rate, HdmiForumFrl::Rate12Gbps4Lanes);
        assert_eq!(dsc.max_slices, HdmiDscMaxSlices::Slices8At340Mhz);
        assert_eq!(dsc.max_chunk_bytes, 3072);
    }

    #[test]
    fn test_hf_scdb_too_short_returns_none() {
        // Only 5 bytes (need 6 minimum)
        assert!(parse_hf_scdb(&[0x00, 0x00, 0x01, 0x78, 0xC0]).is_none());
    }

    #[test]
    fn test_hf_scdb_empty_returns_none() {
        assert!(parse_hf_scdb(&[]).is_none());
    }

    #[test]
    fn test_hf_scdb_partial_vrr_gives_none() {
        // Extended byte present but VRR bytes incomplete (only 1 of 2)
        let mut data = hf_scdb_min(1, 0x00, 0x00, 0x00);
        data.push(0x02); // ext_byte (ALLM)
        data.push(0x30); // b7 (VRRmin), but b8 is missing
        let cap = parse_hf_scdb(&data).unwrap();
        assert!(cap.allm); // feature flag was parsed
        assert!(cap.vrr_min_hz.is_none()); // VRR range absent (need both bytes)
        assert!(cap.vrr_max_hz.is_none());
    }

    // HF-VSDB tests

    /// Builds a minimal valid HF-VSDB payload: OUI + 4-byte SCDS.
    fn hf_vsdb_min(version: u8, tmds_div5: u8, scdc: u8, frl_dc: u8) -> Vec<u8> {
        vec![0xD8, 0x5D, 0xC4, version, tmds_div5, scdc, frl_dc]
    }

    #[test]
    fn test_hf_vsdb_basic_fields() {
        // version=1, TMDS=120 → 600 MHz, SCDC_Present, Max_FRL_Rate=6 (12G×4), DC_48b_420
        let data = hf_vsdb_min(1, 120, 0x80, (6 << 4) | 0x04);
        let cap = parse_hf_vsdb(&data).unwrap();
        assert_eq!(cap.version, 1);
        assert_eq!(cap.max_tmds_rate_mhz, 600);
        assert!(cap.scdc_present);
        assert!(!cap.rr_capable);
        assert_eq!(cap.max_frl_rate, HdmiForumFrl::Rate12Gbps4Lanes);
        assert!(cap.dc_48bit_420);
        assert!(!cap.dc_36bit_420);
        assert!(!cap.dc_30bit_420);
    }

    #[test]
    fn test_hf_vsdb_wrong_oui_returns_none() {
        // OUI is HDMI Licensing (not Forum)
        let data = vec![0x03, 0x0C, 0x00, 1, 0, 0, 0];
        assert!(parse_hf_vsdb(&data).is_none());
    }

    #[test]
    fn test_hf_vsdb_too_short_returns_none() {
        // Only 6 bytes — need at least 7 (3 OUI + 4 SCDS)
        let data = vec![0xD8, 0x5D, 0xC4, 1, 0, 0];
        assert!(parse_hf_vsdb(&data).is_none());
    }

    #[test]
    fn test_hf_vsdb_allm_and_vrr() {
        // ALLM + VRR range: VRRmin=48, VRRmax=144
        let mut data = hf_vsdb_min(1, 0, 0, 0);
        data.push(0x02); // ext_byte: ALLM
        data.push(48); // VRRmin=48, VRRmax[9:8]=0
        data.push(144); // VRRmax[7:0]=144
        let cap = parse_hf_vsdb(&data).unwrap();
        assert!(cap.allm);
        assert_eq!(cap.vrr_min_hz, Some(48));
        assert_eq!(cap.vrr_max_hz, Some(144));
    }

    #[test]
    fn test_hf_vsdb_scds_same_as_scdb() {
        // The SCDS payload is identical for HF-VSDB and HF-SCDB.
        // Build equivalent payloads and confirm equal parsed output.
        let version = 1u8;
        let tmds = 120u8;
        let scdc = 0xC8u8; // SCDC_Present | RR_Capable | LTE_340
        let frl_dc = (5u8 << 4) | 0x03; // FRL=5 (10G), DC_36b_420 | DC_30b_420

        // HF-VSDB: OUI + SCDS directly
        let vsdb_data = vec![0xD8, 0x5D, 0xC4, version, tmds, scdc, frl_dc];
        let vsdb_cap = parse_hf_vsdb(&vsdb_data).unwrap();

        // HF-SCDB: 2 reserved bytes + SCDS (as passed after ext tag strip)
        let scdb_data = vec![0x00, 0x00, version, tmds, scdc, frl_dc];
        let scdb_cap = parse_hf_scdb(&scdb_data).unwrap();

        assert_eq!(vsdb_cap, scdb_cap);
    }

    // Extended tag coverage

    #[test]
    fn test_all_extended_tags_accounted_for() {
        // Every value 0x00–0xFF must be either in IMPLEMENTED_EXTENDED_TAGS or
        // within one of RESERVED_EXTENDED_TAG_RANGES.  If this test fails after
        // a spec update, add an EXT_TAG_* constant, implement the block (or add
        // a stub), update IMPLEMENTED_EXTENDED_TAGS, and shrink the relevant
        // reserved range.
        for tag in 0u16..=255 {
            let tag = tag as u8;
            let implemented = IMPLEMENTED_EXTENDED_TAGS.contains(&tag);
            let reserved = RESERVED_EXTENDED_TAG_RANGES
                .iter()
                .any(|&(lo, hi)| tag >= lo && tag <= hi);
            assert!(
                implemented || reserved,
                "Extended tag 0x{:02X} is unaccounted for: \
                 add it to IMPLEMENTED_EXTENDED_TAGS or RESERVED_EXTENDED_TAG_RANGES",
                tag
            );
        }
    }

    #[test]
    fn test_implemented_and_reserved_are_disjoint() {
        // Sanity check: no tag should appear in both lists simultaneously.
        for &tag in IMPLEMENTED_EXTENDED_TAGS {
            let in_reserved = RESERVED_EXTENDED_TAG_RANGES
                .iter()
                .any(|&(lo, hi)| tag >= lo && tag <= hi);
            assert!(
                !in_reserved,
                "Extended tag 0x{:02X} appears in both IMPLEMENTED and RESERVED",
                tag
            );
        }
    }
}
