use crate::model::capabilities::DisplayCapabilities;
#[cfg(not(any(feature = "alloc", feature = "std")))]
use crate::model::capabilities::ModeSink;
use crate::model::diagnostics::EdidWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::diagnostics::ParseWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::extension::ExtensionHandler;
use crate::model::options::DecodeOptions;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::prelude::{Arc, Vec};

mod descriptors;
mod header;
pub(crate) mod timings;

// Byte-to-type decode primitives shared with the DisplayID sub-decoder.
// DisplayID reuses the EDID encoding for manufacture date (product ID block)
// and color bit depth (display params and display device data blocks).
pub(crate) use header::{decode_color_bit_depth, decode_manufacture_date};

/// Decodes the base block fields that are available in all build configurations,
/// including bare `no_std` without `alloc`.
///
/// Called by [`capabilities_from_edid`][crate::capabilities_from_edid] in `no_std` builds
/// where the handler pipeline is unavailable. In `std`/`alloc` builds the full
/// [`BaseBlockHandler`] is used instead, which additionally decodes variable-length fields
/// and emits diagnostics.
/// Decodes all mode-producing sections of the base block into `sink`.
///
/// Covers established timings, standard timings, detailed timings (sink-based,
/// no `preferred_image_size_mm`), and mode-producing descriptor types.
/// Called by `capabilities_from_edid_static` in bare `no_std` builds.
#[cfg(not(any(feature = "alloc", feature = "std")))]
pub(super) fn decode_base_modes(
    base: &[u8; 128],
    sink: &mut dyn ModeSink,
    options: &DecodeOptions,
) {
    timings::decode_established_timings(base, sink);
    timings::decode_standard_timings(base, sink);
    timings::decode_detailed_timings(base, sink, options);
    descriptors::decode_descriptors_modes(base, sink);
}

#[cfg(not(any(feature = "alloc", feature = "std")))]
pub(super) fn decode_base_block(
    base: &[u8; 128],
    caps: &mut DisplayCapabilities,
    warn: &mut dyn ModeSink,
) {
    if !header::decode_header_fields(base, caps) {
        warn.push_warning(EdidWarning::InvalidManufacturerId);
    }
    descriptors::decode_descriptors_meta(base, caps);
}

/// Decodes the EDID base block into [`DisplayCapabilities`].
///
/// Extracts manufacturer ID, product code, serial number, input type, physical dimensions,
/// monitor name and range limit descriptors, standard timings, and detailed timing descriptors.
#[cfg(any(feature = "alloc", feature = "std"))]
#[derive(Debug)]
pub struct BaseBlockHandler;

#[cfg(any(feature = "alloc", feature = "std"))]
impl ExtensionHandler for BaseBlockHandler {
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
        // Base handlers are always called with a single-element slice containing the
        // base block. Guard against an empty slice from a misconfigured call site.
        let Some(base) = blocks.first() else { return };
        if !header::decode_header_fields(base, caps) {
            warnings.push(Arc::new(EdidWarning::InvalidManufacturerId));
        }
        descriptors::decode_descriptors_meta(base, caps);
        descriptors::decode_descriptors_modes(base, caps);
        timings::decode_established_timings(base, caps);
        timings::decode_standard_timings(base, caps);
        // Use decode_dtd_slot (not decode_detailed_timings) to preserve
        // preferred_image_size_mm population and upgrade-in-place semantics.
        for i in 0..4u8 {
            let offset = 0x36 + (i as usize * 18);
            timings::decode_dtd_slot(&base[offset..offset + 18], caps, i, options);
        }
    }
}
