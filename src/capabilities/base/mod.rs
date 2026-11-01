use crate::model::capabilities::DisplayCapabilities;
#[cfg(not(any(feature = "alloc", feature = "std")))]
use crate::model::capabilities::ModeSink;
#[cfg(not(any(feature = "alloc", feature = "std")))]
use crate::model::capabilities::VideoMode;
use crate::model::diagnostics::EdidWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::diagnostics::ParseWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::extension::{ExtensionHandler, HandlerContext};
#[cfg(not(any(feature = "alloc", feature = "std")))]
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
/// no `preferred_image_size_mm`), and mode-producing descriptor types. Returns the
/// preferred timing, decoded once with the mode list.
/// Called by `capabilities_from_edid_static` in bare `no_std` builds.
#[cfg(not(any(feature = "alloc", feature = "std")))]
pub(super) fn decode_base_modes(
    base: &[u8; 128],
    sink: &mut dyn ModeSink,
    options: &DecodeOptions,
) -> Option<VideoMode> {
    timings::decode_established_timings(base, sink);
    timings::decode_standard_timings(base, sink);
    let preferred = timings::decode_detailed_timings(base, sink, options);
    descriptors::decode_descriptors_modes(base, sink);
    preferred
}

/// The preferred timing's image-shape warning, if any; see
/// `timings::preferred_image_shape_warning`. Used by the bare `no_std` static pipeline,
/// after the base block's modes.
#[cfg(not(any(feature = "alloc", feature = "std")))]
pub(super) fn preferred_image_shape_warning(
    mode: &VideoMode,
    size: (u16, u16),
) -> Option<EdidWarning> {
    timings::preferred_image_shape_warning(mode, size)
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
    caps.preferred_image_size_mm = timings::preferred_image_size_mm(base);
    timings::preferred_timing_warnings(base, &mut |w| warn.push_warning(w));
}

/// Decodes the EDID base block into [`DisplayCapabilities`].
///
/// Extracts manufacturer ID, product code, serial number, input type, physical dimensions,
/// monitor name and range limit descriptors, standard timings, and detailed timing descriptors.
///
/// Sets `preferred_timing` and `preferred_image_size_mm` every time, replacing earlier
/// values: a base handler that corrects them must be registered after this one.
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
        caps.preferred_image_size_mm = timings::preferred_image_size_mm(base);
        timings::preferred_timing_warnings(base, &mut |w| warnings.push(Arc::new(w)));
        // The preferred timing is the first timing descriptor, decoded once for both the
        // field and the mode list (so its warnings are reported once).
        let has_preferred = timings::has_preferred_timing(base);
        caps.preferred_timing = None;
        // Use decode_dtd_slot (not decode_detailed_timings) to preserve upgrade-in-place
        // semantics.
        let mut index = 0u8;
        for slot in base[0x36..0x7E].chunks_exact(18) {
            let decoded = timings::decode_dtd_slot(slot, caps, Some(0), index, options);
            if timings::is_timing_descriptor(slot) {
                if index == 0 && has_preferred {
                    caps.preferred_timing = decoded;
                }
                index += 1;
            }
        }
        // After the DTDs' warnings, as in bare `no_std` builds.
        if let (Some(mode), Some(size)) = (&caps.preferred_timing, caps.preferred_image_size_mm) {
            if let Some(w) = timings::preferred_image_shape_warning(mode, size) {
                caps.push_warning(w);
            }
        }
    }
}
