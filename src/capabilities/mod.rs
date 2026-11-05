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
pub use displayid::{DISPLAYID_HANDLER, DisplayIdHandler};
#[cfg(any(feature = "alloc", feature = "std"))]
pub use displayid::{DisplayIdCapabilities, DisplayIdImageSize, ImageSizeUnit};

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
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::extension::HandlerContext;
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
/// Every handler receives the options through `ExtensionHandler::process_in_context`
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
        let base_ctx = HandlerContext::new(options, &[0]);
        for handler in &library.base_handlers {
            handler.process_in_context(&[base], &mut caps, &mut warnings, &base_ctx);
        }

        // 2. Dispatch extension blocks: for each registered handler, collect all
        // blocks with its tag in stream order and call the handler once with the
        // full slice. Handlers are responsible for any multi-block reassembly.
        for metadata in &library.extensions {
            if let Some(handler) = &metadata.handler {
                let (blocks, numbers) = blocks_with_tag(edid, metadata.tag);
                if !blocks.is_empty() {
                    let ctx = HandlerContext::new(options, &numbers);
                    handler.process_in_context(&blocks, &mut caps, &mut warnings, &ctx);
                }
            }
        }

        edid.propagate_parse_warnings(&mut caps);
        caps.warnings.extend(warnings);
    }

    #[cfg(not(any(feature = "alloc", feature = "std")))]
    {
        // No handlers without `alloc`: the base block is decoded directly. The options
        // reach the preferred timing, the one decoded mode this output holds.
        let _ = library;
        base::decode_base_block(edid.base_block(), &mut caps, &mut NullSink);
        caps.preferred_timing =
            base::decode_preferred_timing(edid.base_block(), &options, &mut NullSink);
    }

    caps
}

/// Derives [`StaticDisplayCapabilities`] from any [`EdidSource`] without heap allocation.
///
/// `N` is the maximum number of video modes the result can hold; 64 is a reasonable default
/// for most displays. Modes beyond `N` are counted in
/// [`modes_dropped`][StaticDisplayCapabilities::modes_dropped]; warnings beyond
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
        base::BaseBlockHandler.process_in_context(
            &[parsed.base_block()],
            &mut base_caps,
            &mut w,
            &HandlerContext::new(options, &[0]),
        );
        // base_caps now holds scalar fields, preferred_image_size_mm, and supported_modes.
        // The base block's warnings are `EdidWarning`s behind `Arc<dyn Error>`; copy them
        // into the static output, as the bare no_std path below records them directly. In
        // the same order as that path (header and preferred-timing warnings, collected in
        // `w`, before the DTDs'), so a full warning list keeps the same warnings in every
        // build.
        for warning in w.iter().chain(base_caps.warnings.iter()) {
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
    // Set by BaseBlockHandler with alloc; in bare no_std builds by decode_base_modes (step 3).
    caps.preferred_timing = base_caps.preferred_timing;
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
        caps.preferred_timing = base::decode_base_modes(parsed.base_block(), &mut caps, &options);
        // After the DTDs' warnings, as BaseBlockHandler reports it with `alloc`.
        if let (Some(mode), Some(size)) = (&caps.preferred_timing, caps.preferred_image_size_mm) {
            if let Some(w) = base::preferred_image_shape_warning(mode, size) {
                caps.push_warning(w);
            }
        }
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
            let (blocks, numbers) = blocks_with_tag(parsed, handler.tag());
            if !blocks.is_empty() {
                let mut ctx = StaticContext::new(&mut caps)
                    .with_options(options)
                    .with_block_numbers(&numbers);
                handler.process(&blocks, &mut ctx);
            }
        }
    }

    #[cfg(not(any(feature = "alloc", feature = "std")))]
    {
        use crate::model::capabilities::StaticContext;
        for (ext, number) in parsed.extension_blocks().zip(1..=u8::MAX) {
            if let Some(handler) = handlers.iter().find(|h| h.tag() == ext[0]) {
                let numbers = [number];
                let mut ctx = StaticContext::new(&mut caps)
                    .with_options(options)
                    .with_block_numbers(&numbers);
                handler.process(&[ext], &mut ctx);
            }
        }
    }

    caps
}

/// The extension blocks with `tag`, in stream order, and the EDID block number of each.
///
/// Extension `n` (from 1) is block `n`. An EDID's extension count is one byte, so its blocks
/// are numbered up to 255; a source yielding more extensions than that is cut off there.
#[cfg(any(feature = "alloc", feature = "std"))]
fn blocks_with_tag<T: EdidSource>(edid: &T, tag: u8) -> (Vec<&[u8; 128]>, Vec<u8>) {
    edid.extension_blocks()
        .zip(1..=u8::MAX)
        .filter(|(b, _)| b[0] == tag)
        .unzip()
}

#[cfg(test)]
#[cfg(any(feature = "alloc", feature = "std"))]
mod tests {
    use super::*;
    use crate::model::capabilities::{ModeSource, VideoMode};
    use crate::model::extension::{ExtensionHandler, ExtensionTagRegistry};
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

    /// An EDID with byte 126 = `declared` and the two CTA-861 revision 3 extensions
    /// `blocks`, each with an HF-EEODB as its first data block when its entry is
    /// `Some(count)`.
    fn edid_with_cta_blocks(declared: u8, blocks: [Option<u8>; 2]) -> [u8; 128 * 3] {
        let mut bytes = [0u8; 128 * 3];
        bytes[0..8].copy_from_slice(&crate::parser::EDID_HEADER);
        bytes[0x12] = 1;
        bytes[126] = declared;
        for (i, block) in bytes.chunks_exact_mut(128).enumerate() {
            if let Some(hf_eeodb) = i.checked_sub(1).map(|n| blocks[n]) {
                block[0] = 0x02;
                block[1] = 3;
                block[2] = 4; // no data blocks
                if let Some(count) = hf_eeodb {
                    block[2] = 7;
                    block[4] = (7 << 5) | 2;
                    block[5] = 0x78;
                    block[6] = count;
                }
            }
            let sum = block[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
            block[127] = 0u8.wrapping_sub(sum);
        }
        bytes
    }

    fn hf_eeodb_of(bytes: &[u8]) -> Option<u8> {
        let library = ExtensionLibrary::with_standard_handlers();
        let parsed = parse_edid(bytes, &library).unwrap();
        let caps = capabilities_from_edid(&parsed, &library);
        caps.get_extension_data::<crate::capabilities::Cea861Capabilities>(0x02)
            .unwrap()
            .hf_eeodb_extension_count
    }

    #[test]
    fn test_capabilities_take_hf_eeodb_from_block_1_only() {
        assert_eq!(
            hf_eeodb_of(&edid_with_cta_blocks(1, [Some(2), None])),
            Some(2)
        );
        // An HF-EEODB in block 2 declares nothing, as for `parse_edid`.
        assert_eq!(hf_eeodb_of(&edid_with_cta_blocks(2, [None, Some(2)])), None);
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
    /// Pushes a mode `n` pixels wide for each block, `n` being the block's number, or a
    /// mode 0 pixels high when the context has none.
    #[derive(Debug)]
    struct BlockNumberRecorder;

    impl ExtensionHandler for BlockNumberRecorder {
        fn process(
            &self,
            _: &[&[u8; 128]],
            _: &mut DisplayCapabilities,
            _: &mut Vec<ParseWarning>,
        ) {
        }

        fn process_in_context(
            &self,
            blocks: &[&[u8; 128]],
            caps: &mut DisplayCapabilities,
            _: &mut Vec<ParseWarning>,
            ctx: &HandlerContext<'_>,
        ) {
            for i in 0..blocks.len() {
                let mode = match ctx.block_number(i) {
                    Some(n) => VideoMode::new(n.into(), 1, 60u32, false),
                    None => VideoMode::new(0, 0, 60u32, false),
                };
                caps.supported_modes.push(mode);
            }
        }
    }

    #[test]
    fn test_dispatch_numbers_blocks() {
        use crate::model::extension::ExtensionMetadata;

        let mut library = ExtensionLibrary::new();
        library.add_base_handler(BlockNumberRecorder);
        library.register(ExtensionMetadata {
            tag: 0x55,
            display_name: "Recorder".into(),
            handler: Some(Box::new(BlockNumberRecorder)),
        });
        let bytes = super::static_tests::edid_with_tagged_extensions();
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid(&parsed, &library);

        let numbers: Vec<(u16, u16)> = caps
            .supported_modes
            .iter()
            .map(|m| (m.width, m.height))
            .collect();
        assert_eq!(numbers, [(0, 1), (1, 1), (3, 1)]);
    }

    /// The DTD sources, with each mode's width and horizontal border.
    fn dtd_sources(caps: &DisplayCapabilities) -> Vec<(u16, u8, Option<ModeSource>)> {
        caps.supported_modes
            .iter()
            .filter(|m| m.source.is_none() || matches!(m.source, Some(ModeSource::Dtd { .. })))
            .map(|m| (m.width, m.h_border, m.source))
            .collect()
    }

    #[test]
    fn test_dtd_sources_name_their_block() {
        let bytes = super::static_tests::edid_with_repeated_preferred_dtd();
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid(&parsed, &ExtensionLibrary::with_standard_handlers());

        // The extension's repeat of the preferred timing leaves the base block's entry,
        // with its borders and source, in place.
        assert_eq!(
            dtd_sources(&caps),
            [
                (1920, 2, Some(ModeSource::Dtd { block: 0, index: 0 })),
                (2560, 2, Some(ModeSource::Dtd { block: 1, index: 1 })),
            ]
        );
    }

    #[test]
    fn test_dtd_without_block_number_has_no_source() {
        use crate::capabilities::cea861::Cea861Handler;

        let bytes = super::static_tests::edid_with_repeated_preferred_dtd();
        let ext: &[u8; 128] = bytes[128..].try_into().unwrap();
        let mut caps = DisplayCapabilities::default();
        ExtensionHandler::process(&Cea861Handler, &[ext], &mut caps, &mut Vec::new());

        assert_eq!(dtd_sources(&caps), [(1920, 0, None), (2560, 2, None)]);
    }

    #[test]
    fn test_default_context_has_no_block_numbers() {
        let ctx = HandlerContext::default();
        assert_eq!(ctx.block_number(0), None);
        assert_eq!(ctx.options(), &DecodeOptions::new());

        let ctx = HandlerContext::new(DecodeOptions::new(), &[1, 3]);
        assert_eq!((ctx.block_number(1), ctx.block_number(2)), (Some(3), None));
    }

    #[test]
    fn test_standard_handlers_are_wired() {
        let mut bytes = [0u8; 128];
        bytes[0..8].copy_from_slice(&crate::parser::EDID_HEADER);
        bytes[0x12] = 1; // EDID 1.x
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
        bytes[0x12] = 1; // EDID 1.x
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
    use crate::model::capabilities::{ModeSource, StaticContext, VideoMode};
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
        bytes[0x12] = 1; // EDID 1.x
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

    /// A base block and three extensions tagged `0x55`, `0x66`, `0x55`: blocks 1 and 3 are
    /// `0x55`.
    pub(super) fn edid_with_tagged_extensions() -> [u8; 512] {
        let mut bytes = [0u8; 512];
        bytes[0..8].copy_from_slice(&crate::parser::EDID_HEADER);
        bytes[0x12] = 1; // EDID 1.x
        bytes[126] = 3;
        for (i, block) in bytes.chunks_exact_mut(128).enumerate() {
            block[0] = match i {
                0 => block[0],
                2 => 0x66,
                _ => 0x55,
            };
            let sum = block[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
            block[127] = 0u8.wrapping_sub(sum);
        }
        bytes
    }

    /// Pushes a mode `n` pixels wide for each block, `n` being the block's number.
    struct StaticBlockNumberRecorder;

    impl StaticExtensionHandler for StaticBlockNumberRecorder {
        fn tag(&self) -> u8 {
            0x55
        }

        fn process(&self, blocks: &[&[u8; 128]], ctx: &mut StaticContext<'_>) {
            for i in 0..blocks.len() {
                let n = ctx
                    .block_number(i)
                    .expect("the dispatch numbers every block");
                ctx.push_mode(VideoMode::new(n.into(), 1, 60u32, false));
            }
        }
    }

    #[test]
    fn test_static_pipeline_numbers_extension_blocks() {
        let bytes = edid_with_tagged_extensions();
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(&parsed, &[&StaticBlockNumberRecorder]);

        assert!(caps.iter_modes().map(|m| m.width).eq([1, 3]));
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
        bytes[0x12] = 1; // EDID 1.x
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

    /// A bordered 1920×1080 DTD in the base block, and a CTA-861 extension (block 1) whose
    /// DTDs are the same timing without borders, then a bordered 2560×1440.
    pub(super) fn edid_with_repeated_preferred_dtd() -> [u8; 256] {
        let mut bytes = [0u8; 256];
        bytes[0..8].copy_from_slice(&crate::parser::EDID_HEADER);
        bytes[0x12..0x14].copy_from_slice(&[1, 4]); // EDID 1.4: base DTD 0 is preferred
        let preferred = bordered_dtd(14850, (1920, 280), (1080, 45), 88, 44);
        bytes[0x36..0x48].copy_from_slice(&preferred);
        bytes[126] = 1;
        let ext = &mut bytes[128..];
        ext[..4].copy_from_slice(&[0x02, 0x03, 4, 0]); // DTDs from byte 4
        ext[4..22].copy_from_slice(&preferred);
        ext[4 + 15] = 0; // no borders
        ext[22..40].copy_from_slice(&bordered_dtd(24150, (2560, 160), (1440, 41), 48, 32));
        for block in bytes.chunks_exact_mut(128) {
            let sum = block[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
            block[127] = 0u8.wrapping_sub(sum);
        }
        bytes
    }

    #[test]
    fn test_static_pipeline_reports_the_preferred_image_size() {
        let mut bytes = edid_with_repeated_preferred_dtd();
        bytes[0x36 + 12..0x36 + 15].copy_from_slice(&[0x3F, 0x43, 0x21]); // 575 × 323 mm
        let sum = bytes[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        bytes[127] = 0u8.wrapping_sub(sum);
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(&parsed, &[]);

        assert_eq!(caps.preferred_image_size_mm, Some((575, 323)));
    }

    #[test]
    fn test_static_pipeline_dtd_sources_name_their_block() {
        let bytes = edid_with_repeated_preferred_dtd();
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(
            &parsed,
            &[crate::capabilities::cea861::CEA861_HANDLER],
        );

        let dtds = caps
            .iter_modes()
            .filter(|m| matches!(m.source, Some(ModeSource::Dtd { .. })))
            .map(|m| (m.width, m.h_border, m.source));
        assert!(dtds.eq([
            (1920, 2, Some(ModeSource::Dtd { block: 0, index: 0 })),
            (2560, 2, Some(ModeSource::Dtd { block: 1, index: 1 })),
        ]));
    }

    fn with_checksums(mut bytes: [u8; 256]) -> [u8; 256] {
        for block in bytes.chunks_exact_mut(128) {
            let sum = block[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
            block[127] = 0u8.wrapping_sub(sum);
        }
        bytes
    }

    // The preferred timing in the static pipeline: these run in every build, bare `no_std`
    // included, where it comes from `decode_base_modes` rather than `BaseBlockHandler`.

    #[test]
    fn test_static_pipeline_sets_the_preferred_timing() {
        let bytes = edid_with_repeated_preferred_dtd();
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(&parsed, &[]);

        let preferred = caps.preferred_timing.as_ref().unwrap();
        assert_eq!((preferred.width, preferred.h_border), (1920, 2));
        assert_eq!(
            preferred.source,
            Some(ModeSource::Dtd { block: 0, index: 0 })
        );
        assert!(caps.iter_modes().any(|m| m == preferred));
        // Its border warning is reported once, not once per use.
        assert_eq!(ambiguous_warnings(&caps), 1);
    }

    #[test]
    fn test_static_pipeline_no_preferred_timing_in_edid_1_3_without_the_bit() {
        let mut bytes = edid_with_repeated_preferred_dtd();
        bytes[0x13] = 3;
        bytes[0x36 + 12..0x36 + 15].copy_from_slice(&[0x3F, 0x43, 0x21]); // 575 × 323 mm
        let bytes = with_checksums(bytes);
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(&parsed, &[]);

        assert_eq!(caps.preferred_timing, None);
        assert_eq!(caps.preferred_image_size_mm, None);
        assert!(caps.iter_modes().any(|m| m.width == 1920)); // still decoded as a mode
    }

    #[test]
    fn test_static_pipeline_keeps_the_preferred_timing_when_the_mode_list_is_full() {
        let mut bytes = edid_with_repeated_preferred_dtd();
        bytes[0x23] = 0b0000_0001; // established 800×600 @ 60 Hz, decoded before the DTD
        let bytes = with_checksums(bytes);
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<1, _>(&parsed, &[]);

        assert_eq!(caps.num_modes, 1);
        assert!(caps.iter_modes().all(|m| m.width == 800));
        assert_eq!(caps.preferred_timing.map(|m| m.width), Some(1920));
    }

    /// A base block of the given EDID revision and feature byte, with a 1920×1080 timing in
    /// `timing_slot` (if any) and a monitor name in every other slot before it.
    fn base_with_timing_in(revision: u8, features: u8, timing_slot: Option<usize>) -> [u8; 128] {
        let mut bytes = [0u8; 128];
        bytes[0..8].copy_from_slice(&crate::parser::EDID_HEADER);
        bytes[0x12..0x14].copy_from_slice(&[1, revision]);
        bytes[0x18] = features;
        let slots = timing_slot.unwrap_or(4);
        for slot in 0..slots {
            let o = 0x36 + slot * 18;
            bytes[o..o + 5].copy_from_slice(&[0, 0, 0, 0xFC, 0]); // monitor name
            bytes[o + 5] = b'\n';
        }
        if let Some(slot) = timing_slot {
            let o = 0x36 + slot * 18;
            bytes[o..o + 18].copy_from_slice(&bordered_dtd(14850, (1920, 280), (1080, 45), 88, 44));
            bytes[o + 15] = 0; // no borders
        }
        let sum = bytes[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        bytes[127] = 0u8.wrapping_sub(sum);
        bytes
    }

    /// The preferred-timing warnings the static pipeline reports for `bytes`.
    fn preferred_timing_warnings(bytes: &[u8; 128]) -> ([Option<EdidWarning>; 2], usize) {
        let parsed = parse_edid(bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(&parsed, &[]);
        let mut found = [None; 2];
        let mut n = 0;
        for w in caps.iter_warnings().filter(|w| {
            matches!(
                w,
                EdidWarning::PreferredTimingNotInFirstSlot { .. }
                    | EdidWarning::PreferredTimingBitClear
            )
        }) {
            if n < 2 {
                found[n] = Some(*w);
            }
            n += 1;
        }
        (found, n)
    }

    #[test]
    fn test_static_pipeline_reports_an_undecodable_preferred_timing() {
        let mut bytes = base_with_timing_in(4, 0, Some(0));
        bytes[0x3B] = 0; // V active 0 (low byte) …
        bytes[0x3D] &= 0x0F; // … and high nibble
        let sum = bytes[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        bytes[127] = 0u8.wrapping_sub(sum);
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(&parsed, &[]);

        assert_eq!(caps.preferred_timing, None);
        let expected = EdidWarning::DtdGeometryInvalid {
            h_active: 1920,
            h_blank: 280,
            v_active: 0,
            v_blank: 45,
        };
        assert_eq!(caps.iter_warnings().filter(|w| **w == expected).count(), 1);
    }

    /// The base block's warnings come in the same order with and without `alloc`, so a
    /// full warning list keeps the same ones in every build.
    #[test]
    fn test_static_pipeline_orders_base_warnings_the_same_in_every_build() {
        // Manufacturer ID all zero (invalid), timing in slot 1, with a 2-pixel border.
        let mut bytes = base_with_timing_in(4, 0, Some(1));
        bytes[0x48 + 15] = 2;
        let sum = bytes[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        bytes[127] = 0u8.wrapping_sub(sum);
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(&parsed, &[]);

        assert_eq!(
            caps.warnings(),
            [
                EdidWarning::InvalidManufacturerId,
                EdidWarning::PreferredTimingNotInFirstSlot {
                    timing_slot: Some(1)
                },
                EdidWarning::DtdBordersAmbiguous {
                    h_border: 2,
                    v_border: 0
                },
            ]
        );
    }

    /// Runs in every build: `capabilities_from_edid` reports the preferred timing too, in
    /// bare `no_std` decoded on its own, with the caller's options.
    #[test]
    fn test_capabilities_from_edid_sets_the_preferred_timing_in_every_build() {
        use crate::model::options::{DtdBorderPolicy, DtdBorderReading};

        let bytes = edid_with_repeated_preferred_dtd();
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        #[cfg(any(feature = "alloc", feature = "std"))]
        let library = ExtensionLibrary::with_standard_handlers();
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        let library = ExtensionLibrary::new();

        let caps = capabilities_from_edid(&parsed, &library);
        let preferred = caps.preferred_timing.as_ref().unwrap();
        assert_eq!((preferred.width, preferred.h_border), (1920, 2));
        assert_eq!(
            preferred.source,
            Some(ModeSource::Dtd { block: 0, index: 0 })
        );
        let from_static = capabilities_from_edid_static::<8, _>(&parsed, &[]).preferred_timing;
        assert_eq!(caps.preferred_timing, from_static);

        let options = DecodeOptions::new()
            .with_dtd_border_policy(DtdBorderPolicy::Fixed(DtdBorderReading::Edid14));
        let caps = capabilities_from_edid_with_options(&parsed, &library, options);
        let reading = caps.preferred_timing.and_then(|m| m.dtd_border_reading);
        assert_eq!(reading, Some(DtdBorderReading::Edid14));
    }

    /// Runs in every build: the shape warning comes after the DTDs' warnings.
    #[test]
    fn test_static_pipeline_reports_an_image_size_of_the_wrong_shape() {
        let mut bytes = base_with_timing_in(4, 0, Some(0));
        bytes[0x36 + 12..0x36 + 15].copy_from_slice(&[0x0F, 0x90, 0x21]); // 527 × 400 mm
        let sum = bytes[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        bytes[127] = 0u8.wrapping_sub(sum);
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(&parsed, &[]);

        assert_eq!(
            caps.warnings(),
            [
                EdidWarning::InvalidManufacturerId,
                EdidWarning::PreferredImageSizeShapeMismatch {
                    width_mm: 527,
                    height_mm: 400,
                    expected_width_mm: 527,
                    expected_height_mm: 296,
                },
            ]
        );
    }

    #[test]
    fn test_warns_when_the_preferred_timing_is_not_in_slot_0() {
        let (found, n) = preferred_timing_warnings(&base_with_timing_in(4, 0, Some(1)));
        assert_eq!(n, 1);
        let expected = EdidWarning::PreferredTimingNotInFirstSlot {
            timing_slot: Some(1),
        };
        assert_eq!(found[0], Some(expected));

        let (found, n) = preferred_timing_warnings(&base_with_timing_in(4, 0, None));
        assert_eq!(n, 1);
        let expected = EdidWarning::PreferredTimingNotInFirstSlot { timing_slot: None };
        assert_eq!(found[0], Some(expected));
    }

    #[test]
    fn test_warns_for_edid_1_3_without_the_preferred_timing_bit() {
        let (found, n) = preferred_timing_warnings(&base_with_timing_in(3, 0, Some(0)));
        assert_eq!(
            (found[0], n),
            (Some(EdidWarning::PreferredTimingBitClear), 1)
        );
        // Only once, not also for the slot: without the bit there is no preferred timing.
        let (_, n) = preferred_timing_warnings(&base_with_timing_in(3, 0, Some(1)));
        assert_eq!(n, 1);
    }

    #[test]
    fn test_no_preferred_timing_warning_where_none_is_due() {
        for (revision, features) in [(4, 0x00), (4, 0x02), (3, 0x02), (2, 0x00)] {
            let (_, n) =
                preferred_timing_warnings(&base_with_timing_in(revision, features, Some(0)));
            assert_eq!(n, 0, "1.{revision} {features:#x}");
        }
    }

    #[cfg(any(feature = "alloc", feature = "std"))]
    #[test]
    fn test_preferred_timing_warnings_reach_display_capabilities() {
        let bytes = base_with_timing_in(4, 0, Some(2));
        let library = ExtensionLibrary::with_standard_handlers();
        let caps = capabilities_from_edid(&parse_edid(&bytes, &library).unwrap(), &library);
        let expected = EdidWarning::PreferredTimingNotInFirstSlot {
            timing_slot: Some(2),
        };
        let found: Vec<_> = caps
            .iter_warnings()
            .filter_map(|w| (**w).downcast_ref::<EdidWarning>().copied())
            .filter(|w| *w == expected)
            .collect();
        assert_eq!(found, [expected]);
        assert_eq!(caps.preferred_timing.map(|m| m.width), Some(1920));
    }

    /// Runs in every build, bare `no_std` included, where the base block's modes go
    /// through `StaticDisplayCapabilities`' own sink rather than `BaseBlockHandler`.
    #[test]
    fn test_static_pipeline_base_dtd_takes_over_a_standard_timing() {
        let mut bytes = [0u8; 128];
        bytes[0..8].copy_from_slice(&crate::parser::EDID_HEADER);
        bytes[0x12] = 1; // EDID 1.x
        bytes[0x26..0x28].copy_from_slice(&[209, 0xC0]); // standard timing 1920×1080 @ 60 Hz
        bytes[0x36..0x48].copy_from_slice(&[
            0x02, 0x3A, 0x80, 0x18, 0x71, 0x38, 0x2D, 0x40, 88, 44, 0x45, 0x00, 0, 0, 0, 0, 0, 0x1E,
        ]); // the same mode as a DTD, 148.5 MHz
        let sum = bytes[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        bytes[127] = 0u8.wrapping_sub(sum);
        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(&parsed, &[]);

        let mut entries = caps
            .iter_modes()
            .filter(|m| m.width == 1920 && m.height == 1080);
        let entry = entries.next().unwrap();
        assert!(entries.next().is_none());
        assert_eq!(entry.pixel_clock_khz, Some(148_500));
        assert_eq!(entry.source, Some(ModeSource::Dtd { block: 0, index: 0 }));
    }

    /// A display descriptor before the base block's first timing does not shift its
    /// index: `Dtd { index }` counts timing descriptors, as edid-decode numbers them.
    #[test]
    fn test_dtd_index_counts_timing_descriptors_only() {
        let mut bytes = [0u8; 128];
        bytes[0..8].copy_from_slice(&crate::parser::EDID_HEADER);
        bytes[0x12] = 1; // EDID 1.x
        bytes[0x36..0x3D].copy_from_slice(&[0, 0, 0, 0xFC, 0, b'P', b'A']); // monitor name
        bytes[0x3D] = b'\n';
        bytes[0x48..0x5A].copy_from_slice(&[
            0x02, 0x3A, 0x80, 0x18, 0x71, 0x38, 0x2D, 0x40, 88, 44, 0x45, 0x00, 0, 0, 0, 0, 0, 0x1E,
        ]); // 1920×1080 @ 60 Hz in slot 1
        let sum = bytes[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        bytes[127] = 0u8.wrapping_sub(sum);
        let first_dtd = Some(ModeSource::Dtd { block: 0, index: 0 });

        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();
        let caps = capabilities_from_edid_static::<8, _>(&parsed, &[]);
        let mut dtds = caps.iter_modes().filter(|m| m.width == 1920);
        assert_eq!(dtds.next().unwrap().source, first_dtd);
        assert!(dtds.next().is_none());

        #[cfg(any(feature = "alloc", feature = "std"))]
        {
            let library = ExtensionLibrary::with_standard_handlers();
            let caps = capabilities_from_edid(&parse_edid(&bytes, &library).unwrap(), &library);
            let sources: Vec<_> = caps
                .supported_modes
                .iter()
                .filter(|m| m.width == 1920)
                .map(|m| m.source)
                .collect();
            assert_eq!(sources, [first_dtd]);
        }
    }

    /// `BaseBlockHandler` sets the preferred timing every time: a base handler registered
    /// after it can correct it (a quirk fix), one registered before it is overwritten.
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[test]
    fn test_a_later_base_handler_can_correct_the_preferred_timing() {
        use crate::model::extension::ExtensionHandler;
        #[derive(Debug)]
        struct Quirk;
        impl ExtensionHandler for Quirk {
            fn process(
                &self,
                _: &[&[u8; 128]],
                caps: &mut DisplayCapabilities,
                _: &mut Vec<ParseWarning>,
            ) {
                caps.preferred_timing = Some(VideoMode::new(640, 480, 60u32, false));
            }
        }
        let bytes = edid_with_repeated_preferred_dtd();
        let width = |library: &ExtensionLibrary| {
            let parsed = parse_edid(&bytes, library).unwrap();
            capabilities_from_edid(&parsed, library)
                .preferred_timing
                .map(|m| m.width)
        };

        let mut after = ExtensionLibrary::with_standard_handlers();
        after.add_base_handler(Quirk);
        assert_eq!(width(&after), Some(640));

        let mut before = ExtensionLibrary::new();
        before.add_base_handler(Quirk);
        before.add_base_handler(base::BaseBlockHandler);
        assert_eq!(width(&before), Some(1920));
    }

    /// PinReady's DMD panel: 1280×800 in the base block, 1280×720 in the CTA block. Only
    /// the base block's timing is the preferred timing, and no extension DTD stands in for
    /// a missing one.
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[test]
    fn test_preferred_timing_comes_from_the_base_block_only() {
        let mut bytes = [0u8; 256];
        bytes[0..8].copy_from_slice(&crate::parser::EDID_HEADER);
        bytes[0x12..0x14].copy_from_slice(&[1, 3]);
        bytes[0x18] = 0x0A; // preferred-timing bit set
        bytes[0x36..0x48].copy_from_slice(&bordered_dtd(7100, (1280, 160), (800, 23), 48, 32));
        bytes[0x36 + 15] = 0;
        bytes[126] = 1;
        bytes[128..132].copy_from_slice(&[0x02, 0x03, 4, 0]); // DTDs from byte 4
        bytes[132..150].copy_from_slice(&bordered_dtd(7425, (1280, 370), (720, 30), 110, 40));
        bytes[132 + 15] = 0;
        let checksum = |bytes: &mut [u8; 256]| {
            for block in bytes.chunks_exact_mut(128) {
                let sum = block[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
                block[127] = 0u8.wrapping_sub(sum);
            }
        };
        checksum(&mut bytes);
        let library = ExtensionLibrary::with_standard_handlers();
        let decode =
            |bytes: &[u8]| capabilities_from_edid(&parse_edid(bytes, &library).unwrap(), &library);

        let preferred = decode(&bytes).preferred_timing.unwrap();
        assert_eq!((preferred.width, preferred.height), (1280, 800));

        bytes[0x36..0x48].fill(0); // no base timing: the CTA DTD does not stand in
        checksum(&mut bytes);
        let caps = decode(&bytes);
        assert_eq!(caps.preferred_timing, None);
        assert!(caps.supported_modes.iter().any(|m| m.height == 720));
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
