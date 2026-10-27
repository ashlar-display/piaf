mod base;
mod cea861;
mod displayid;

#[cfg(any(feature = "alloc", feature = "std"))]
pub use base::BaseBlockHandler;
pub use cea861::Cea861Flags;
#[cfg(any(feature = "alloc", feature = "std"))]
pub use cea861::{
    Audio3dChannels, Audio3dSpeakerAllocation, AudioFormat, AudioFormatInfo, AudioSampleRates,
    Cea861Capabilities, ColorimetryBlock, ColorimetryFlags, DtcPointEncoding, HdmiAudioBlock,
    HdmiDscMaxSlices, HdmiForumDsc, HdmiForumFrl, HdmiForumSinkCap, HdmiVsdb, HdmiVsdbFlags,
    HdrDynamicMetadataDescriptor, HdrEotf, HdrStaticMetadata, InfoFrameDescriptor,
    RoomConfigurationBlock, ShortAudioDescriptor, SpeakerAllocation, SpeakerAllocationFlags,
    SpeakerAllocationFlags2, SpeakerAllocationFlags3, SpeakerLocationEntry, T7VtdbBlock,
    T8VtdbBlock, T10VtdbBlock, T10VtdbEntry, VendorSpecificBlock, VesaDisplayDeviceBlock,
    VesaTransferCharacteristic, VideoCapability, VideoCapabilityFlags, VtbExtBlock, infoframe_type,
};
pub use cea861::{CEA861_HANDLER, Cea861Handler};
#[cfg(any(feature = "alloc", feature = "std"))]
pub use displayid::DisplayIdCapabilities;
pub use displayid::{DISPLAYID_HANDLER, DisplayIdHandler};

/// Pre-built static handler slice containing the standard built-in handlers
/// (CEA-861 and DisplayID).
///
/// Pass to [`parse_edid`][crate::parse_edid] and
/// [`capabilities_from_edid_static`] for the
/// common case where no custom extension handlers are needed:
///
/// ```no_run
/// use piaf::{parse_edid, capabilities_from_edid_static, StaticDisplayCapabilities, STANDARD_HANDLERS};
///
/// let bytes: &[u8] = &[/* raw EDID bytes */];
/// let parsed = parse_edid(bytes, STANDARD_HANDLERS).unwrap();
/// let caps: StaticDisplayCapabilities<64> =
///     capabilities_from_edid_static(&parsed, STANDARD_HANDLERS);
/// ```
pub static STANDARD_HANDLERS: &[&dyn StaticExtensionHandler] = &[&Cea861Handler, &DisplayIdHandler];

use crate::model::capabilities::{DisplayCapabilities, ModeSink, StaticDisplayCapabilities};
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::diagnostics::ParseWarning;
use crate::model::edid::EdidSource;
use crate::model::extension::{ExtensionLibrary, StaticExtensionHandler};
use crate::model::options::DecodeOptions;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::prelude::{Box, Vec};

/// A [`ModeSink`] that discards all output.
///
/// Used by [`capabilities_from_edid`] in bare `no_std` builds where `DisplayCapabilities`
/// carries no warning or mode storage. Warnings are unrecoverable in that path; use
/// [`capabilities_from_edid_static`] if warnings are needed in a bare `no_std` build.
#[cfg(not(any(feature = "alloc", feature = "std")))]
struct NullSink;

#[cfg(not(any(feature = "alloc", feature = "std")))]
impl ModeSink for NullSink {
    fn push_mode(&mut self, _: crate::model::capabilities::VideoMode) {}
    fn push_warning(&mut self, _: crate::model::diagnostics::EdidWarning) {}
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl ExtensionLibrary {
    /// Creates a library pre-loaded with the built-in [`BaseBlockHandler`] and [`Cea861Handler`].
    ///
    /// This is the recommended starting point for most consumers. Additional handlers can be
    /// added after construction via [`add_base_handler`][ExtensionLibrary::add_base_handler]
    /// and [`register`][ExtensionLibrary::register].
    pub fn with_standard_handlers() -> Self {
        let mut lib = Self::with_standard_extensions();
        lib.add_base_handler(BaseBlockHandler);
        if let Some(cea) = lib.extensions.iter_mut().find(|e| e.tag == 0x02) {
            cea.handler = Some(Box::new(Cea861Handler));
        }
        if let Some(did) = lib.extensions.iter_mut().find(|e| e.tag == 0x70) {
            did.handler = Some(Box::new(DisplayIdHandler));
        }
        lib
    }
}

/// Derives [`DisplayCapabilities`] from any [`EdidSource`] by running all registered handlers.
///
/// Base handlers are called first (in registration order), then extension block handlers are
/// called for each extension block whose tag matches a registered entry in `library`.
/// Warnings from all handlers are collected into [`DisplayCapabilities::warnings`].
///
/// Decodes with the default [`DecodeOptions`]; see [`capabilities_from_edid_with_options`]
/// to settle ambiguous readings.
pub fn capabilities_from_edid<T: EdidSource>(
    edid: &T,
    library: &ExtensionLibrary,
) -> DisplayCapabilities {
    capabilities_from_edid_with_options(edid, library, DecodeOptions::new())
}

/// [`capabilities_from_edid`] with the caller's [`DecodeOptions`].
///
/// Every handler receives the options through `ExtensionHandler::process_with_options`
/// (`alloc`/`std`), so one setting covers the whole EDID. A handler that does not override
/// it ignores them.
pub fn capabilities_from_edid_with_options<T: EdidSource>(
    edid: &T,
    library: &ExtensionLibrary,
    options: DecodeOptions,
) -> DisplayCapabilities {
    let mut caps = DisplayCapabilities::default();

    #[cfg(any(feature = "alloc", feature = "std"))]
    {
        let mut warnings: Vec<ParseWarning> = Vec::new();

        // 1. Process Base Block through all registered base handlers, in order.
        // Each handler receives a single-element slice containing the base block.
        let base = edid.base_block();
        for handler in &library.base_handlers {
            handler.process_with_options(&[base], &mut caps, &mut warnings, &options);
        }

        // 2. Dispatch extension blocks: for each registered handler, collect all
        // blocks with its tag in stream order and call the handler once with the
        // full slice. Handlers are responsible for any multi-block reassembly.
        for metadata in &library.extensions {
            if let Some(handler) = &metadata.handler {
                let blocks: Vec<&[u8; 128]> = edid
                    .extension_blocks()
                    .filter(|b| b[0] == metadata.tag)
                    .collect();
                if !blocks.is_empty() {
                    handler.process_with_options(&blocks, &mut caps, &mut warnings, &options);
                }
            }
        }

        edid.propagate_parse_warnings(&mut caps);
        caps.warnings.extend(warnings);
    }

    #[cfg(not(any(feature = "alloc", feature = "std")))]
    {
        // The base block's decoded fields do not depend on the options.
        let _ = (library, options);
        base::decode_base_block(edid.base_block(), &mut caps, &mut NullSink);
    }

    caps
}

/// Derives [`StaticDisplayCapabilities`] from any [`EdidSource`] without heap allocation.
///
/// `N` is the maximum number of video modes the result can hold; 64 is a reasonable default
/// for most displays. Modes beyond `N` are silently dropped; warnings beyond
/// [`MAX_STATIC_WARNINGS`][crate::MAX_STATIC_WARNINGS] are counted in
/// [`warnings_dropped`][StaticDisplayCapabilities::warnings_dropped].
///
/// Extension blocks are dispatched to the first handler in `handlers` whose
/// [`tag`][StaticExtensionHandler::tag] matches the block's tag byte. Only mode-producing
/// data (SVDs, DTDs, VTB-EXT timings) is extracted; rich metadata such as audio descriptors
/// and colorimetry blocks requires the alloc pipeline.
///
/// The result's warnings are the parser's (from [`EdidSource::parse_warnings`]) first, then
/// the base block's, then the handlers'. Its
/// [`warnings_dropped`][StaticDisplayCapabilities::warnings_dropped] includes the warnings
/// the parser could not store ([`EdidSource::parse_warnings_dropped`]).
///
/// This function is a replacement for [`capabilities_from_edid`], not a complement —
/// calling both on the same EDID will process extension blocks twice.
///
/// Decodes with the default [`DecodeOptions`]; see
/// [`capabilities_from_edid_static_with_options`] to settle ambiguous readings.
pub fn capabilities_from_edid_static<const N: usize, T: EdidSource>(
    parsed: &T,
    handlers: &[&dyn StaticExtensionHandler],
) -> StaticDisplayCapabilities<N> {
    capabilities_from_edid_static_with_options(parsed, handlers, DecodeOptions::new())
}

/// [`capabilities_from_edid_static`] with the caller's [`DecodeOptions`].
///
/// The options apply to the base block and reach every handler through
/// [`StaticContext::options`][crate::StaticContext::options], so one setting covers the whole EDID. A custom handler
/// applies them to the data it decodes.
pub fn capabilities_from_edid_static_with_options<const N: usize, T: EdidSource>(
    parsed: &T,
    handlers: &[&dyn StaticExtensionHandler],
    options: DecodeOptions,
) -> StaticDisplayCapabilities<N> {
    // Step 1 — Decode the base block into a temporary DisplayCapabilities.
    // This gives us all scalar fields and preferred_image_size_mm "for free" from the
    // existing base-block decoder, without duplicating its logic here.
    let mut base_caps = DisplayCapabilities::default();
    // Initialised early so it can serve as the warning sink in bare no_std builds.
    let mut caps = StaticDisplayCapabilities::<N>::default();

    // The parser's warnings first: they occurred before any block was decoded.
    for &warning in parsed.parse_warnings() {
        caps.push_warning(warning);
    }
    caps.add_warnings_dropped(parsed.parse_warnings_dropped());

    #[cfg(any(feature = "alloc", feature = "std"))]
    {
        use crate::model::extension::ExtensionHandler;
        let mut w: Vec<ParseWarning> = Vec::new();
        base::BaseBlockHandler.process_with_options(
            &[parsed.base_block()],
            &mut base_caps,
            &mut w,
            &options,
        );
        // base_caps now holds scalar fields, preferred_image_size_mm, and supported_modes.
        // The base block's warnings are `EdidWarning`s behind `Arc<dyn Error>`; copy them
        // into the static output, as the bare no_std path below records them directly.
        for warning in base_caps.warnings.iter().chain(w.iter()) {
            if let Some(&edid_warning) =
                (**warning).downcast_ref::<crate::model::diagnostics::EdidWarning>()
            {
                caps.push_warning(edid_warning);
            }
        }
    }

    #[cfg(not(any(feature = "alloc", feature = "std")))]
    {
        // Route warnings directly into caps so they are preserved in the static output.
        base::decode_base_block(parsed.base_block(), &mut base_caps, &mut caps);
    }

    // Step 2 — Copy all scalar fields from the temporary into the static output.
    caps.manufacturer = base_caps.manufacturer;
    caps.manufacture_date = base_caps.manufacture_date;
    caps.edid_version = base_caps.edid_version;
    caps.product_code = base_caps.product_code;
    caps.serial_number = base_caps.serial_number;
    caps.serial_number_string = base_caps.serial_number_string;
    caps.display_name = base_caps.display_name;
    caps.unspecified_text = base_caps.unspecified_text;
    caps.white_points = base_caps.white_points;
    caps.digital = base_caps.digital;
    caps.color_bit_depth = base_caps.color_bit_depth;
    caps.chromaticity = base_caps.chromaticity;
    caps.gamma = base_caps.gamma;
    caps.display_features = base_caps.display_features;
    caps.digital_color_encoding = base_caps.digital_color_encoding;
    caps.analog_color_type = base_caps.analog_color_type;
    caps.video_interface = base_caps.video_interface;
    caps.analog_sync_level = base_caps.analog_sync_level;
    caps.screen_size = base_caps.screen_size;
    caps.min_v_rate = base_caps.min_v_rate;
    caps.max_v_rate = base_caps.max_v_rate;
    caps.min_h_rate_khz = base_caps.min_h_rate_khz;
    caps.max_h_rate_khz = base_caps.max_h_rate_khz;
    caps.max_pixel_clock_mhz = base_caps.max_pixel_clock_mhz;
    caps.preferred_image_size_mm = base_caps.preferred_image_size_mm;
    caps.timing_formula = base_caps.timing_formula;
    caps.color_management = base_caps.color_management;

    // Step 3 — Populate modes.
    #[cfg(any(feature = "alloc", feature = "std"))]
    {
        // base_caps.supported_modes was populated by BaseBlockHandler above.
        for mode in base_caps.supported_modes {
            caps.push_mode(mode);
        }
    }

    #[cfg(not(any(feature = "alloc", feature = "std")))]
    {
        // No supported_modes in bare no_std DisplayCapabilities; call ungated functions directly.
        base::decode_base_modes(parsed.base_block(), &mut caps, &options);
    }

    // Step 4 — Dispatch extension blocks to the static handler slice.
    //
    // In alloc/std builds, collect all blocks per handler first and call once with the
    // full slice, so multi-block formats (e.g. DisplayID) receive all their fragments
    // together for reassembly.
    //
    // In bare no_std builds there is no Vec, so each block is passed individually as a
    // single-element slice. Single-block formats (CEA-861) work correctly. Multi-block
    // formats receive one call per fragment and are responsible for handling that gracefully.
    #[cfg(any(feature = "alloc", feature = "std"))]
    {
        use crate::model::capabilities::StaticContext;
        for handler in handlers {
            let blocks: Vec<&[u8; 128]> = parsed
                .extension_blocks()
                .filter(|b| b[0] == handler.tag())
                .collect();
            if !blocks.is_empty() {
                let mut ctx = StaticContext::new(&mut caps).with_options(options);
                handler.process(&blocks, &mut ctx);
            }
        }
    }

    #[cfg(not(any(feature = "alloc", feature = "std")))]
    {
        use crate::model::capabilities::StaticContext;
        for ext in parsed.extension_blocks() {
            if let Some(handler) = handlers.iter().find(|h| h.tag() == ext[0]) {
                let mut ctx = StaticContext::new(&mut caps).with_options(options);
                handler.process(&[ext], &mut ctx);
            }
        }
    }

    caps
}

#[cfg(test)]
#[cfg(any(feature = "alloc", feature = "std"))]
mod tests {
    use super::*;
    use crate::model::extension::ExtensionTagRegistry;
    use crate::parser::parse_edid;

    /// The modes with the 2-pixel borders of `static_tests::edid_with_bordered_dtds`, and
    /// the number of `DtdBordersAmbiguous` warnings.
    fn bordered_modes_and_ambiguous(
        caps: &DisplayCapabilities,
    ) -> (Vec<&crate::model::capabilities::VideoMode>, usize) {
        use crate::model::diagnostics::EdidWarning;
        let modes = caps
            .supported_modes
            .iter()
            .filter(|m| m.h_border == 2)
            .collect();
        let ambiguous = caps
            .iter_warnings()
            .filter_map(|w| (**w).downcast_ref::<EdidWarning>())
            .filter(|w| matches!(w, EdidWarning::DtdBordersAmbiguous { .. }))
            .count();
        (modes, ambiguous)
    }

    #[test]
    fn test_default_options_leave_border_reading_unsettled() {
        let bytes = super::static_tests::edid_with_bordered_dtds();
        let parsed = parse_edid(&bytes, STANDARD_HANDLERS).unwrap();
        let caps = capabilities_from_edid(&parsed, &ExtensionLibrary::with_standard_handlers());

        let (modes, ambiguous) = bordered_modes_and_ambiguous(&caps);
        assert_eq!(modes.len(), 3);
        assert!(modes.iter().all(|m| m.dtd_border_reading.is_none()));
        assert_eq!(ambiguous, 3);
    }

    #[test]
    fn test_options_reach_every_dtd() {
        use crate::model::options::{DtdBorderPolicy, DtdBorderReading};

        let bytes = super::static_tests::edid_with_bordered_dtds();
        let parsed = parse_edid(&bytes, STANDARD_HANDLERS).unwrap();
        let options = DecodeOptions::new()
            .with_dtd_border_policy(DtdBorderPolicy::Fixed(DtdBorderReading::Edid14));
        let caps = capabilities_from_edid_with_options(
            &parsed,
            &ExtensionLibrary::with_standard_handlers(),
            options,
        );

        // The base block, the extension's DTD and its VTB-EXT DTB all read 1.4.
        let (modes, ambiguous) = bordered_modes_and_ambiguous(&caps);
        assert_eq!(modes.len(), 3);
        for mode in &modes {
            assert_eq!(mode.dtd_border_reading, Some(DtdBorderReading::Edid14));
        }
        let mode_1440 = modes.iter().find(|m| m.height == 1440).unwrap();
        assert_eq!(mode_1440.h_blank, Some(160));
        assert_eq!(ambiguous, 0);
    }

    // Verifies that with_standard_handlers() wires BaseBlockHandler into the pipeline.
    // Handler-level assertions live in base.rs and cea861.rs.
    #[test]
    fn test_standard_handlers_are_wired() {
        let mut bytes = [0u8; 128];
        bytes[0..8].copy_from_slice(&crate::parser::EDID_HEADER);
        bytes[0x14] = 0x80; // Digital input flag

        let mut sum = 0u8;
        for &b in bytes[..127].iter() {
            sum = sum.wrapping_add(b);
        }
        bytes[127] = 0u8.wrapping_sub(sum);

        let registry = ExtensionTagRegistry::new();
        let parsed = parse_edid(&bytes, &registry).unwrap();
        let caps = capabilities_from_edid(&parsed, &ExtensionLibrary::with_standard_handlers());

        assert!(caps.digital);
    }

    /// The static pipeline keeps the base block's warnings in alloc builds too (it used to
    /// drop them there, while bare no_std builds kept them).
    #[test]
    fn test_static_pipeline_keeps_base_block_warnings() {
        use crate::model::diagnostics::EdidWarning;
        let mut bytes = [0u8; 128];
        bytes[0..8].copy_from_slice(&crate::parser::EDID_HEADER);
        bytes[0x14] = 0x80;
        // DTD at 0x36: 1920×1080, 148.50 MHz, blanking 280 × 45, borders 8 / 2.
        bytes[0x36..0x48].copy_from_slice(&[
            0x02, 0x3A, 0x80, 0x18, 0x71, 0x38, 0x2D, 0x40, 88, 44, 0x45, 0x00, 0, 0, 0, 8, 2, 0x1E,
        ]);
        let sum = bytes[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        bytes[127] = 0u8.wrapping_sub(sum);

        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(&parsed, &[]);
        assert!(caps.iter_warnings().any(|w| matches!(
            w,
            EdidWarning::DtdBordersAmbiguous {
                h_border: 8,
                v_border: 2
            }
        )));
        let dynamic = capabilities_from_edid(&parsed, &ExtensionLibrary::with_standard_handlers());
        assert_eq!(
            caps.iter_warnings().count(),
            dynamic.iter_warnings().count()
        );
    }
}

// Builds in every configuration, so the bare no_std path is covered too.
#[cfg(test)]
mod static_tests {
    use super::*;
    use crate::model::diagnostics::EdidWarning;
    use crate::model::extension::ExtensionTagRegistry;
    use crate::parser::parse_edid;

    // With the size mismatch, more parse warnings than `ParsedEdidRef` holds in bare no_std.
    const EXTENSIONS: usize = 10;

    /// A base block with an all-zero (invalid) manufacturer ID and `EXTENSIONS` blocks with
    /// the unknown tag `0x55`, followed by one stray byte.
    fn edid_with_unknown_extensions() -> [u8; 128 * (1 + EXTENSIONS) + 1] {
        let mut bytes = [0u8; 128 * (1 + EXTENSIONS) + 1];
        bytes[0..8].copy_from_slice(&crate::parser::EDID_HEADER);
        bytes[126] = EXTENSIONS as u8;
        for (i, block) in bytes.chunks_exact_mut(128).enumerate() {
            if i > 0 {
                block[0] = 0x55;
            }
            let sum = block[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
            block[127] = 0u8.wrapping_sub(sum);
        }
        bytes
    }

    #[test]
    fn test_static_pipeline_keeps_parse_warnings_first() {
        let bytes = edid_with_unknown_extensions();
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(&parsed, &[]);

        let mut expected = [EdidWarning::UnknownExtension(0x55); 8];
        expected[0] = EdidWarning::SizeMismatch {
            expected: 128 * (1 + EXTENSIONS),
            actual: bytes.len(),
        };
        assert_eq!(caps.warnings(), expected);
        // 3 unknown extensions and the base block's `InvalidManufacturerId` did not fit,
        // whether the parser (bare no_std) or the static output (alloc) dropped them.
        assert_eq!(caps.warnings_dropped(), 4);
    }

    /// An 18-byte DTD with 2-pixel horizontal borders, V front porch 4 and V sync 5.
    fn bordered_dtd(clk: u16, h: (u16, u16), v: (u16, u16), fp: u8, sync: u8) -> [u8; 18] {
        let mut d = [0u8; 18];
        d[..2].copy_from_slice(&clk.to_le_bytes());
        d[2] = h.0 as u8;
        d[3] = h.1 as u8;
        d[4] = ((h.0 >> 8) as u8) << 4 | (h.1 >> 8) as u8;
        d[5] = v.0 as u8;
        d[6] = v.1 as u8;
        d[7] = ((v.0 >> 8) as u8) << 4 | (v.1 >> 8) as u8;
        d[8] = fp;
        d[9] = sync;
        d[10] = 0x45;
        d[15] = 2;
        d[17] = 0x1E;
        d
    }

    /// A bordered DTD in the base block (1920×1080), in a CTA-861 extension's VTB-EXT data
    /// block (1280×720) and in its DTD area (2560×1440).
    pub(super) fn edid_with_bordered_dtds() -> [u8; 256] {
        let mut bytes = [0u8; 256];
        bytes[0..8].copy_from_slice(&crate::parser::EDID_HEADER);
        bytes[0x36..0x48].copy_from_slice(&bordered_dtd(14850, (1920, 280), (1080, 45), 88, 44));
        bytes[126] = 1;
        let ext = &mut bytes[128..];
        ext[..4].copy_from_slice(&[0x02, 0x03, 28, 0]); // DTDs from byte 28
        // Extended tag data block (tag 7, 23 bytes): VTB-EXT, version 1, one DTB.
        ext[4..10].copy_from_slice(&[0xE0 | 23, 0x03, 0x01, 0x01, 0x00, 0x00]);
        ext[10..28].copy_from_slice(&bordered_dtd(7425, (1280, 370), (720, 30), 110, 40));
        ext[28..46].copy_from_slice(&bordered_dtd(24150, (2560, 160), (1440, 41), 48, 32));
        for block in bytes.chunks_exact_mut(128) {
            let sum = block[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
            block[127] = 0u8.wrapping_sub(sum);
        }
        bytes
    }

    // VTB-EXT is decoded in alloc builds only.
    const BORDERED_MODES: usize = if cfg!(any(feature = "alloc", feature = "std")) {
        3
    } else {
        2
    };

    fn ambiguous_warnings<const N: usize>(caps: &StaticDisplayCapabilities<N>) -> usize {
        caps.iter_warnings()
            .filter(|w| matches!(w, EdidWarning::DtdBordersAmbiguous { .. }))
            .count()
    }

    #[test]
    fn test_static_pipeline_default_options_leave_border_reading_unsettled() {
        let bytes = edid_with_bordered_dtds();
        let parsed = parse_edid(&bytes, STANDARD_HANDLERS).unwrap();
        let caps = capabilities_from_edid_static::<16, _>(&parsed, STANDARD_HANDLERS);

        let bordered: usize = caps.iter_modes().filter(|m| m.h_border == 2).count();
        assert_eq!(bordered, BORDERED_MODES);
        assert!(caps.iter_modes().all(|m| m.dtd_border_reading.is_none()));
        assert_eq!(ambiguous_warnings(&caps), BORDERED_MODES);
    }

    #[test]
    fn test_static_pipeline_options_reach_every_dtd() {
        use crate::model::options::{DtdBorderPolicy, DtdBorderReading};

        let bytes = edid_with_bordered_dtds();
        let parsed = parse_edid(&bytes, STANDARD_HANDLERS).unwrap();
        let options = DecodeOptions::new()
            .with_dtd_border_policy(DtdBorderPolicy::Fixed(DtdBorderReading::Edid14));
        let caps = capabilities_from_edid_static_with_options::<16, _>(
            &parsed,
            STANDARD_HANDLERS,
            options,
        );

        // The base block, the extension's DTD and (alloc) its VTB-EXT DTB all read 1.4.
        let settled = caps
            .iter_modes()
            .filter(|m| m.h_border == 2)
            .inspect(|m| assert_eq!(m.dtd_border_reading, Some(DtdBorderReading::Edid14)))
            .count();
        assert_eq!(settled, BORDERED_MODES);
        let mode_1440 = caps.iter_modes().find(|m| m.height == 1440).unwrap();
        assert_eq!(mode_1440.h_blank, Some(160));
        assert_eq!(ambiguous_warnings(&caps), 0);
    }
}
