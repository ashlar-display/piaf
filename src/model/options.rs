pub use display_types::DtdBorderReading;

/// Choices the caller makes about how ambiguous EDID data is decoded.
///
/// The default decodes as piaf always has: where the data allows more than one reading,
/// piaf takes the one EDIDs follow in practice and warns. A field set here settles that
/// reading instead; piaf applies it, records it where the output has room for it, and
/// does not warn.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DecodeOptions {
    /// How a Detailed Timing Descriptor's blanking is read with respect to its borders.
    pub dtd_border_policy: DtdBorderPolicy,
}

impl DecodeOptions {
    /// The default options: every reading unsettled.
    pub const fn new() -> Self {
        Self {
            dtd_border_policy: DtdBorderPolicy::Unsettled,
        }
    }

    /// Sets the DTD border policy, returning the updated options.
    pub const fn with_dtd_border_policy(mut self, policy: DtdBorderPolicy) -> Self {
        self.dtd_border_policy = policy;
        self
    }
}

/// How a Detailed Timing Descriptor's blanking is read with respect to its borders.
///
/// EDID 1.3 §3.12 counts the borders in the blanking; EDID 1.4 §3.12 does not. EDIDs in
/// practice follow 1.3 whatever version they declare (as edid-decode observes), so the
/// declared version is not evidence of the reading. Only descriptors with non-zero borders
/// are affected.
///
/// To read each EDID by the version it declares, parse it first and set
/// [`Fixed`][Self::Fixed] from its base block (byte 18 is the version, byte 19 the
/// revision):
///
/// ```
/// use piaf::{DecodeOptions, DtdBorderPolicy, DtdBorderReading, EdidSource};
///
/// fn options_by_declared_version(edid: &impl EdidSource) -> DecodeOptions {
///     let base = edid.base_block();
///     let reading = if (base[18], base[19]) >= (1, 4) {
///         DtdBorderReading::Edid14
///     } else {
///         DtdBorderReading::Edid13
///     };
///     DecodeOptions::new().with_dtd_border_policy(DtdBorderPolicy::Fixed(reading))
/// }
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DtdBorderPolicy {
    /// Not settled: piaf reads EDID 1.3, warns with
    /// [`DtdBordersAmbiguous`][crate::EdidWarning::DtdBordersAmbiguous] for a descriptor
    /// with borders, and leaves [`VideoMode::dtd_border_reading`][crate::VideoMode] `None`.
    #[default]
    Unsettled,
    /// Settled by the caller: piaf applies this reading, does not warn, and records it in
    /// [`VideoMode::dtd_border_reading`][crate::VideoMode].
    Fixed(DtdBorderReading),
}
