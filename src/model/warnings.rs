//! Private warning storage shared by the parser output and the static capability model.
//!
//! Both types print the same way in every build (`Warnings { items, dropped }`), showing only
//! the warnings in use.

use crate::model::diagnostics::EdidWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::prelude::Vec;
#[cfg(not(any(feature = "alloc", feature = "std")))]
use crate::parser::MAX_PARSE_WARNINGS;

// Fills the unused inline slots; never returned (`as_slice()` stops at `len`).
const PLACEHOLDER_WARNING: EdidWarning = EdidWarning::DescriptorParseFailed;

// Inline storage for up to `N` warnings: the first `len` entries are real, the rest are
// `PLACEHOLDER_WARNING`, so the warnings can be returned as a slice. Warnings beyond `N`
// are counted.
#[derive(Clone)]
pub(crate) struct InlineWarnings<const N: usize> {
    items: [EdidWarning; N],
    len: usize,
    dropped: u16,
}

impl<const N: usize> InlineWarnings<N> {
    pub(crate) const fn new() -> Self {
        Self {
            items: [PLACEHOLDER_WARNING; N],
            len: 0,
            dropped: 0,
        }
    }

    pub(crate) fn as_slice(&self) -> &[EdidWarning] {
        &self.items[..self.len]
    }

    pub(crate) fn push(&mut self, w: EdidWarning) {
        if self.len < N {
            self.items[self.len] = w;
            self.len += 1;
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    pub(crate) fn dropped(&self) -> u16 {
        self.dropped
    }
}

impl<const N: usize> Default for InlineWarnings<N> {
    fn default() -> Self {
        Self::new()
    }
}

// By hand, so that it prints the warnings in use, never the unused slots.
impl<const N: usize> core::fmt::Debug for InlineWarnings<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Warnings")
            .field("items", &self.as_slice())
            .field("dropped", &self.dropped)
            .finish()
    }
}

// The parser's warnings: all of them with alloc; without it, up to `MAX_PARSE_WARNINGS`
// and a count of the rest.
#[derive(Clone, Default)]
pub(crate) struct ParseWarnings {
    #[cfg(any(feature = "alloc", feature = "std"))]
    items: Vec<EdidWarning>,
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    items: InlineWarnings<MAX_PARSE_WARNINGS>,
}

impl ParseWarnings {
    pub(crate) fn as_slice(&self) -> &[EdidWarning] {
        #[cfg(any(feature = "alloc", feature = "std"))]
        return &self.items;
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        self.items.as_slice()
    }

    pub(crate) fn push(&mut self, w: EdidWarning) {
        self.items.push(w);
    }

    pub(crate) fn dropped(&self) -> u16 {
        #[cfg(any(feature = "alloc", feature = "std"))]
        return 0;
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        self.items.dropped()
    }
}

// By hand, so that every build prints the same.
impl core::fmt::Debug for ParseWarnings {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Warnings")
            .field("items", &self.as_slice())
            .field("dropped", &self.dropped())
            .finish()
    }
}
