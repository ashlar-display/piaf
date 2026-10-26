# Static Extension Handler Pipeline

The static pipeline is a zero-allocation alternative to the dynamic `capabilities_from_edid`
pipeline. It produces `StaticDisplayCapabilities` — a fixed-capacity output type — and works
at all build tiers, including bare `no_std` without an allocator.

## Motivation

The dynamic pipeline relies on `Vec<Box<dyn ExtensionHandler>>` for handler registration and
`Vec<VideoMode>` for output. Neither is available in bare `no_std` builds. Firmware targets with
an RTOS allocator (the `alloc` feature tier) can allocate, but still cannot register handlers
without `Box`.

The static pipeline solves both problems at once.

## Usage

```rust
let parsed = piaf::parse_edid(&bytes, piaf::STANDARD_HANDLERS)?;
let caps: piaf::StaticDisplayCapabilities<64> =
    piaf::capabilities_from_edid_static(&parsed, piaf::STANDARD_HANDLERS);

for mode in caps.iter_modes() {
    // ...
}
```

`STANDARD_HANDLERS` is a `&[&dyn StaticExtensionHandler]` covering CEA-861 and DisplayID. It also implements
`KnownExtensions`, so it can be passed directly to `parse_edid` — no separate
`ExtensionTagRegistry` needed.

## Public API

### `capabilities_from_edid_static`

```rust
pub fn capabilities_from_edid_static<const N: usize, T: EdidSource>(
    parsed: &T,
    handlers: &[&dyn StaticExtensionHandler],
) -> StaticDisplayCapabilities<N>
```

Available unconditionally. Decodes the base block through the same logic as
`capabilities_from_edid`, then dispatches each extension block to the first handler whose
`tag()` matches. In bare `no_std` builds, pass the `ParsedEdidRef` from `parse_edid`: it
borrows the extension blocks, so they are processed at every tier.

The result's warnings are the parser's first, then the base block's, then the handlers'.
`warnings_dropped()` includes the warnings the parser could not store.

`capabilities_from_edid_static` is a **replacement** for `capabilities_from_edid`, not a
complement. Calling both on the same EDID will process extension blocks twice.

### `StaticDisplayCapabilities<const MAX_MODES: usize>`

Contains all scalar fields from `DisplayCapabilities` (manufacturer, timings, chromaticity,
etc.) plus a fixed-capacity array for modes and private, fixed-capacity warning storage:

```rust
pub supported_modes: [Option<VideoMode>; MAX_MODES],
pub num_modes: usize,
```

Accessors:

```rust
pub fn iter_modes(&self) -> impl Iterator<Item = &VideoMode>
pub fn warnings(&self) -> &[EdidWarning]
pub fn iter_warnings(&self) -> impl Iterator<Item = &EdidWarning>
pub fn warnings_dropped(&self) -> u16
```

Modes beyond capacity are silently dropped. 64 is a reasonable default for `MAX_MODES`;
real displays rarely declare more than ~40 modes. Up to `MAX_STATIC_WARNINGS` (8) warnings
are stored; `warnings_dropped()` counts the ones that did not fit (saturating at
`u16::MAX`). The warning API is the same in every build.

### `StaticExtensionHandler`

The no-alloc counterpart to `ExtensionHandler`. Object-safe; `&dyn StaticExtensionHandler`
works in a static slice.

```rust
pub trait StaticExtensionHandler: Sync {
    fn tag(&self) -> u8;
    fn process(&self, blocks: &[&[u8; 128]], ctx: &mut StaticContext<'_>);
}
```

`tag()` makes each handler self-describing. A `&[&dyn StaticExtensionHandler]` slice implements
`KnownExtensions` via the same method, so the same slice serves both as a handler list and
an extension tag filter for `parse_edid`.

### `StaticContext`

The output context passed to `StaticExtensionHandler::process`. Handlers write decoded data
through its methods rather than directly to a sink trait object. This allows new output channels
to be added as `Option` fields in the future without changing the handler trait signature.

```rust
pub struct StaticContext<'a> { /* private */ }

impl StaticContext<'_> {
    pub fn push_mode(&mut self, mode: VideoMode);
    pub fn push_warning(&mut self, w: EdidWarning);
}
```

### `ModeSink`

Internal trait abstracting mode and warning writes. Implemented by `DisplayCapabilities` (alloc),
`StaticDisplayCapabilities<N>` (no-alloc), and `StaticContext` (which forwards to whatever sink
it wraps). Exposed publicly for custom handler implementations that need to share logic across
build tiers.

```rust
pub trait ModeSink {
    fn push_mode(&mut self, mode: VideoMode);
    fn push_warning(&mut self, w: EdidWarning);
}
```

Both `DisplayCapabilities` and `StaticDisplayCapabilities` deduplicate modes by
(width, height, refresh_rate, interlaced): the first entry for a given resolution and rate wins.
A DTD-derived mode with full timing detail therefore takes precedence over a later SVD-derived
mode with sparse timing detail.

### Pre-built statics

```rust
pub static CEA861_HANDLER: &dyn StaticExtensionHandler = &Cea861Handler;
pub static DISPLAYID_HANDLER: &dyn StaticExtensionHandler = &DisplayIdHandler;
pub static STANDARD_HANDLERS: &[&dyn StaticExtensionHandler] = &[&Cea861Handler, &DisplayIdHandler];
```

These are `static`, not `const` — fat pointer coercions to trait objects are not const-stable
in current Rust.

## Custom handlers

Implement `StaticExtensionHandler` for a unit struct, then pass it alongside the pre-built
handlers:

```rust
struct MyHandler;

impl piaf::StaticExtensionHandler for MyHandler {
    fn tag(&self) -> u8 { 0xAB }
    fn process(&self, blocks: &[&[u8; 128]], ctx: &mut piaf::StaticContext<'_>) {
        // blocks contains all extension blocks with this handler's tag, in stream order.
        // Push modes via ctx.push_mode(...) and warnings via ctx.push_warning(...).
    }
}

static MY_HANDLER: MyHandler = MyHandler;
static HANDLERS: &[&dyn piaf::StaticExtensionHandler] = &[
    piaf::CEA861_HANDLER,
    &MY_HANDLER,
];
```

## What the static pipeline extracts from CEA-861

The static CEA-861 path (`Cea861Handler` via `StaticExtensionHandler::process`) extracts
**modes only**:

- Short Video Descriptors (SVDs) from the Video Data Block, including extended VICs (128–255)
- Y420 Video Data Block VIC lookups
- DTDs from the CEA extension block's detailed timing section
- Mode-producing extended blocks: VTB-EXT, T7VTDB, T8VTDB, T10VTDB

It does not extract audio descriptors, VSDB/HF-VSDB fields, colorimetry, HDR metadata, or
speaker allocation — those require the alloc pipeline and are available through
`Cea861Capabilities` via `capabilities_from_edid`.

## Limitations and sizing notes

**Stack size.** At `MAX_MODES = 64`, `StaticDisplayCapabilities` is roughly 3 KB on the stack.
On Cortex-M0 targets with small stacks, place the result in a `static mut` if stack pressure is
a concern.

**`preferred_image_size_mm`.** Populated correctly from the base block. Not populated from CEA
DTDs — that field is a base-block concern and does not belong to the extension pipeline.

**Multi-block formats in bare `no_std`.** In `alloc`/`std` builds, the dispatch layer collects
all fragments for a given tag and calls the handler once with the full slice. In bare `no_std`
builds there is no `Vec`, so each extension block is passed individually as a single-element
slice. Handlers for single-block formats (CEA-861) work correctly at all tiers. Handlers for
multi-block formats (DisplayID) receive one call per fragment and are responsible for handling
that gracefully; full reassembly is not supported at the bare `no_std` tier.
