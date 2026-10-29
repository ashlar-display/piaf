#[cfg(any(feature = "alloc", feature = "std"))]
pub use display_types::ParseWarning;

/// A non-fatal condition encountered while parsing or processing an EDID block.
///
/// The parser's warnings are this enum directly, in every build: see
/// [`ParsedEdidRef::warnings`][crate::ParsedEdidRef::warnings], and with `alloc`,
/// `ParsedEdid::warnings`. So are the warnings of
/// [`StaticDisplayCapabilities`][crate::StaticDisplayCapabilities]. In
/// `DisplayCapabilities::warnings` (`alloc`/`std`) each entry is a `ParseWarning`; use
/// `downcast_ref` to recover the concrete type.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EdidWarning {
    /// An extension block with an unrecognised tag was encountered.
    /// The inner value is the tag byte.
    #[error("unknown extension block tag: {0:#04x}")]
    UnknownExtension(u8),
    /// A 18-byte descriptor could not be decoded.
    #[error("descriptor could not be parsed")]
    DescriptorParseFailed,
    /// The manufacturer ID bytes do not encode a valid PNP ID.
    ///
    /// Each of the three 5-bit fields must be in the range 1–26 (A–Z). Values of 0 or
    /// 27–31 indicate a corrupted or unprogrammed EEPROM.
    /// [`DisplayCapabilities::manufacturer`][crate::DisplayCapabilities::manufacturer]
    /// is left as `None`.
    #[error("manufacturer ID bytes do not encode a valid PNP ID")]
    InvalidManufacturerId,
    /// A data block inside an extension block declared a length that extends past the
    /// end of the data block collection. Remaining data blocks in the collection are skipped.
    #[error("data block length exceeds collection boundary")]
    MalformedDataBlock,
    /// A Detailed Timing Descriptor slot was skipped because the byte slice passed to the
    /// decoder was shorter than the required 18 bytes.
    ///
    /// This indicates a malformed extension block that claimed to contain a DTD but did
    /// not supply enough data.
    #[error("DTD slot skipped: slice is shorter than the required 18 bytes")]
    DtdSlotTooShort,
    /// A Detailed Timing Descriptor slot was skipped because the pixel clock value
    /// would overflow during refresh rate calculation.
    ///
    /// This indicates a malformed or corrupted EDID: valid pixel clocks are at most
    /// a few hundred MHz (fits comfortably in a `u32` after scaling by 10 000).
    #[error("DTD slot skipped: pixel clock overflow during refresh rate calculation")]
    DtdPixelClockOverflow,
    /// A Detailed Timing Descriptor has non-zero borders, which EDID versions read
    /// differently, and the caller did not settle the reading. The mode is kept.
    ///
    /// EDID 1.3 §3.12 counts the borders in the horizontal and vertical blanking; EDID 1.4
    /// §3.12 does not. EDIDs in practice follow 1.3 (as edid-decode observes), and piaf
    /// reads them that way: the line total is active + blanking, and
    /// [`VideoMode::h_blank`][crate::VideoMode::h_blank] is the blanking minus both
    /// borders. Under the 1.4 reading the blanking would be 2 × `h_border` larger, and the
    /// refresh rate lower, at the same pixel clock. When the 1.3 reading leaves less than
    /// the front porch and sync pulse, `h_blank` is left `None` and
    /// [`DtdBlankingInconsistent`][EdidWarning::DtdBlankingInconsistent] follows.
    ///
    /// Not emitted when the caller settles the reading with
    /// [`DtdBorderPolicy::Fixed`][crate::DtdBorderPolicy::Fixed].
    #[error(
        "DTD has non-zero borders ({h_border} pixels, {v_border} lines); its blanking is read as including them (EDID 1.3)"
    )]
    DtdBordersAmbiguous {
        /// Horizontal border on each side, in pixels (byte 15).
        h_border: u8,
        /// Vertical border on each side, in lines (byte 16).
        v_border: u8,
    },
    /// A Detailed Timing Descriptor's horizontal blanking, as read with respect to its
    /// borders, is smaller than its front porch plus sync pulse: that reading is impossible.
    /// The mode is kept with [`VideoMode::h_blank`][crate::VideoMode::h_blank] left `None`.
    ///
    /// The reading is EDID 1.3 (the blanking less both borders) unless the caller fixed
    /// another with [`DtdBorderPolicy::Fixed`][crate::DtdBorderPolicy::Fixed]; the mode's
    /// [`dtd_border_reading`][crate::VideoMode::dtd_border_reading] records a fixed one.
    /// Without borders, or under EDID 1.4 (`h_blank` = `blanking`), the descriptor is
    /// malformed. Under an unsettled 1.3 reading with borders, the 1.4 reading may still
    /// hold; piaf does not switch to it, because the refresh rate and the mode's identity
    /// depend on the reading too. A caller can fix the 1.4 reading and decode again.
    #[error(
        "DTD blanking {blanking} with borders {h_border} cannot hold front porch {h_front_porch} and sync {h_sync_width} under the reading taken; h_blank left unset"
    )]
    DtdBlankingInconsistent {
        /// Horizontal blanking as stored in the descriptor, in pixels.
        blanking: u16,
        /// Horizontal border on each side, in pixels.
        h_border: u8,
        /// Horizontal front porch, in pixels.
        h_front_porch: u16,
        /// Horizontal sync pulse width, in pixels.
        h_sync_width: u16,
    },
    /// An HDMI Audio Data Block declares MS NonMixed support with a maximum stream count of
    /// one (edid-decode and libdisplay-info both flag this). The flag is kept as declared.
    #[error("HDMI Audio Data Block: MS NonMixed support declared with a single stream")]
    HdmiAudioNonMixedWithoutMultiStream,
    /// An HDMI Audio Data Block claims more 3D audio descriptors (plus the speaker
    /// allocation that follows them, 4 bytes each) than it holds; none are taken.
    #[error(
        "HDMI Audio Data Block claims {claimed} 3D audio descriptors, but holds only {available_bytes} bytes for them"
    )]
    HdmiAudio3dDescriptorsExceedBlock {
        /// The number of 3D audio descriptors claimed (byte 2, bits 2:0).
        claimed: u8,
        /// The bytes left in the block for descriptors.
        available_bytes: u8,
    },
    /// An HDMI 3D Audio Descriptor has a format other than L-PCM or One Bit Audio, the
    /// only two it may carry; the descriptor is skipped.
    #[error("HDMI 3D Audio Descriptor with unsupported format code {code}; skipped")]
    HdmiAudio3dUnsupportedFormat {
        /// The audio format code (byte 0, bits 3:0).
        code: u8,
    },
    /// The number of extension blocks declared in the base block exceeds the parser's
    /// safety limit. Only the first `limit` blocks were parsed; the rest were ignored.
    ///
    /// Real displays have at most a handful of extension blocks. A very large
    /// `extension_count` most likely indicates a malformed or hostile EDID.
    #[error(
        "extension block count {declared} exceeds limit {limit}; only {limit} blocks were parsed"
    )]
    ExtensionBlockLimitReached {
        /// The value of `extension_count` as declared in the base block.
        declared: usize,
        /// The maximum number of extension blocks the parser will process.
        limit: usize,
    },
    /// A DisplayID extension block carries an unrecognised version byte.
    ///
    /// The inner value is the version byte found at offset 1 of the extension block.
    /// The block is skipped; other extension blocks are processed normally.
    #[error("unrecognised DisplayID version: {0:#04x}")]
    DisplayIdVersionUnknown(u8),
    /// The extension count declared in a DisplayID section header does not match the
    /// number of `0x70`-tagged blocks actually present in the EDID stream.
    ///
    /// The DisplayID section is processed with whichever fragments are available.
    #[error(
        "DisplayID extension count mismatch: header declares {declared} continuation block(s), \
         found {found}"
    )]
    DisplayIdExtensionCountMismatch {
        /// Extension count as declared in the DisplayID section header.
        declared: u8,
        /// Actual number of continuation blocks found (i.e. `total 0x70 blocks − 1`).
        found: u8,
    },
    /// The DisplayID section checksum byte does not make the section sum to zero.
    ///
    /// The checksum covers the three-byte section header plus all data block bytes; the byte
    /// immediately following them must bring the running total to zero mod 256. Parsing
    /// continues with whatever data is present in the fragment.
    #[error("DisplayID section checksum is invalid")]
    DisplayIdChecksumMismatch,
    /// The `section_byte_count` field in the DisplayID section header is too large to fit
    /// within the 128-byte extension block.
    ///
    /// A valid DisplayID section holds at most 122 data bytes (bytes 4–125), with the
    /// checksum at byte 126. A larger `section_byte_count` places the checksum outside
    /// the block. The section is still parsed using the clamped available bytes.
    #[error(
        "DisplayID section_byte_count {0} places checksum outside the extension block (max 122)"
    )]
    DisplayIdSectionBytesOutOfRange(u8),
    /// A DisplayID Transfer Characteristics block carries a reserved encoding byte.
    ///
    /// Bits 7:6 of the first payload byte encode the sample bit depth; value `0b11` is
    /// reserved. The inner value is the raw 2-bit field. The block is skipped.
    #[error("DisplayID Transfer Characteristics block has reserved encoding byte: {0:#04x}")]
    UnknownTransferEncoding(u8),
    /// A DisplayID 2.x data block carries a revision byte the spec marks as reserved.
    ///
    /// The block is parsed anyway with the fields defined for revision 0; consumers may
    /// see incorrect values if the revision actually changed the wire format.
    #[error("DisplayID 2.x block tag {tag:#04x} has unsupported revision {revision:#04x}")]
    UnsupportedV2BlockRevision {
        /// Block tag byte (first byte of the 3-byte data block header).
        tag: u8,
        /// Revision byte (second byte of the 3-byte data block header) as found.
        revision: u8,
    },
    /// The byte slice length does not match the size implied by the extension count.
    ///
    /// The EDID header declares `extension_count` extension blocks, so the expected
    /// size is `(1 + extension_count) × 128` bytes. Extra bytes are ignored but may
    /// indicate a driver bug, a KVM device, or a hotplug race.
    #[error("EDID byte length {actual} does not match expected {expected}")]
    SizeMismatch {
        /// The size implied by the extension count: `(1 + extension_count) × 128`.
        expected: usize,
        /// The actual length of the byte slice passed to [`crate::parse_edid`].
        actual: usize,
    },
}

/// A fatal error that prevents useful parsing of an EDID byte stream.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EdidError {
    /// The byte slice is shorter than a complete EDID block (128 bytes per block).
    #[error("input is shorter than a complete EDID block")]
    InvalidLength,
    /// The fixed 8-byte EDID header was not found at offset 0.
    #[error("EDID header not found at offset 0")]
    InvalidHeader,
    /// The checksum byte does not make the block sum to zero.
    #[error("block checksum does not sum to zero")]
    ChecksumMismatch,
}
