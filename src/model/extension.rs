#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::capabilities::DisplayCapabilities;
use crate::model::capabilities::StaticContext;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::diagnostics::ParseWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::options::DecodeOptions;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::prelude::{Box, String, Vec};

/// Processes one or more 128-byte EDID blocks and populates [`DisplayCapabilities`].
///
/// Implement this trait to add support for extension block formats or to customise
/// base block parsing. Register handlers via [`ExtensionLibrary`].
///
/// The dispatch layer collects all extension blocks with the handler's registered tag in
/// stream order and calls `process` once with the full slice. Single-block formats (such
/// as CEA-861) receive a slice of length one per block; multi-block formats (such as
/// DisplayID) receive all their fragments together and are responsible for reassembly.
#[cfg(any(feature = "alloc", feature = "std"))]
pub trait ExtensionHandler: core::fmt::Debug {
    /// Inspect `blocks` and update `caps` accordingly.
    ///
    /// `blocks` contains all extension blocks with this handler's registered tag, in the
    /// order they appear in the EDID stream. Push non-fatal issues to `warnings` rather
    /// than panicking or discarding silently.
    fn process(
        &self,
        blocks: &[&[u8; 128]],
        caps: &mut DisplayCapabilities,
        warnings: &mut Vec<ParseWarning>,
    );

    /// [`process`][Self::process] with the caller's [`DecodeOptions`].
    ///
    /// [`capabilities_from_edid_with_options`][crate::capabilities_from_edid_with_options]
    /// calls this, [`capabilities_from_edid`][crate::capabilities_from_edid] with the
    /// default options. The default implementation ignores the options and calls
    /// `process`. A handler that decodes data the options apply to (such as Detailed
    /// Timing Descriptors) overrides it, and its `process` calls it with
    /// [`DecodeOptions::new`].
    fn process_with_options(
        &self,
        blocks: &[&[u8; 128]],
        caps: &mut DisplayCapabilities,
        warnings: &mut Vec<ParseWarning>,
        options: &DecodeOptions,
    ) {
        let _ = options;
        self.process(blocks, caps, warnings);
    }
}

/// Processes one or more 128-byte EDID blocks in a no-alloc context.
///
/// The no-alloc counterpart to [`ExtensionHandler`]. Implement this trait on a unit struct (or
/// any `const`-constructible type) and place an instance in a `static` to register it with
/// `capabilities_from_edid_static`.
///
/// The dispatch layer collects all extension blocks with the handler's tag and calls `process`
/// once with the full slice. Single-block formats (such as CEA-861) receive a slice of length
/// one per block; multi-block formats (such as DisplayID) receive all their fragments together.
///
/// The trait is object-safe, so a slice of static references can be passed directly:
///
/// ```
/// # use piaf::{StaticContext, StaticExtensionHandler};
/// # struct MyHandler;
/// # impl StaticExtensionHandler for MyHandler {
/// #     fn tag(&self) -> u8 { 0xFF }
/// #     fn process(&self, _blocks: &[&[u8; 128]], _ctx: &mut StaticContext<'_>) {}
/// # }
/// static MY_HANDLER: MyHandler = MyHandler;
/// static HANDLERS: &[&dyn piaf::StaticExtensionHandler] = &[&MY_HANDLER];
/// ```
pub trait StaticExtensionHandler: Sync {
    /// The extension block tag byte this handler processes (e.g. `0x02` for CEA-861).
    fn tag(&self) -> u8;
    /// Inspect `blocks` and write decoded modes and warnings into `ctx`.
    ///
    /// `blocks` contains all extension blocks with this handler's tag, in stream order.
    fn process(&self, blocks: &[&[u8; 128]], ctx: &mut StaticContext<'_>);
}

/// Registration entry for an EDID extension block type.
#[cfg(any(feature = "alloc", feature = "std"))]
pub struct ExtensionMetadata {
    /// The tag byte that identifies this extension block format (e.g. `0x02` for CEA-861).
    pub tag: u8,
    /// Human-readable name used in diagnostics.
    pub display_name: String,
    /// Optional handler that processes blocks with this tag.
    /// `None` means the tag is recognised but no processing is performed.
    pub handler: Option<Box<dyn ExtensionHandler>>,
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl core::fmt::Debug for ExtensionMetadata {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ExtensionMetadata")
            .field("tag", &self.tag)
            .field("display_name", &self.display_name)
            .field("has_handler", &self.handler.is_some())
            .finish()
    }
}

/// Minimal registration entry for an EDID extension tag (`no_std` build).
#[cfg(not(any(feature = "alloc", feature = "std")))]
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionMetadata {
    /// The tag byte that identifies this extension block format.
    pub tag: u8,
}

/// Implemented by types that can tell the parser which extension tags are known.
///
/// Implemented for both [`ExtensionTagRegistry`] and [`ExtensionLibrary`], so either
/// can be passed directly to [`parse_edid`][crate::parse_edid].
pub trait KnownExtensions {
    /// Returns `true` if `tag` identifies a known extension block type.
    fn is_known(&self, tag: u8) -> bool;
}

/// A lightweight set of known extension tag bytes.
///
/// Use this when you need to pass a tag list to [`parse_edid`][crate::parse_edid] without
/// setting up a full [`ExtensionLibrary`]. If you are using `ExtensionLibrary`, you can pass it
/// directly to `parse_edid` instead — it implements [`KnownExtensions`] too.
#[cfg(any(feature = "alloc", feature = "std"))]
pub struct ExtensionTagRegistry {
    /// The registered tag bytes.
    pub known_tags: Vec<u8>,
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl Default for ExtensionTagRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl ExtensionTagRegistry {
    /// Creates an empty registry.
    pub const fn new() -> Self {
        Self {
            known_tags: Vec::new(),
        }
    }

    /// Adds `tag` to the set of known tags.
    ///
    /// Returns `true` if the tag was added or was already present, `false` if the registry
    /// is full. In `alloc`/`std` builds the registry is unbounded so this always returns
    /// `true`.
    pub fn register(&mut self, tag: u8) -> bool {
        if !self.known_tags.contains(&tag) {
            self.known_tags.push(tag);
        }
        true
    }

    /// Returns `true` if `tag` has been registered.
    pub fn is_known(&self, tag: u8) -> bool {
        self.known_tags.contains(&tag)
    }
}

/// A fixed-capacity set of known extension tag bytes (`no_std` build, capacity 16).
///
/// # Capacity limit
///
/// This registry can hold at most 16 tags. Attempting to register a 17th distinct tag
/// returns `false` from [`register`][Self::register] and triggers a `debug_assert!` in
/// debug builds. In `alloc`/`std` builds the registry is backed by a `Vec` and has no
/// fixed limit.
#[cfg(not(any(feature = "alloc", feature = "std")))]
pub struct ExtensionTagRegistry {
    tags: [u8; 16],
    len: usize,
}

#[cfg(not(any(feature = "alloc", feature = "std")))]
impl Default for ExtensionTagRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(any(feature = "alloc", feature = "std")))]
impl ExtensionTagRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            tags: [0u8; 16],
            len: 0,
        }
    }

    /// Adds `tag` to the set of known tags.
    ///
    /// Returns `true` if the tag was added, or `false` if it was already present or the
    /// registry is full (capacity 16). Triggers a `debug_assert!` on overflow so that
    /// firmware that registers too many tags fails loudly in debug builds.
    pub fn register(&mut self, tag: u8) -> bool {
        if self.is_known(tag) {
            return true; // already registered — not an error
        }
        debug_assert!(
            self.len < 16,
            "ExtensionTagRegistry is full (capacity 16); tag {tag:#04x} was not registered"
        );
        if self.len < 16 {
            self.tags[self.len] = tag;
            self.len += 1;
            true
        } else {
            false
        }
    }

    /// Returns `true` if `tag` has been registered.
    pub fn is_known(&self, tag: u8) -> bool {
        self.tags[..self.len].contains(&tag)
    }
}

impl KnownExtensions for ExtensionTagRegistry {
    fn is_known(&self, tag: u8) -> bool {
        Self::is_known(self, tag)
    }
}

impl KnownExtensions for [&dyn StaticExtensionHandler] {
    fn is_known(&self, tag: u8) -> bool {
        self.iter().any(|h| h.tag() == tag)
    }
}

/// A registry of extension block handlers and base block handlers.
///
/// Pass to [`parse_edid`][crate::parse_edid] (it implements [`KnownExtensions`]) and to
/// [`capabilities_from_edid`][crate::capabilities_from_edid] to drive capability extraction.
///
/// Use `ExtensionLibrary::with_standard_handlers` (available when the `std` or `alloc` feature
/// is enabled) to get a library pre-loaded with the built-in base block and CEA-861 handlers.
pub struct ExtensionLibrary {
    /// Handlers run against the base block, in registration order.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub base_handlers: Vec<Box<dyn ExtensionHandler>>,
    /// Registered extension block types with optional handlers.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub extensions: Vec<ExtensionMetadata>,
}

impl Default for ExtensionLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensionLibrary {
    /// Creates an empty library with no handlers or registered extensions.
    pub fn new() -> Self {
        Self {
            #[cfg(any(feature = "alloc", feature = "std"))]
            base_handlers: Vec::new(),
            #[cfg(any(feature = "alloc", feature = "std"))]
            extensions: Vec::new(),
        }
    }

    /// Appends a handler that will be run against the base block during capability extraction.
    ///
    /// Handlers are called in registration order. Multiple base handlers may be registered.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub fn add_base_handler<H: ExtensionHandler + 'static>(&mut self, handler: H) {
        self.base_handlers.push(Box::new(handler));
    }

    /// Creates a library with CEA-861 (`0x02`) and DisplayID (`0x70`) registered as known
    /// tags but without any handlers attached. Handlers can be added with [`register`][Self::register].
    ///
    /// Prefer `ExtensionLibrary::with_standard_handlers` unless you need to customise the handlers.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub fn with_standard_extensions() -> Self {
        let mut lib = Self::new();
        lib.register(ExtensionMetadata {
            tag: 0x02,
            display_name: String::from("CEA-861"),
            handler: None, // Will be set by caller or library user
        });
        lib.register(ExtensionMetadata {
            tag: 0x70,
            display_name: String::from("DisplayID"),
            handler: None,
        });
        lib.register(ExtensionMetadata {
            tag: 0xF0,
            display_name: String::from("Block Map"),
            handler: None, // Contents not needed; blocks are discovered sequentially
        });
        lib
    }

    /// Registers an extension block type. Duplicate tags (same `metadata.tag`) are ignored.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub fn register(&mut self, metadata: ExtensionMetadata) {
        if !self.extensions.iter().any(|ext| ext.tag == metadata.tag) {
            self.extensions.push(metadata);
        }
    }

    /// Returns an [`ExtensionTagRegistry`] containing all tags registered in this library.
    ///
    /// Useful when you need a [`KnownExtensions`] value without handing ownership of the
    /// full library to the parser.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub fn export_tags(&self) -> ExtensionTagRegistry {
        let mut known_tags = Vec::new();
        for ext in &self.extensions {
            known_tags.push(ext.tag);
        }
        ExtensionTagRegistry { known_tags }
    }

    /// Returns an empty [`ExtensionTagRegistry`] (`no_std` build — tag export not supported).
    #[cfg(not(any(feature = "alloc", feature = "std")))]
    pub fn export_tags(&self) -> ExtensionTagRegistry {
        ExtensionTagRegistry::new()
    }
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl KnownExtensions for ExtensionLibrary {
    fn is_known(&self, tag: u8) -> bool {
        self.extensions.iter().any(|e| e.tag == tag)
    }
}
