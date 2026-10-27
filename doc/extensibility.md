# Extensibility

## Philosophy

PIAF is a building block, not a complete solution. Different consumers will encounter
different hardware, proprietary extensions, and application-specific requirements that the
library cannot anticipate. Rather than trying to cover every case internally, PIAF provides
the parsing infrastructure and exposes the right seams for consumers to plug into.

The goal is that a consumer should never need to fork the library to get their work done.
They should be able to register custom handlers, store custom data, and receive diagnostics
— all without modifying library code.

This also means PIAF stays small. Features that belong in a specific application or a
higher-level library should not accumulate here.

## Extension points

PIAF is designed so that consumers can add to its behaviour without forking the library.
This document describes the extension points and how to use them.

## Extension handlers

PIAF provides two handler traits that cover different build tiers. Use the one that matches
your allocation constraints.

### `ExtensionHandler` (dynamic, `alloc`/`std`)

`ExtensionHandler` is the primary extension point for std builds. Implement it to process
any EDID block — base or extension — and write results into `DisplayCapabilities`. Handlers
are registered via `ExtensionLibrary` using `Box<dyn ExtensionHandler>`.

```rust
#[derive(Debug)]
struct MyHandler;

impl ExtensionHandler for MyHandler {
    fn process(&self, blocks: &[&[u8; 128]], caps: &mut DisplayCapabilities, warnings: &mut Vec<ParseWarning>) {
        // blocks contains all extension blocks with this handler's tag, in stream order.
        // Read from blocks, write to caps or warnings.
    }
}
```

The pipeline calls each handler through `process_with_options`, which also receives the
caller's `DecodeOptions` (see [Decode options](#decode-options)). Its default implementation
ignores them and calls `process`.

### `StaticExtensionHandler` (static, all tiers)

`StaticExtensionHandler` is the no-alloc counterpart for bare `no_std` builds. It uses
`'static` references in a slice rather than boxed trait objects, and produces mode output
only — rich metadata requires `ExtensionHandler`.

See [`doc/static-pipeline.md`](static-pipeline.md) for the full API reference, custom
handler examples, and sizing notes.

### Registering a base block handler (dynamic pipeline)

Use `add_base_handler` to register one or more handlers for the base block. All registered
handlers are called in order.

```rust
let mut library = ExtensionLibrary::with_standard_handlers();
library.add_base_handler(MyBaseHandler);
```

### Registering an extension block handler (dynamic pipeline)

Extension block handlers are registered per tag via `ExtensionMetadata`.

```rust
library.register(ExtensionMetadata {
    tag: 0xAB,
    display_name: String::from("My Extension"),
    handler: Some(Box::new(MyHandler)),
});
```

Registering an extension with the library is sufficient — passing the library to
`parse_edid` automatically covers tag recognition:

```rust
let parsed = parse_edid(&bytes, &library)?;
let caps = capabilities_from_edid(&parsed, &library);
```

### Replacing a built-in handler (dynamic pipeline)

The handlers registered by `with_standard_handlers()` can be replaced by mutating the
library before use:

```rust
let mut library = ExtensionLibrary::with_standard_handlers();
if let Some(cea) = library.extensions.iter_mut().find(|e| e.tag == 0x02) {
    cea.handler = Some(Box::new(MyCeaHandler));
}
```

## Custom extension data

Handlers can store typed data in `DisplayCapabilities` for consumers to retrieve later.
Any type that is `Debug + Send + Sync` qualifies.

```rust
#[derive(Debug)]
struct MyData { version: u8 }

impl ExtensionHandler for MyHandler {
    fn process(&self, blocks: &[&[u8; 128]], caps: &mut DisplayCapabilities, _warnings: &mut Vec<ParseWarning>) {
        if let Some(block) = blocks.first() {
            caps.set_extension_data(MY_TAG, MyData { version: block[1] });
        }
    }
}
```

Retrieve the data after `capabilities_from_edid`:

```rust
if let Some(data) = caps.get_extension_data::<MyData>(MY_TAG) {
    // data: &MyData
}
```

The stored value is wrapped in `Arc`, so cloning `DisplayCapabilities` shares the data
rather than deep-copying it.

See `examples/inspect_displays.rs` for a working demonstration using CEA-861.

## Emitting warnings

Dynamic handlers receive `&mut Vec<ParseWarning>` and can push any type implementing
`core::error::Error + Send + Sync + 'static`. Built-in library code pushes `EdidWarning`
variants; custom handlers may push their own types.

```rust
warnings.push(Arc::new(EdidWarning::MalformedDataBlock));
```

Static handlers emit warnings via `StaticContext::push_warning`, which accepts `EdidWarning`
directly (no boxing required):

```rust
ctx.push_warning(EdidWarning::MalformedDataBlock);
```

## Decode options

`capabilities_from_edid_with_options` and `capabilities_from_edid_static_with_options` take
the caller's `DecodeOptions`; the functions without the suffix use the defaults. The options
reach every handler, so one setting covers the whole EDID. A handler that decodes data the
options apply to (such as Detailed Timing Descriptors, for `dtd_border_policy`) should apply
them too:

- a dynamic handler overrides `process_with_options` and has `process` call it with
  `DecodeOptions::new()`;
- a static handler reads them from `StaticContext::options()`.

```rust
impl ExtensionHandler for MyHandler {
    fn process(&self, blocks: &[&[u8; 128]], caps: &mut DisplayCapabilities, warnings: &mut Vec<ParseWarning>) {
        self.process_with_options(blocks, caps, warnings, &DecodeOptions::new());
    }

    fn process_with_options(
        &self,
        blocks: &[&[u8; 128]],
        caps: &mut DisplayCapabilities,
        warnings: &mut Vec<ParseWarning>,
        options: &DecodeOptions,
    ) {
        // Decode, applying `options`.
    }
}
```

## `no_std` support

`ExtensionHandler` and `ExtensionLibrary` require `alloc` or `std`. `StaticExtensionHandler`,
`capabilities_from_edid_static`, `capabilities_from_edid_static_with_options` and
`DecodeOptions` are available unconditionally at all build tiers.

See [`doc/static-pipeline.md`](static-pipeline.md) for full details on the no-alloc pipeline.
