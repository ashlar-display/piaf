#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::capabilities::DisplayCapabilities;
use crate::model::diagnostics::EdidWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::prelude::Vec;
use crate::model::warnings::ParseWarnings;

pub use display_types::EdidVersion;

/// Abstraction over parsed EDID representations.
///
/// Implemented by [`ParsedEdidRef`] (borrowed, zero-copy) and, with `alloc`, `ParsedEdid`
/// (owned, copies block bytes). Both capability pipeline entry points —
/// [`capabilities_from_edid`][crate::capabilities_from_edid] and
/// [`capabilities_from_edid_static`][crate::capabilities_from_edid_static] — accept any
/// `T: EdidSource`.
///
/// You can implement this trait for your own parse representation if you read EDID data
/// from a custom source (e.g. a proprietary bus) and want to drive the capability pipelines
/// directly.
pub trait EdidSource {
    /// The raw 128-byte base block, validated and checksum-verified.
    fn base_block(&self) -> &[u8; 128];

    /// Iterator over extension blocks in stream order, each as a `&[u8; 128]`.
    fn extension_blocks(&self) -> impl Iterator<Item = &[u8; 128]>;

    /// Copies parse-phase warnings into `caps`.
    ///
    /// Called by [`capabilities_from_edid`][crate::capabilities_from_edid] after handler
    /// processing. The default implementation copies
    /// [`parse_warnings`][Self::parse_warnings].
    #[cfg(any(feature = "alloc", feature = "std"))]
    fn propagate_parse_warnings(&self, caps: &mut DisplayCapabilities) {
        for &w in self.parse_warnings() {
            // The inherent `DisplayCapabilities::push_warning`, which wraps it in an `Arc`.
            caps.push_warning(w);
        }
    }

    /// The parse-phase warnings, in the order they occurred.
    ///
    /// Read by [`capabilities_from_edid_static`][crate::capabilities_from_edid_static],
    /// which stores them ahead of the base block's and the handlers' warnings, and (with
    /// `alloc`) by the default `propagate_parse_warnings`. The default implementation
    /// returns none.
    fn parse_warnings(&self) -> &[EdidWarning] {
        &[]
    }

    /// The number of parse-phase warnings that did not fit in the source's storage
    /// (see [`ParsedEdidRef::warnings_dropped`]).
    ///
    /// [`capabilities_from_edid_static`][crate::capabilities_from_edid_static] adds it to
    /// the result's [`warnings_dropped`][crate::StaticDisplayCapabilities::warnings_dropped].
    /// The default implementation returns 0.
    fn parse_warnings_dropped(&self) -> u16 {
        0
    }
}

/// The owned output of [`parse_edid_owned`][crate::parse_edid_owned] (`alloc`/`std`).
///
/// Copies block bytes out of the input buffer so the result can outlive it. Prefer
/// [`ParsedEdidRef`] from [`parse_edid`][crate::parse_edid] when you can keep the input
/// buffer alive — it avoids copying the block data entirely. Without `alloc` there is no
/// owned form: keep the input buffer and use [`ParsedEdidRef`].
///
/// Pass to [`capabilities_from_edid`][crate::capabilities_from_edid] or
/// [`capabilities_from_edid_static`][crate::capabilities_from_edid_static] to derive a
/// consumer-friendly [`DisplayCapabilities`].
#[cfg(any(feature = "alloc", feature = "std"))]
#[derive(Debug, Clone)]
pub struct ParsedEdid {
    /// The raw 128-byte EDID base block, validated and checksum-verified.
    pub base_block: [u8; 128],
    /// Raw 128-byte extension blocks, in the order they appear in the stream.
    pub extensions: Vec<[u8; 128]>,
    // Non-fatal conditions encountered during parsing.
    warnings: ParseWarnings,
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl ParsedEdid {
    /// The non-fatal conditions encountered during parsing, in the order they occurred.
    pub fn warnings(&self) -> &[EdidWarning] {
        self.warnings.as_slice()
    }

    /// Iterates the non-fatal conditions encountered during parsing, in the order they
    /// occurred.
    pub fn iter_warnings(&self) -> impl Iterator<Item = &EdidWarning> {
        self.warnings().iter()
    }

    /// The number of warnings that did not fit; always 0, as `ParsedEdid` keeps every
    /// warning. Here for parity with [`ParsedEdidRef::warnings_dropped`].
    pub fn warnings_dropped(&self) -> u16 {
        self.warnings.dropped()
    }
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl EdidSource for ParsedEdid {
    fn base_block(&self) -> &[u8; 128] {
        &self.base_block
    }

    fn extension_blocks(&self) -> impl Iterator<Item = &[u8; 128]> {
        self.extensions.iter()
    }

    fn parse_warnings(&self) -> &[EdidWarning] {
        self.warnings()
    }

    fn parse_warnings_dropped(&self) -> u16 {
        self.warnings_dropped()
    }
}

/// The zero-copy output of [`parse_edid`][crate::parse_edid].
///
/// Borrows the base block and extension blocks directly from the input byte slice — no
/// block data is copied during parsing. Parse-phase warnings (unknown extension tags,
/// size mismatches, etc.) are owned by this struct since they are generated data, not
/// borrowed data.
///
/// Pass to [`capabilities_from_edid`][crate::capabilities_from_edid] or
/// [`capabilities_from_edid_static`][crate::capabilities_from_edid_static] to derive a
/// consumer-friendly representation. With `alloc`, convert to `ParsedEdid` via
/// `ParsedEdid::from(r)` if you need to store the result after freeing the input buffer.
#[derive(Debug)]
pub struct ParsedEdidRef<'a> {
    /// The raw 128-byte EDID base block, validated and checksum-verified.
    /// Borrowed from the input byte slice.
    pub base_block: &'a [u8; 128],
    /// Raw extension block bytes, borrowed from the input. Length is `num_extensions * 128`.
    /// Access via [`extension_block`][Self::extension_block].
    pub(crate) raw_extensions: &'a [u8],
    /// Number of validated extension blocks.
    pub num_extensions: usize,
    // Non-fatal conditions encountered during parsing.
    pub(crate) warnings: ParseWarnings,
}

impl<'a> ParsedEdidRef<'a> {
    /// Returns a reference to extension block `i`, or `None` if `i >= num_extensions`.
    ///
    /// The returned reference has lifetime `'a`, borrowing from the original input buffer
    /// rather than from `self`.
    pub fn extension_block(&self, i: usize) -> Option<&'a [u8; 128]> {
        let start = i.checked_mul(128)?;
        self.raw_extensions.get(start..start + 128)?.try_into().ok()
    }

    /// The non-fatal conditions encountered during parsing, in the order they occurred.
    pub fn warnings(&self) -> &[EdidWarning] {
        self.warnings.as_slice()
    }

    /// Iterates the non-fatal conditions encountered during parsing, in the order they
    /// occurred.
    pub fn iter_warnings(&self) -> impl Iterator<Item = &EdidWarning> {
        self.warnings().iter()
    }

    /// The number of warnings that did not fit (saturating at `u16::MAX`): without `alloc`
    /// the parser stores up to [`MAX_PARSE_WARNINGS`][crate::parser::MAX_PARSE_WARNINGS].
    /// Always 0 with `alloc`.
    pub fn warnings_dropped(&self) -> u16 {
        self.warnings.dropped()
    }
}

impl<'a> EdidSource for ParsedEdidRef<'a> {
    fn base_block(&self) -> &[u8; 128] {
        self.base_block
    }

    fn extension_blocks(&self) -> impl Iterator<Item = &[u8; 128]> {
        (0..self.num_extensions).filter_map(|i| self.extension_block(i))
    }

    fn parse_warnings(&self) -> &[EdidWarning] {
        self.warnings()
    }

    fn parse_warnings_dropped(&self) -> u16 {
        self.warnings_dropped()
    }
}

/// Converts a [`ParsedEdidRef`] into an owned [`ParsedEdid`] by copying the block data.
///
/// Useful when you need the parse result to outlive the input buffer.
#[cfg(any(feature = "alloc", feature = "std"))]
impl<'a> From<ParsedEdidRef<'a>> for ParsedEdid {
    fn from(r: ParsedEdidRef<'a>) -> Self {
        Self {
            base_block: *r.base_block,
            extensions: (0..r.num_extensions)
                .filter_map(|i| r.extension_block(i))
                .copied()
                .collect(),
            warnings: r.warnings,
        }
    }
}
