mod metadata;
mod timing;

#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::capabilities::DisplayCapabilities;
use crate::model::capabilities::StaticContext;
use crate::model::diagnostics::EdidWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::diagnostics::ParseWarning;
use crate::model::extension::StaticExtensionHandler;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::extension::{ExtensionHandler, HandlerContext};
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::prelude::{Arc, Vec};

#[cfg(any(feature = "alloc", feature = "std"))]
use metadata::scan_all_metadata_blocks;
pub(crate) use timing::decode_type_vii_descriptor_to_mode;
use timing::process_data_blocks;

#[cfg(any(feature = "alloc", feature = "std"))]
pub use display_types::{DisplayIdCapabilities, DisplayIdImageSize, ImageSizeUnit};

/// Processes DisplayID extension blocks (tag `0x70`).
///
/// A single handler handles both DisplayID 1.x and 2.x — both versions use tag `0x70`
/// and the dispatch layer cannot distinguish them before the handler receives the payload.
/// Version-specific logic is selected internally after inspecting the version byte.
#[derive(Debug)]
pub struct DisplayIdHandler;

/// Minimum version byte for DisplayID 1.x (0x10 = version 1, revision 0).
const DISPLAYID_V1_MIN: u8 = 0x10;
/// Maximum version byte for DisplayID 1.x (0x1F = version 1, revision 15).
const DISPLAYID_V1_MAX: u8 = 0x1F;
/// Version byte for DisplayID 2.x.
const DISPLAYID_V2: u8 = 0x20;

use display_types::displayid::tag;

const TAG_PRODUCT_ID: u8 = tag::PRODUCT_ID;
const TAG_DISPLAY_PARAMS: u8 = tag::DISPLAY_PARAMS;
const TAG_COLOR_CHARACTERISTICS: u8 = tag::COLOR_CHARACTERISTICS;
const TAG_TYPE_I_TIMING: u8 = tag::TYPE_I_TIMING;
const TAG_TYPE_II_TIMING: u8 = tag::TYPE_II_TIMING;
const TAG_TYPE_III_TIMING: u8 = tag::TYPE_III_TIMING;
const TAG_TYPE_IV_TIMING: u8 = tag::TYPE_IV_TIMING;
const TAG_VESA_VIDEO_TIMING: u8 = tag::VESA_VIDEO_TIMING;
const TAG_CTA_VIDEO_TIMING: u8 = tag::CTA_VIDEO_TIMING;
const TAG_VIDEO_TIMING_RANGE: u8 = tag::VIDEO_TIMING_RANGE;
const TAG_SERIAL_NUMBER: u8 = tag::SERIAL_NUMBER;
const TAG_ASCII_STRING: u8 = tag::ASCII_STRING;
const TAG_DISPLAY_DEVICE_DATA: u8 = tag::DISPLAY_DEVICE_DATA;
const TAG_POWER_SEQUENCING: u8 = tag::POWER_SEQUENCING;
const TAG_TRANSFER_CHARACTERISTICS: u8 = tag::TRANSFER_CHARACTERISTICS;
const TAG_DISPLAY_INTERFACE: u8 = tag::DISPLAY_INTERFACE;
const TAG_STEREO_DISPLAY_INTERFACE: u8 = tag::STEREO_DISPLAY_INTERFACE;
const TAG_TYPE_V_TIMING: u8 = tag::TYPE_V_TIMING;
const TAG_TILED_TOPOLOGY: u8 = tag::TILED_TOPOLOGY;
const TAG_TYPE_VI_TIMING: u8 = tag::TYPE_VI_TIMING;
const TAG_V2_PRODUCT_ID: u8 = tag::V2_PRODUCT_ID;
const TAG_V2_DISPLAY_PARAMS: u8 = tag::V2_DISPLAY_PARAMS;
const TAG_V2_TYPE_VII_TIMING: u8 = tag::V2_TYPE_VII_TIMING;
const TAG_V2_TYPE_VIII_TIMING: u8 = tag::V2_TYPE_VIII_TIMING;
const TAG_V2_TYPE_IX_TIMING: u8 = tag::V2_TYPE_IX_TIMING;
const TAG_V2_DYNAMIC_TIMING_RANGE: u8 = tag::V2_DYNAMIC_TIMING_RANGE;
const TAG_V2_INTERFACE_FEATURES: u8 = tag::V2_INTERFACE_FEATURES;
const TAG_V2_STEREO_INTERFACE: u8 = tag::V2_STEREO_INTERFACE;
const TAG_V2_TILED_TOPOLOGY: u8 = tag::V2_TILED_TOPOLOGY;
const TAG_V2_CONTAINER_ID: u8 = tag::V2_CONTAINER_ID;
const TAG_V2_VENDOR_SPECIFIC: u8 = tag::V2_VENDOR_SPECIFIC;
const TAG_V2_CTA_DISPLAYID: u8 = tag::V2_CTA_DISPLAYID;

/// Calls `f(tag, revision, block_payload)` for each well-formed data block in `payload`.
///
/// `revision` is the second byte of the 3-byte data block header and carries block-specific
/// flags (e.g., the code-space selector for Type IV timing blocks).
///
/// Stops at the end-of-section sentinel (tag `0x00`, length `0`) or when a block's
/// declared length would extend past the available payload.
///
/// Note: `TAG_PRODUCT_ID` (`0x00`) shares the sentinel's tag byte, but a valid Product
/// Identification Block always has a non-zero length, so the two are unambiguous.
fn for_each_data_block(payload: &[u8], mut f: impl FnMut(u8, u8, &[u8])) {
    let mut offset = 0;
    while offset + 3 <= payload.len() {
        let tag = payload[offset];
        let revision = payload[offset + 1];
        let length = payload[offset + 2] as usize;

        // End-of-section sentinel: tag 0x00 with length 0. Unambiguous because a valid
        // Product Identification Block (also tag 0x00) always has a non-zero length.
        if tag == 0x00 && length == 0 {
            break;
        }

        let block_end = offset + 3 + length;
        if block_end > payload.len() {
            // Malformed block — extends past payload; stop iterating.
            break;
        }

        f(tag, revision, &payload[offset + 3..block_end]);
        offset = block_end;
    }
}

/// Parses the 4-byte section header common to all DisplayID fragments.
///
/// Returns `(version, section_byte_count, product_type, extension_count)`.
/// - `version`: byte 1 of the block (DisplayID version/revision)
/// - `section_byte_count`: byte 2, count of data block bytes in this fragment
/// - `product_type`: bits 2:0 of byte 3 (display product primary use case)
/// - `extension_count`: bits 7:3 of byte 3 (number of continuation blocks after the first)
const fn parse_section_header(block: &[u8; 128]) -> (u8, u8, u8, u8) {
    let version = block[1];
    let section_byte_count = block[2];
    let packed = block[3];
    let product_type = packed & 0x07;
    let extension_count = (packed >> 3) & 0x1F;
    (version, section_byte_count, product_type, extension_count)
}

/// Returns the data-block payload slice for a single DisplayID fragment.
///
/// Extracts `block[4..end]` where `end = min(4 + section_byte_count, 127)`.
fn fragment_payload(block: &[u8; 128]) -> &[u8] {
    let section_byte_count = block[2] as usize;
    let end = (4 + section_byte_count).min(127);
    if end > 4 { &block[4..end] } else { &[] }
}

/// Validates the DisplayID section checksum for `block`.
///
/// Returns `None` when the checksum is valid (no warning to emit).
/// Returns `Some(EdidWarning::DisplayIdSectionBytesOutOfRange)` when `section_byte_count`
/// is so large that the checksum position falls outside bytes 1–126 of the block.
/// Returns `Some(EdidWarning::DisplayIdChecksumMismatch)` when the checksum byte does not
/// bring the sum of `block[1..=checksum_pos]` to zero mod 256.
///
/// The checksum byte sits at `block[4 + section_byte_count]`, immediately after the data
/// blocks. A valid section holds at most 122 data bytes, placing the checksum no later
/// than byte 126.
fn check_displayid_section(block: &[u8; 128]) -> Option<EdidWarning> {
    let n = block[2] as usize;
    let checksum_pos = 4 + n;
    if checksum_pos > 126 {
        return Some(EdidWarning::DisplayIdSectionBytesOutOfRange(block[2]));
    }
    let ok = block[1..=checksum_pos]
        .iter()
        .fold(0u8, |acc, &x| acc.wrapping_add(x))
        == 0;
    if ok {
        None
    } else {
        Some(EdidWarning::DisplayIdChecksumMismatch)
    }
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl ExtensionHandler for DisplayIdHandler {
    fn process(
        &self,
        blocks: &[&[u8; 128]],
        caps: &mut DisplayCapabilities,
        warnings: &mut Vec<ParseWarning>,
    ) {
        self.process_in_context(blocks, caps, warnings, &HandlerContext::default());
    }

    fn process_in_context(
        &self,
        blocks: &[&[u8; 128]],
        caps: &mut DisplayCapabilities,
        warnings: &mut Vec<ParseWarning>,
        ctx: &HandlerContext<'_>,
    ) {
        let options = ctx.options();
        let Some(first) = blocks.first() else { return };

        let (version, _section_byte_count, product_type, extension_count) =
            parse_section_header(first);

        // Validate version range.
        match version {
            DISPLAYID_V1_MIN..=DISPLAYID_V1_MAX | DISPLAYID_V2 => {}
            v => {
                warnings.push(Arc::new(EdidWarning::DisplayIdVersionUnknown(v)));
                return;
            }
        }

        // Validate extension count vs actual number of continuation blocks.
        let actual_continuation = blocks.len().saturating_sub(1);
        if extension_count as usize != actual_continuation {
            warnings.push(Arc::new(EdidWarning::DisplayIdExtensionCountMismatch {
                declared: extension_count,
                found: actual_continuation.min(u8::MAX as usize) as u8,
            }));
        }

        // Build rich capabilities locally; metadata decoders may populate fields on it
        // (e.g., `manufacturer_oui` from V2 block 0x20). Stored at the end so all fragments
        // contribute before the immutable Arc is constructed.
        let mut did = DisplayIdCapabilities::new(version, product_type);

        // Process data blocks from all fragments.
        for block in blocks {
            if let Some(w) = check_displayid_section(block) {
                warnings.push(Arc::new(w));
            }
            let payload = fragment_payload(block);
            process_data_blocks(payload, version, caps, options);
            scan_all_metadata_blocks(payload, version, caps, &mut did, warnings, options);
        }

        caps.set_extension_data(0x70, did);
    }
}

impl StaticExtensionHandler for DisplayIdHandler {
    fn tag(&self) -> u8 {
        0x70
    }

    fn process(&self, blocks: &[&[u8; 128]], ctx: &mut StaticContext<'_>) {
        let Some(first) = blocks.first() else { return };

        let (version, _section_byte_count, _product_type, extension_count) =
            parse_section_header(first);

        // Validate version range.
        match version {
            DISPLAYID_V1_MIN..=DISPLAYID_V1_MAX | DISPLAYID_V2 => {}
            v => {
                ctx.push_warning(EdidWarning::DisplayIdVersionUnknown(v));
                return;
            }
        }

        // Validate extension count vs actual number of continuation blocks.
        // In bare no_std the dispatch layer calls once per block, so blocks.len() == 1 always;
        // the mismatch warning may fire once per block in that case. In alloc builds the full
        // slice is provided and the warning fires at most once.
        let actual_continuation = blocks.len().saturating_sub(1);
        if extension_count as usize != actual_continuation {
            ctx.push_warning(EdidWarning::DisplayIdExtensionCountMismatch {
                declared: extension_count,
                found: actual_continuation.min(u8::MAX as usize) as u8,
            });
        }

        // Process data blocks from all fragments.
        let options = *ctx.options();
        for block in blocks {
            if let Some(w) = check_displayid_section(block) {
                ctx.push_warning(w);
            }
            process_data_blocks(fragment_payload(block), version, ctx, &options);
        }
    }
}

/// Data block tags decoded by this handler.
///
/// Must be kept in sync with the tag dispatches in `timing::process_data_blocks`
/// (timing blocks) and `metadata::scan_all_metadata_blocks` (metadata blocks).
/// `test_all_block_tags_accounted_for` verifies that the union of implemented,
/// deferred, and reserved ranges covers every value 0x00–0xFF.
#[cfg(test)]
const IMPLEMENTED_BLOCK_TAGS: &[u8] = &[
    TAG_PRODUCT_ID,               // 0x00 — Product Identification Block
    TAG_DISPLAY_PARAMS,           // 0x01 — Display Parameters Block
    TAG_COLOR_CHARACTERISTICS,    // 0x02 — Color Characteristics Block
    TAG_TYPE_I_TIMING,            // 0x03 — Detailed Timings Block (Type I descriptors)
    TAG_TYPE_II_TIMING,           // 0x04 — Video Timing Modes Type II — Detailed Timings Block
    TAG_TYPE_III_TIMING,          // 0x05 — Video Timing Modes Type III — Short Timings Block
    TAG_TYPE_IV_TIMING,           // 0x06 — Video Timing Modes Type IV — DMT/VIC Code Block
    TAG_VESA_VIDEO_TIMING,        // 0x07 — VESA Video Timing Block (DMT presence bitmap)
    TAG_CTA_VIDEO_TIMING,         // 0x08 — CTA-861 Video Timing Block (VIC presence bitmap)
    TAG_VIDEO_TIMING_RANGE,       // 0x09 — Video Timing Range Limits Block
    TAG_SERIAL_NUMBER,            // 0x0A — Product Serial Number Block
    TAG_ASCII_STRING,             // 0x0B — General Purpose ASCII String Block
    TAG_DISPLAY_DEVICE_DATA,      // 0x0C — Display Device Data Block
    TAG_POWER_SEQUENCING,         // 0x0D — Interface Power Sequencing Block
    TAG_TRANSFER_CHARACTERISTICS, // 0x0E — Transfer Characteristics Block
    TAG_DISPLAY_INTERFACE,        // 0x0F — Display Interface Data Block
    TAG_STEREO_DISPLAY_INTERFACE, // 0x10 — Stereo Display Interface Data Block
    TAG_TYPE_V_TIMING,            // 0x11 — Video Timing Modes Type V — Short Timings Block
    TAG_TILED_TOPOLOGY,           // 0x12 — Tiled Display Topology Data Block
    TAG_TYPE_VI_TIMING,           // 0x13 — Video Timing Modes Type VI — Detailed Timings Block
];

/// DisplayID 1.x data block tags that are defined by the specification but not
/// yet decoded, plus tag ranges reserved or unassigned by the specification.
///
/// Each entry is an inclusive `(first, last)` range. When a new block type is
/// implemented, remove its tag from here and add it to `IMPLEMENTED_BLOCK_TAGS`.
#[cfg(test)]
const DEFERRED_OR_RESERVED_TAG_RANGES: &[(u8, u8)] = &[
    (0x14, 0x7E), // Reserved for future use in DisplayID 1.x
    (0x7F, 0x7F), // Vendor-specific
    (0x80, 0xFF), // Undefined (outside the DisplayID 1.x tag space)
];

/// DisplayID 2.x data block tags decoded by this handler.
///
/// Tracked separately from [`IMPLEMENTED_BLOCK_TAGS`] because the 2.x tag space is
/// disjoint from 1.x and dispatched via a separate `(is_v2, tag)` arm. Empty until
/// Phase 2 of the 2.x rollout adds the first 2.x decoder.
#[cfg(test)]
const IMPLEMENTED_V2_BLOCK_TAGS: &[u8] = &[
    TAG_V2_PRODUCT_ID,           // 0x20
    TAG_V2_DISPLAY_PARAMS,       // 0x21
    TAG_V2_TYPE_VII_TIMING,      // 0x22
    TAG_V2_TYPE_VIII_TIMING,     // 0x23
    TAG_V2_TYPE_IX_TIMING,       // 0x24
    TAG_V2_DYNAMIC_TIMING_RANGE, // 0x25
    TAG_V2_INTERFACE_FEATURES,   // 0x26
    TAG_V2_STEREO_INTERFACE,     // 0x27
    TAG_V2_TILED_TOPOLOGY,       // 0x28
    TAG_V2_CONTAINER_ID,         // 0x29
    TAG_V2_VENDOR_SPECIFIC,      // 0x7E
    TAG_V2_CTA_DISPLAYID,        // 0x81
];

/// DisplayID 2.x block tags that are defined by the specification but not yet
/// decoded, plus tag ranges reserved or unassigned in the 2.x tag space.
///
/// Each entry is an inclusive `(first, last)` range. When a new 2.x block type is
/// implemented, remove its tag from here and add it to [`IMPLEMENTED_V2_BLOCK_TAGS`].
#[cfg(test)]
const DEFERRED_OR_RESERVED_V2_TAG_RANGES: &[(u8, u8)] = &[
    (0x00, 0x1F), // Outside the DisplayID 2.x tag space (covers 1.x range)
    (0x2A, 0x7D), // Reserved in DisplayID 2.x
    (0x7F, 0x80), // Reserved in DisplayID 2.x
    (0x82, 0xFF), // Reserved in DisplayID 2.x
];

/// Pre-built static reference to the built-in DisplayID handler.
///
/// Suitable for inclusion in a `&[&dyn StaticExtensionHandler]` slice alongside
/// [`CEA861_HANDLER`][crate::CEA861_HANDLER].
pub static DISPLAYID_HANDLER: &dyn StaticExtensionHandler = &DisplayIdHandler;

#[cfg(test)]
#[cfg(any(feature = "alloc", feature = "std"))]
mod tests {
    use super::*;
    use crate::model::extension::ExtensionHandler;
    use crate::model::manufacture::ManufacturerId;

    // -----------------------------------------------------------------------
    // Shared test helpers
    // -----------------------------------------------------------------------

    fn make_displayid_block(version: u8, data_blocks: &[u8]) -> [u8; 128] {
        let mut block = [0u8; 128];
        block[0] = 0x70; // extension tag
        block[1] = version;
        let section_byte_count = data_blocks.len().min(122);
        block[2] = section_byte_count as u8;
        block[3] = 0x00; // product_type=0, extension_count=0
        let end = 4 + section_byte_count;
        block[4..end].copy_from_slice(&data_blocks[..section_byte_count]);
        // Set DisplayID section checksum at block[4 + section_byte_count].
        let sum: u8 = block[1..end]
            .iter()
            .fold(0u8, |acc, &x| acc.wrapping_add(x));
        block[end] = 0u8.wrapping_sub(sum);
        // EDID extension block checksum at block[127] (covers all 128 bytes).
        let edid_sum: u8 = block[..127].iter().fold(0u8, |acc, &x| acc.wrapping_add(x));
        block[127] = 0u8.wrapping_sub(edid_sum);
        block
    }

    /// Recomputes the DisplayID section checksum and the EDID extension block checksum
    /// after any modification to the block's header or data bytes.
    fn fix_checksums(block: &mut [u8; 128]) {
        let n = block[2] as usize;
        let checksum_pos = 4 + n;
        if checksum_pos <= 126 {
            let sum: u8 = block[1..checksum_pos]
                .iter()
                .fold(0u8, |acc, &x| acc.wrapping_add(x));
            block[checksum_pos] = 0u8.wrapping_sub(sum);
        }
        let edid_sum: u8 = block[..127].iter().fold(0u8, |acc, &x| acc.wrapping_add(x));
        block[127] = 0u8.wrapping_sub(edid_sum);
    }

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

    fn make_type_i_data_block(descriptor: &[u8; 20]) -> [u8; 23] {
        let mut db = [0u8; 23];
        db[0] = TAG_TYPE_I_TIMING;
        db[1] = 0x00;
        db[2] = 20;
        db[3..23].copy_from_slice(descriptor);
        db
    }

    // -----------------------------------------------------------------------
    // Handler-level tests (warnings, capabilities, multi-fragment)
    // -----------------------------------------------------------------------

    #[test]
    fn test_unknown_version_emits_warning() {
        let block = make_displayid_block(0x05, &[]);
        let mut caps = DisplayCapabilities::default();
        let mut warnings: Vec<ParseWarning> = Vec::new();
        ExtensionHandler::process(&DisplayIdHandler, &[&block], &mut caps, &mut warnings);
        assert_eq!(warnings.len(), 1);
        let w = (*warnings[0]).downcast_ref::<EdidWarning>().unwrap();
        assert_eq!(*w, EdidWarning::DisplayIdVersionUnknown(0x05));
    }

    #[test]
    fn test_extension_count_mismatch_warning() {
        // Declare 1 continuation block but provide none.
        let mut block = make_displayid_block(0x10, &[]);
        block[3] = 0x08; // extension_count = 1, product_type = 0
        fix_checksums(&mut block);
        let mut caps = DisplayCapabilities::default();
        let mut warnings: Vec<ParseWarning> = Vec::new();
        ExtensionHandler::process(&DisplayIdHandler, &[&block], &mut caps, &mut warnings);
        assert_eq!(warnings.len(), 1);
        let w = (*warnings[0]).downcast_ref::<EdidWarning>().unwrap();
        assert_eq!(
            *w,
            EdidWarning::DisplayIdExtensionCountMismatch {
                declared: 1,
                found: 0
            }
        );
    }

    #[test]
    fn test_v2_accepted_without_warning() {
        let block = make_displayid_block(0x20, &[]);
        let mut caps = DisplayCapabilities::default();
        let mut warnings: Vec<ParseWarning> = Vec::new();
        ExtensionHandler::process(&DisplayIdHandler, &[&block], &mut caps, &mut warnings);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_displayid_capabilities_stored() {
        // version 0x13 (DisplayID 1.3), product_type = 2 (packed in byte 3 as 0x02)
        let mut block = make_displayid_block(0x13, &[]);
        block[3] = 0x02; // extension_count=0, product_type=2
        fix_checksums(&mut block);
        let mut caps = DisplayCapabilities::default();
        let mut warnings: Vec<ParseWarning> = Vec::new();
        ExtensionHandler::process(&DisplayIdHandler, &[&block], &mut caps, &mut warnings);
        assert!(warnings.is_empty());
        let did = caps
            .get_extension_data::<DisplayIdCapabilities>(0x70)
            .unwrap();
        assert_eq!(did.version, 0x13);
        assert_eq!(did.product_type, 2);
    }

    #[test]
    fn test_multi_fragment_reassembly() {
        // First fragment: 1920×1080@60. Declares one continuation block.
        let desc1 = make_type_i_descriptor(14850, 1920, 280, 88, 44, 1080, 45, 4, 5, 0x00);
        let db1 = make_type_i_data_block(&desc1);
        let mut block1 = make_displayid_block(0x10, &db1);
        block1[3] = 0x08; // extension_count=1 (bits 7:3), product_type=0
        fix_checksums(&mut block1);

        // Second fragment (continuation): 2560×1440@60.
        let desc2 = make_type_i_descriptor(22118, 2560, 440, 80, 32, 1440, 41, 4, 5, 0x00);
        let db2 = make_type_i_data_block(&desc2);
        let block2 = make_displayid_block(0x10, &db2);

        let mut caps = DisplayCapabilities::default();
        let mut warnings: Vec<ParseWarning> = Vec::new();
        ExtensionHandler::process(
            &DisplayIdHandler,
            &[&block1, &block2],
            &mut caps,
            &mut warnings,
        );

        assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
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
    fn test_valid_checksum_no_warning() {
        // make_displayid_block already sets a valid checksum; no warning expected.
        let block = make_displayid_block(0x10, &[]);
        let mut caps = DisplayCapabilities::default();
        let mut warnings: Vec<ParseWarning> = Vec::new();
        ExtensionHandler::process(&DisplayIdHandler, &[&block], &mut caps, &mut warnings);
        assert!(warnings.is_empty(), "unexpected warnings: {:?}", warnings);
    }

    #[test]
    fn test_invalid_checksum_emits_warning() {
        let mut block = make_displayid_block(0x10, &[]);
        // Corrupt the DisplayID section checksum (at block[4 + section_byte_count]).
        let n = block[2] as usize;
        block[4 + n] ^= 0xFF;
        // Also fix the EDID extension block checksum so parsing reaches our code.
        let edid_sum: u8 = block[..127].iter().fold(0u8, |acc, &x| acc.wrapping_add(x));
        block[127] = 0u8.wrapping_sub(edid_sum);

        let mut caps = DisplayCapabilities::default();
        let mut warnings: Vec<ParseWarning> = Vec::new();
        ExtensionHandler::process(&DisplayIdHandler, &[&block], &mut caps, &mut warnings);
        assert_eq!(warnings.len(), 1, "expected exactly one warning");
        let w = (*warnings[0]).downcast_ref::<EdidWarning>().unwrap();
        assert_eq!(*w, EdidWarning::DisplayIdChecksumMismatch);
    }

    #[test]
    fn test_invalid_checksum_static_pipeline_emits_warning() {
        let mut block = make_displayid_block(0x10, &[]);
        let n = block[2] as usize;
        block[4 + n] ^= 0xFF;
        let edid_sum: u8 = block[..127].iter().fold(0u8, |acc, &x| acc.wrapping_add(x));
        block[127] = 0u8.wrapping_sub(edid_sum);

        let mut caps = crate::model::StaticDisplayCapabilities::<4>::default();
        let mut ctx = crate::model::StaticContext::new(&mut caps);
        StaticExtensionHandler::process(&DisplayIdHandler, &[&block], &mut ctx);
        assert_eq!(caps.warnings(), [EdidWarning::DisplayIdChecksumMismatch]);
    }

    /// A DisplayID 2.x block with a CTA DisplayID block whose collection holds a VTB-EXT
    /// with one DTB: 1920×1080, blanking 280, front porch 88, sync 44, 8-pixel horizontal
    /// borders.
    fn displayid_with_bordered_vtb_ext() -> [u8; 128] {
        // Extended tag data block (23 bytes): VTB-EXT, version 1, one DTB.
        let vtb_ext_header = [0xE0 | 23, 0x03, 0x01, 0x01, 0x00, 0x00];
        let dtb = [
            0x02, 0x3A, 0x80, 0x18, 0x71, 0x38, 0x2D, 0x40, 88, 44, 0x45, 0, 0, 0, 0, 8, 0, 0x1E,
        ];
        let mut data_block = vec![TAG_V2_CTA_DISPLAYID, 0x00, 24];
        data_block.extend_from_slice(&vtb_ext_header);
        data_block.extend_from_slice(&dtb);
        make_displayid_block(DISPLAYID_V2, &data_block)
    }

    #[test]
    fn test_handler_applies_options_to_embedded_cta_dtds() {
        use crate::model::options::{DecodeOptions, DtdBorderPolicy, DtdBorderReading};

        let block = displayid_with_bordered_vtb_ext();
        let options = DecodeOptions::new()
            .with_dtd_border_policy(DtdBorderPolicy::Fixed(DtdBorderReading::Edid14));
        let mut caps = DisplayCapabilities::default();
        let mut warnings: Vec<ParseWarning> = Vec::new();
        ExtensionHandler::process_in_context(
            &DisplayIdHandler,
            &[&block],
            &mut caps,
            &mut warnings,
            &HandlerContext::new(options, &[1]),
        );

        // The block is decoded for its modes and for its CTA metadata; both read 1.4, so
        // they give one mode.
        assert_eq!(caps.supported_modes.len(), 1);
        let mode = &caps.supported_modes[0];
        assert_eq!(mode.dtd_border_reading, Some(DtdBorderReading::Edid14));
        assert_eq!(mode.h_blank, Some(280));
        assert!(caps.warnings.is_empty() && warnings.is_empty());
    }

    #[test]
    fn test_static_handler_applies_options_to_embedded_cta_dtds() {
        use crate::model::options::{DecodeOptions, DtdBorderPolicy, DtdBorderReading};

        let block = displayid_with_bordered_vtb_ext();

        let options = DecodeOptions::new()
            .with_dtd_border_policy(DtdBorderPolicy::Fixed(DtdBorderReading::Edid14));
        let mut caps = crate::model::StaticDisplayCapabilities::<4>::default();
        let mut ctx = crate::model::StaticContext::new(&mut caps).with_options(options);
        StaticExtensionHandler::process(&DisplayIdHandler, &[&block], &mut ctx);

        let mode = caps.iter_modes().next().unwrap();
        assert_eq!(mode.dtd_border_reading, Some(DtdBorderReading::Edid14));
        assert_eq!(mode.h_blank, Some(280));
        assert!(caps.warnings().is_empty());
    }

    #[test]
    #[allow(clippy::identity_op)] // PNP letter-to-bits encoding: (c - 'A' + 1) for symmetry.
    fn test_product_id_and_timing_in_same_block() {
        // Product ID block followed by a Type I timing block — both must be decoded.
        let ca = (b'S' - b'A' + 1) as u16;
        let cb = (b'A' - b'A' + 1) as u16;
        let cc = (b'M' - b'A' + 1) as u16;
        let packed: u16 = (ca << 10) | (cb << 5) | cc;
        let mut pid_payload = Vec::new();
        pid_payload.extend_from_slice(&packed.to_be_bytes());
        pid_payload.extend_from_slice(&0xABCDu16.to_le_bytes());
        pid_payload.extend_from_slice(&0u32.to_le_bytes()); // serial = 0
        pid_payload.push(0); // week
        pid_payload.push(0); // year
        let mut pid_db = vec![TAG_PRODUCT_ID, 0x00, pid_payload.len() as u8];
        pid_db.extend_from_slice(&pid_payload);

        let desc = make_type_i_descriptor(14850, 1920, 280, 88, 44, 1080, 45, 4, 5, 0x00);
        let timing_db = make_type_i_data_block(&desc);

        let mut payload = Vec::new();
        payload.extend_from_slice(&pid_db);
        payload.extend_from_slice(&timing_db);

        let block = make_displayid_block(0x10, &payload);
        let mut caps = DisplayCapabilities::default();
        let mut warnings: Vec<ParseWarning> = Vec::new();
        ExtensionHandler::process(&DisplayIdHandler, &[&block], &mut caps, &mut warnings);

        assert!(warnings.is_empty());
        assert_eq!(caps.manufacturer, Some(ManufacturerId(*b"SAM")));
        assert_eq!(caps.product_code, Some(0xABCD));
        assert_eq!(caps.supported_modes.len(), 1);
        assert_eq!(caps.supported_modes[0].width, 1920);
    }

    // -----------------------------------------------------------------------
    // Block tag coverage
    // -----------------------------------------------------------------------

    #[test]
    fn test_all_block_tags_accounted_for() {
        for tag in 0u16..=255 {
            let tag = tag as u8;
            let implemented = IMPLEMENTED_BLOCK_TAGS.contains(&tag);
            let deferred_or_reserved = DEFERRED_OR_RESERVED_TAG_RANGES
                .iter()
                .any(|&(lo, hi)| tag >= lo && tag <= hi);
            assert!(
                implemented || deferred_or_reserved,
                "DisplayID block tag 0x{:02X} is unaccounted for: \
                 add it to IMPLEMENTED_BLOCK_TAGS or DEFERRED_OR_RESERVED_TAG_RANGES",
                tag
            );
        }
    }

    #[test]
    fn test_implemented_and_deferred_are_disjoint() {
        for &tag in IMPLEMENTED_BLOCK_TAGS {
            let in_deferred = DEFERRED_OR_RESERVED_TAG_RANGES
                .iter()
                .any(|&(lo, hi)| tag >= lo && tag <= hi);
            assert!(
                !in_deferred,
                "DisplayID block tag 0x{:02X} appears in both IMPLEMENTED_BLOCK_TAGS \
                 and DEFERRED_OR_RESERVED_TAG_RANGES",
                tag
            );
        }
    }

    #[test]
    fn test_all_v2_block_tags_accounted_for() {
        for tag in 0u16..=255 {
            let tag = tag as u8;
            let implemented = IMPLEMENTED_V2_BLOCK_TAGS.contains(&tag);
            let deferred_or_reserved = DEFERRED_OR_RESERVED_V2_TAG_RANGES
                .iter()
                .any(|&(lo, hi)| tag >= lo && tag <= hi);
            assert!(
                implemented || deferred_or_reserved,
                "DisplayID 2.x block tag 0x{:02X} is unaccounted for: \
                 add it to IMPLEMENTED_V2_BLOCK_TAGS or DEFERRED_OR_RESERVED_V2_TAG_RANGES",
                tag
            );
        }
    }

    #[test]
    fn test_v2_implemented_and_deferred_are_disjoint() {
        for &tag in IMPLEMENTED_V2_BLOCK_TAGS {
            let in_deferred = DEFERRED_OR_RESERVED_V2_TAG_RANGES
                .iter()
                .any(|&(lo, hi)| tag >= lo && tag <= hi);
            assert!(
                !in_deferred,
                "DisplayID 2.x block tag 0x{:02X} appears in both IMPLEMENTED_V2_BLOCK_TAGS \
                 and DEFERRED_OR_RESERVED_V2_TAG_RANGES",
                tag
            );
        }
    }
}
