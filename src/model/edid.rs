#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::capabilities::DisplayCapabilities;
use crate::model::diagnostics::EdidWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::diagnostics::ParseWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::prelude::Vec;

pub use display_types::EdidVersion;

/// Abstraction over parsed EDID representations.
///
/// Implemented by both [`ParsedEdid`] (owned, copies block bytes) and [`ParsedEdidRef`]
/// (borrowed, zero-copy). Both capability pipeline entry points —
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
    /// processing. The default implementation is a no-op; [`ParsedEdid`] and [`ParsedEdidRef`]
    /// override it in `alloc`/`std` builds to propagate their stored warnings.
    #[cfg(any(feature = "alloc", feature = "std"))]
    fn propagate_parse_warnings(&self, _caps: &mut DisplayCapabilities) {}

    /// Parse-phase warnings as [`EdidWarning`]s, in the order they occurred.
    ///
    /// Read by [`capabilities_from_edid_static`][crate::capabilities_from_edid_static],
    /// which stores them ahead of the base block's and the handlers' warnings. A stored
    /// warning of another type (a `ParseWarning` a caller added to `ParsedEdid::warnings`)
    /// cannot be held by [`StaticDisplayCapabilities`][crate::StaticDisplayCapabilities]
    /// and is not yielded. The default implementation yields nothing; [`ParsedEdid`] and
    /// [`ParsedEdidRef`] override it to yield their stored warnings.
    fn edid_parse_warnings(&self) -> impl Iterator<Item = EdidWarning> + '_ {
        core::iter::empty()
    }
}

/// The [`EdidWarning`]s among `warnings`, skipping warnings of other types.
#[cfg(any(feature = "alloc", feature = "std"))]
fn edid_warnings(warnings: &[ParseWarning]) -> impl Iterator<Item = EdidWarning> + '_ {
    warnings
        .iter()
        .filter_map(|w| (**w).downcast_ref::<EdidWarning>().copied())
}

/// The owned output of [`parse_edid_owned`][crate::parse_edid_owned].
///
/// Copies block bytes out of the input buffer so the result can outlive it. Prefer
/// [`ParsedEdidRef`] from [`parse_edid`][crate::parse_edid] when you can keep the input
/// buffer alive — it avoids copying the block data entirely.
///
/// Pass to [`capabilities_from_edid`][crate::capabilities_from_edid] or
/// [`capabilities_from_edid_static`][crate::capabilities_from_edid_static] to derive a
/// consumer-friendly [`DisplayCapabilities`].
#[derive(Debug, Clone)]
pub struct ParsedEdid {
    /// The raw 128-byte EDID base block, validated and checksum-verified.
    pub base_block: [u8; 128],
    /// Raw 128-byte extension blocks, in the order they appear in the stream.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub extensions: Vec<[u8; 128]>,
    /// Non-fatal conditions encountered during parsing (e.g. unrecognised extension tags).
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub warnings: Vec<ParseWarning>,
}

impl EdidSource for ParsedEdid {
    fn base_block(&self) -> &[u8; 128] {
        &self.base_block
    }

    fn extension_blocks(&self) -> impl Iterator<Item = &[u8; 128]> {
        #[cfg(any(feature = "alloc", feature = "std"))]
        {
            self.extensions.iter()
        }
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        {
            core::iter::empty::<&[u8; 128]>()
        }
    }

    #[cfg(any(feature = "alloc", feature = "std"))]
    fn propagate_parse_warnings(&self, caps: &mut DisplayCapabilities) {
        caps.warnings.extend(self.warnings.iter().cloned());
    }

    fn edid_parse_warnings(&self) -> impl Iterator<Item = EdidWarning> + '_ {
        #[cfg(any(feature = "alloc", feature = "std"))]
        {
            edid_warnings(&self.warnings)
        }
        // Bare `no_std` `ParsedEdid` stores no warnings.
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        {
            core::iter::empty()
        }
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
/// consumer-friendly representation. Convert to [`ParsedEdid`] via `ParsedEdid::from(r)`
/// if you need to store the result after freeing the input buffer.
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
    /// Non-fatal conditions encountered during parsing (`alloc`/`std`).
    /// Use [`iter_warnings`][Self::iter_warnings] for portable access.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub warnings: Vec<ParseWarning>,
    /// Non-fatal conditions encountered during parsing (bare `no_std`).
    /// Use [`iter_warnings`][Self::iter_warnings].
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    pub warnings: [Option<EdidWarning>; 8],
    /// Number of valid entries in `warnings` (bare `no_std`).
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    pub num_warnings: usize,
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

    /// Iterates non-fatal parse warnings (`alloc`/`std`).
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub fn iter_warnings(&self) -> impl Iterator<Item = &ParseWarning> {
        self.warnings.iter()
    }

    /// Iterates non-fatal parse warnings (bare `no_std`).
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    pub fn iter_warnings(&self) -> impl Iterator<Item = &EdidWarning> {
        self.warnings[..self.num_warnings].iter().flatten()
    }
}

impl<'a> EdidSource for ParsedEdidRef<'a> {
    fn base_block(&self) -> &[u8; 128] {
        self.base_block
    }

    fn extension_blocks(&self) -> impl Iterator<Item = &[u8; 128]> {
        (0..self.num_extensions).filter_map(|i| self.extension_block(i))
    }

    #[cfg(any(feature = "alloc", feature = "std"))]
    fn propagate_parse_warnings(&self, caps: &mut DisplayCapabilities) {
        caps.warnings.extend(self.warnings.iter().cloned());
    }

    fn edid_parse_warnings(&self) -> impl Iterator<Item = EdidWarning> + '_ {
        #[cfg(any(feature = "alloc", feature = "std"))]
        {
            edid_warnings(&self.warnings)
        }
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        {
            self.iter_warnings().copied()
        }
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

#[cfg(not(any(feature = "alloc", feature = "std")))]
impl<'a> From<ParsedEdidRef<'a>> for ParsedEdid {
    fn from(r: ParsedEdidRef<'a>) -> Self {
        Self {
            base_block: *r.base_block,
        }
    }
}
