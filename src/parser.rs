use crate::model::diagnostics::EdidError;
use crate::model::diagnostics::EdidWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::edid::ParsedEdid;
use crate::model::edid::ParsedEdidRef;
use crate::model::extension::KnownExtensions;
use crate::model::warnings::ParseWarnings;

/// The fixed 8-byte magic header at offset 0 of every valid EDID block.
pub const EDID_HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

/// Maximum number of extension blocks the parser processes: 255, the most byte 126 or an
/// HF-EEODB can declare.
///
/// [`parse_edid`] borrows its input and copies no block, so every build takes the full
/// count; it was 64 before piaf 0.7.
pub const MAX_EXTENSION_BLOCKS: usize = 255;

/// The CTA-861 extension tag.
const CTA_EXTENSION_TAG: u8 = 0x02;

/// The HF-EEODB's extended tag code.
const HF_EEODB_EXTENDED_TAG: u8 = 0x78;

/// The extension count an HDMI Forum EDID Extension Override Data Block (HF-EEODB) declares,
/// read from `block1`, the EDID's block 1.
///
/// An HF-EEODB counts only as the first data block of block 1, at bytes 4–6, in a CTA-861
/// extension of revision 3 or later with a data block collection of at least three bytes, as
/// Linux reads it (`drm_edid.c`, `edid_hfeeodb_extension_block_count`); Amlogic reads the
/// same bytes. One found anywhere else declares nothing. A count of 0 declares nothing
/// either: `None`.
pub fn hf_eeodb_extension_count(block1: &[u8; 128]) -> Option<u8> {
    let dtd_offset = block1[2];
    let is_cta_v3 = block1[0] == CTA_EXTENSION_TAG && block1[1] >= 3;
    // The data block collection is bytes 4 to `dtd_offset - 1`.
    let collection_fits = (7..=127).contains(&dtd_offset);
    let header = block1[4];
    let is_extended_block = header >> 5 == 7 && header & 0x1F >= 2;
    if !(is_cta_v3 && collection_fits && is_extended_block) || block1[5] != HF_EEODB_EXTENDED_TAG {
        return None;
    }
    core::num::NonZeroU8::new(block1[6]).map(core::num::NonZeroU8::get)
}

/// Whether `block`'s bytes sum to zero mod 256, as every EDID block's must: its last byte
/// is its checksum.
pub fn block_checksum_valid(block: &[u8; 128]) -> bool {
    block.iter().fold(0u8, |acc, &x| acc.wrapping_add(x)) == 0
}

/// The checks a base block passes before its contents are trusted: the header, the
/// checksum, then the version byte (a corrupt block's version byte cannot be trusted).
pub(crate) fn check_base_block(base: &[u8; 128]) -> Result<(), EdidError> {
    if base[0..8] != EDID_HEADER {
        return Err(EdidError::InvalidHeader);
    }
    if !block_checksum_valid(base) {
        return Err(EdidError::ChecksumMismatch);
    }
    if base[0x12] != 1 {
        return Err(EdidError::UnsupportedVersion {
            version: base[0x12],
        });
    }
    Ok(())
}

/// How many extension blocks an EDID has: byte 126 of its base block, and the HF-EEODB
/// count when block 1 carries one.
///
/// One rule for every reader of an EDID: [`parse_edid`] sizes the EDID with it.
/// [`extensions`](Self::extensions) is the EDID's extension count: the HF-EEODB count when
/// present, as Linux and Amlogic take it, even when it is smaller than byte 126.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExtensionCount {
    declared: u8,
    hf_eeodb: Option<u8>,
}

impl ExtensionCount {
    /// The count for an EDID with base block `base` and, when byte 126 declares at least one
    /// extension, block 1. Without block 1 (not read yet), or with a block 1 whose checksum
    /// is bad, the HF-EEODB count is unknown and [`extensions`](Self::extensions) is byte
    /// 126: Linux reads the HF-EEODB only from a valid block 1.
    pub fn new(base: &[u8; 128], block1: Option<&[u8; 128]>) -> Self {
        let declared = base[126];
        let hf_eeodb = match block1 {
            Some(block1) if declared > 0 && block_checksum_valid(block1) => {
                hf_eeodb_extension_count(block1)
            }
            _ => None,
        };
        ExtensionCount { declared, hf_eeodb }
    }

    /// The count from what has been read: `declared`, byte 126, and `hf_eeodb`, block 1's
    /// HF-EEODB count, already read under [`hf_eeodb_extension_count`]'s rule.
    pub(crate) const fn from_parts(declared: u8, hf_eeodb: Option<u8>) -> Self {
        ExtensionCount { declared, hf_eeodb }
    }

    /// Byte 126 of the base block: the extension count it declares.
    pub const fn declared(&self) -> u8 {
        self.declared
    }

    /// The extension count block 1's HF-EEODB declares, if it carries one; see
    /// [`hf_eeodb_extension_count`].
    pub const fn hf_eeodb(&self) -> Option<u8> {
        self.hf_eeodb
    }

    /// The EDID's extension count: the HF-EEODB count when present, otherwise byte 126.
    pub const fn extensions(&self) -> u8 {
        match self.hf_eeodb {
            Some(count) => count,
            None => self.declared,
        }
    }
}

/// The number of warnings [`parse_edid`] stores in a [`ParsedEdidRef`] without `alloc`;
/// further warnings are counted in
/// [`warnings_dropped`][ParsedEdidRef::warnings_dropped]. With `alloc` every warning is kept.
pub const MAX_PARSE_WARNINGS: usize = 8;

/// Validates a raw EDID byte slice and returns a zero-copy [`ParsedEdidRef`] borrowing
/// directly from `bytes`.
///
/// No block data is copied. The returned value borrows the base block and all extension
/// blocks from `bytes`; it remains valid for as long as `bytes` is alive.
///
/// With `alloc`, to obtain an owned result that outlives the input buffer, call
/// `parse_edid_owned` or convert with `ParsedEdid::from(parsed_ref)`.
///
/// Performs length validation, header verification, and checksum verification for the base
/// block and all parsed extension blocks, and checks that the base block's version byte is
/// 1 (EDID 1.x; anything else is [`EdidError::UnsupportedVersion`]). Extension blocks with tags not known to `tags`
/// generate an [`EdidWarning::UnknownExtension`] warning.
///
/// The EDID's extension blocks are [`ExtensionCount::extensions`]: the HF-EEODB count when
/// block 1 carries one, otherwise byte 126. The input must hold the blocks both counts
/// declare, the smaller of the two ([`EdidError::InvalidLength`] otherwise). When an
/// HF-EEODB declares more than byte 126 and the input stops short of them, as a reader that
/// does not know HF-EEODB returns the EDID, the blocks present are parsed with
/// [`EdidWarning::HfEeodbBlocksMissing`]; one that declares fewer ends the EDID there, with
/// [`EdidWarning::HfEeodbBelowDeclared`], whether the input holds byte 126's further blocks
/// or not.
///
/// Pass an [`ExtensionLibrary`][crate::ExtensionLibrary], an
/// [`ExtensionTagRegistry`][crate::ExtensionTagRegistry], or a
/// `&[&dyn StaticExtensionHandler]` slice such as [`STANDARD_HANDLERS`][crate::STANDARD_HANDLERS]
/// as `tags`; all implement [`KnownExtensions`].
///
/// # Errors
///
/// Returns [`EdidError`] if the byte slice fails any structural check.
pub fn parse_edid<'a, T: KnownExtensions + ?Sized>(
    bytes: &'a [u8],
    tags: &T,
) -> Result<ParsedEdidRef<'a>, EdidError> {
    if bytes.len() < 128 {
        return Err(EdidError::InvalidLength);
    }

    let base_block: &'a [u8; 128] = bytes[0..128].try_into().unwrap();
    check_base_block(base_block)?;

    let declared = usize::from(base_block[126]);
    // Block 1 first: an HF-EEODB in it says how many blocks the EDID has.
    if declared > 0 && bytes.len() < 256 {
        return Err(EdidError::InvalidLength);
    }

    let block1: Option<&[u8; 128]> = (declared > 0).then(|| bytes[128..256].try_into().unwrap());
    let count = ExtensionCount::new(base_block, block1);
    let extensions = usize::from(count.extensions());
    // The blocks both counts declare: an EDID that ends at a smaller HF-EEODB count is
    // complete, as is one that holds only byte 126's blocks of a larger one.
    if bytes.len() < 128 * (1 + declared.min(extensions)) {
        return Err(EdidError::InvalidLength);
    }
    // Whole blocks after the base block; at least the count checked above.
    let present = (bytes.len() - 128) / 128;

    let mut warnings = ParseWarnings::default();

    if let Some(hf_eeodb) = count.hf_eeodb() {
        if hf_eeodb < count.declared() {
            warnings.push(EdidWarning::HfEeodbBelowDeclared {
                declared: count.declared(),
                hf_eeodb,
            });
        }
    }

    // Fewer only when an HF-EEODB declares more blocks than byte 126 and the input holds
    // byte 126's: complete by the base block's count, as a reader that does not know
    // HF-EEODB returns it.
    let blocks_to_parse = extensions.min(present);
    if blocks_to_parse < extensions {
        warnings.push(EdidWarning::HfEeodbBlocksMissing {
            hf_eeodb: count.extensions(),
            present: blocks_to_parse as u8,
        });
    }

    let accounted = 128 * (1 + declared.max(extensions));
    if bytes.len() > accounted {
        warnings.push(EdidWarning::SizeMismatch {
            expected: accounted,
            actual: bytes.len(),
        });
    }

    for i in 0..blocks_to_parse {
        let start = 128 + i * 128;
        let ext_block: &[u8; 128] = bytes[start..start + 128].try_into().unwrap();
        if !block_checksum_valid(ext_block) {
            return Err(EdidError::ChecksumMismatch);
        }
        let tag = ext_block[0];
        if !tags.is_known(tag) {
            warnings.push(EdidWarning::UnknownExtension(tag));
        }
    }

    let raw_extensions = &bytes[128..128 + blocks_to_parse * 128];

    Ok(ParsedEdidRef {
        base_block,
        raw_extensions,
        num_extensions: blocks_to_parse,
        warnings,
    })
}

/// Validates a raw EDID byte slice and returns an owned [`ParsedEdid`], copying the block
/// data out of `bytes` (`alloc`/`std`; without `alloc`, use [`parse_edid`]).
///
/// Equivalent to `ParsedEdid::from(parse_edid(bytes, tags)?)`. Prefer [`parse_edid`] when
/// you can keep the input buffer alive — it avoids copying the block bytes entirely.
///
/// # Errors
///
/// Returns [`EdidError`] if the byte slice fails any structural check.
#[cfg(any(feature = "alloc", feature = "std"))]
pub fn parse_edid_owned<T: KnownExtensions + ?Sized>(
    bytes: &[u8],
    tags: &T,
) -> Result<ParsedEdid, EdidError> {
    Ok(ParsedEdid::from(parse_edid(bytes, tags)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::extension::ExtensionTagRegistry;

    #[test]
    fn test_parse_invalid_length() {
        let bytes = [0u8; 10];
        let registry = ExtensionTagRegistry::new();
        assert_eq!(
            parse_edid(&bytes, &registry).unwrap_err(),
            EdidError::InvalidLength
        );
    }

    #[test]
    fn test_parse_invalid_header() {
        let mut bytes = [0u8; 128];
        bytes[0] = 0x01; // Corrupt header
        let registry = ExtensionTagRegistry::new();
        assert_eq!(
            parse_edid(&bytes, &registry).unwrap_err(),
            EdidError::InvalidHeader
        );
    }

    /// A base block with the header, version byte `version` and a valid checksum.
    fn base_with_version(version: u8) -> [u8; 128] {
        let mut bytes = [0u8; 128];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[0x12] = version;
        let sum = bytes[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        bytes[127] = 0u8.wrapping_sub(sum);
        bytes
    }

    #[test]
    fn test_parse_rejects_a_version_other_than_1() {
        let registry = ExtensionTagRegistry::new();
        for version in [0, 2] {
            assert_eq!(
                parse_edid(&base_with_version(version), &registry).unwrap_err(),
                EdidError::UnsupportedVersion { version }
            );
        }
        assert!(parse_edid(&base_with_version(1), &registry).is_ok());
    }

    #[test]
    fn test_parse_reports_a_bad_checksum_before_the_version() {
        let mut bytes = base_with_version(2);
        bytes[127] = bytes[127].wrapping_add(1);
        let registry = ExtensionTagRegistry::new();
        assert_eq!(
            parse_edid(&bytes, &registry).unwrap_err(),
            EdidError::ChecksumMismatch
        );
    }

    #[test]
    fn test_parse_checksum_mismatch() {
        let mut bytes = [0u8; 128];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[127] = 0x01; // Wrong checksum (should be 6 for all-zeros block with header)
        bytes[0x12] = 1; // EDID 1.x, with the checksum above adjusted
        bytes[127] = bytes[127].wrapping_sub(1);
        let registry = ExtensionTagRegistry::new();
        assert_eq!(
            parse_edid(&bytes, &registry).unwrap_err(),
            EdidError::ChecksumMismatch
        );
    }

    #[test]
    fn test_parse_valid_minimal() {
        let mut bytes = [0u8; 128];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[127] = 6; // Correct checksum for header + zeros
        bytes[0x12] = 1; // EDID 1.x, with the checksum above adjusted
        bytes[127] = bytes[127].wrapping_sub(1);
        let registry = ExtensionTagRegistry::new();
        let result = parse_edid(&bytes, &registry);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.base_block[0..8], EDID_HEADER);
        assert_eq!(parsed.num_extensions, 0);
    }

    #[test]
    #[cfg(any(feature = "alloc", feature = "std"))]
    fn test_parse_with_extensions() {
        let mut bytes = [0u8; 256];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[126] = 1; // 1 extension
        bytes[127] = 5; // Checksum for header + extension_count=1
        bytes[0x12] = 1; // EDID 1.x, with the checksum above adjusted
        bytes[127] = bytes[127].wrapping_sub(1);

        // Extension block
        bytes[128] = 0x02; // Some tag
        bytes[255] = 254; // Checksum: 256 - 2 = 254 (0xFE)

        let registry = ExtensionTagRegistry::new();
        let result = parse_edid(&bytes, &registry);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.num_extensions, 1);
        assert_eq!(parsed.extension_block(0).unwrap()[0], 0x02);
    }

    #[test]
    #[cfg(any(feature = "alloc", feature = "std"))]
    fn test_parse_extension_checksum_mismatch() {
        let mut bytes = [0u8; 256];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[126] = 1;
        bytes[127] = 5;
        bytes[0x12] = 1; // EDID 1.x, with the checksum above adjusted
        bytes[127] = bytes[127].wrapping_sub(1);
        bytes[128] = 0x01;
        bytes[255] = 0x00; // Wrong checksum
        let registry = ExtensionTagRegistry::new();
        assert_eq!(
            parse_edid(&bytes, &registry).unwrap_err(),
            EdidError::ChecksumMismatch
        );
    }

    #[test]
    fn test_parse_unknown_extension_warning() {
        let mut bytes = [0u8; 256];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[126] = 1;
        bytes[127] = 5;
        bytes[0x12] = 1; // EDID 1.x, with the checksum above adjusted
        bytes[127] = bytes[127].wrapping_sub(1);

        // Extension block with tag 0xEE (Unknown)
        bytes[128] = 0xEE;
        bytes[255] = 256u16.wrapping_sub(0xEE) as u8; // Correct checksum for 0xEE

        let registry = ExtensionTagRegistry::new();
        let result = parse_edid(&bytes, &registry);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.warnings(), [EdidWarning::UnknownExtension(0xEE)]);
    }

    #[test]
    fn test_parse_trailing_bytes_warns() {
        // 256-byte buffer but extension_count = 0 → expected 128, actual 256
        let mut bytes = [0u8; 256];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[127] = 6; // Correct checksum for header + zeros (no extensions)
        bytes[0x12] = 1; // EDID 1.x, with the checksum above adjusted
        bytes[127] = bytes[127].wrapping_sub(1);
        let registry = ExtensionTagRegistry::new();
        let result = parse_edid(&bytes, &registry);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(
            parsed.warnings(),
            [EdidWarning::SizeMismatch {
                expected: 128,
                actual: 256
            }]
        );
    }

    #[test]
    #[cfg(any(feature = "alloc", feature = "std"))]
    fn test_parse_known_extension_displayid() {
        let mut bytes = [0u8; 256];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[126] = 1;
        bytes[127] = 5;
        bytes[0x12] = 1; // EDID 1.x, with the checksum above adjusted
        bytes[127] = bytes[127].wrapping_sub(1);

        // Extension block with tag 0x70 (DisplayID)
        bytes[128] = 0x70;
        bytes[255] = 256u16.wrapping_sub(0x70) as u8;

        let registry =
            crate::model::extension::ExtensionLibrary::with_standard_extensions().export_tags();
        let result = parse_edid(&bytes, &registry);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.warnings().is_empty()); // Should be known
        assert_eq!(parsed.num_extensions, 1);
        assert_eq!(parsed.extension_block(0).unwrap()[0], 0x70);
    }

    #[test]
    fn test_custom_extension_registration() {
        let mut bytes = [0u8; 256];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[126] = 1;
        bytes[127] = 5;
        bytes[0x12] = 1; // EDID 1.x, with the checksum above adjusted
        bytes[127] = bytes[127].wrapping_sub(1);

        // Custom tag 0xEE
        let custom_tag = 0xEE;
        bytes[128] = custom_tag;
        bytes[255] = 256u16.wrapping_sub(custom_tag as u16) as u8;

        let mut registry = ExtensionTagRegistry::new();
        registry.register(custom_tag);

        let result = parse_edid(&bytes, &registry);
        assert!(result.is_ok());
        let parsed = result.unwrap();

        // Should NOT have a warning because it was registered
        assert!(parsed.warnings().is_empty());
        assert_eq!(parsed.num_extensions, 1);
        assert_eq!(parsed.extension_block(0).unwrap()[0], custom_tag);
    }

    #[test]
    fn test_parses_the_255_extensions_byte_126_can_declare() {
        let mut bytes = [0u8; 128 * (1 + MAX_EXTENSION_BLOCKS)];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[0x12] = 1; // EDID 1.x
        bytes[126] = 255;
        set_checksum(&mut bytes[..128]);
        // All-zero extension blocks have a valid checksum.

        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();

        assert_eq!(parsed.num_extensions, 255);
        assert!(
            parsed
                .iter_warnings()
                .all(|w| *w == EdidWarning::UnknownExtension(0))
        );
    }

    /// Makes `block`'s last byte its checksum.
    fn set_checksum(block: &mut [u8]) {
        let sum = block[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        block[127] = 0u8.wrapping_sub(sum);
    }

    /// A CTA-861 revision 3 block whose first data block is an HF-EEODB declaring `count`
    /// extensions; no checksum.
    fn cta_with_hf_eeodb(count: u8) -> [u8; 128] {
        let mut block = [0u8; 128];
        block[0] = 0x02; // CTA-861
        block[1] = 3; // revision
        block[2] = 7; // DTDs from byte 7: data blocks at bytes 4–6
        block[4] = (7 << 5) | 2; // extended tag, payload of 2
        block[5] = 0x78; // HF-EEODB
        block[6] = count;
        block
    }

    /// An EDID whose byte 126 is `declared`, whose block 1 carries an HF-EEODB declaring
    /// `hf_eeodb`, and which holds `present` extension blocks (the rest tagged `0x55`); the
    /// input is the first `1 + present` blocks of the array.
    fn edid_with_hf_eeodb(declared: u8, hf_eeodb: u8, present: usize) -> ([u8; 128 * 5], usize) {
        let mut bytes = [0u8; 128 * 5];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[0x12] = 1;
        bytes[126] = declared;
        bytes[128..256].copy_from_slice(&cta_with_hf_eeodb(hf_eeodb));
        for (i, block) in bytes.chunks_exact_mut(128).enumerate().take(1 + present) {
            if i > 1 {
                block[0] = 0x55;
            }
            set_checksum(block);
        }
        (bytes, 128 * (1 + present))
    }

    /// Parses with a registry that knows CTA-861 and `0x55`, so the only warnings are the
    /// block count's.
    fn parse_known(bytes: &[u8]) -> ParsedEdidRef<'_> {
        let mut registry = ExtensionTagRegistry::new();
        registry.register(0x02);
        registry.register(0x55);
        parse_edid(bytes, &registry).unwrap()
    }

    #[test]
    fn test_hf_eeodb_counts_at_bytes_4_to_6_of_a_cta_v3_block() {
        assert_eq!(hf_eeodb_extension_count(&cta_with_hf_eeodb(3)), Some(3));
        assert_eq!(hf_eeodb_extension_count(&cta_with_hf_eeodb(255)), Some(255));
        // A count of 0 declares nothing.
        assert_eq!(hf_eeodb_extension_count(&cta_with_hf_eeodb(0)), None);

        let refused: [(usize, u8); 9] = [
            (0, 0x70),       // not a CTA-861 block
            (1, 2),          // CTA-861 revision 2: no data blocks
            (2, 6),          // a data block collection of two bytes
            (2, 0),          // no data block collection
            (2, 128),        // DTD offset past the block
            (4, 3 << 5 | 2), // a vendor-specific data block, not an extended one
            (4, 7 << 5 | 1), // an extended block without a count byte
            (5, 0x79),       // another extended tag (HF-SCDB)
            (6, 0),          // count 0
        ];
        for (byte, value) in refused {
            let mut block = cta_with_hf_eeodb(3);
            block[byte] = value;
            assert_eq!(
                hf_eeodb_extension_count(&block),
                None,
                "byte {byte} = {value:#04x}"
            );
        }
    }

    #[test]
    fn test_hf_eeodb_is_not_found_after_another_data_block() {
        let mut block = [0u8; 128];
        block[0] = 0x02;
        block[1] = 3;
        block[2] = 9;
        block[4] = (2 << 5) | 1; // a one-byte video data block first
        block[5] = 0x10;
        block[6] = (7 << 5) | 2; // then the HF-EEODB, at bytes 6–8
        block[7] = 0x78;
        block[8] = 3;
        assert_eq!(hf_eeodb_extension_count(&block), None);
    }

    #[test]
    fn test_extension_count_takes_hf_eeodb_when_present() {
        let mut base = base_with_version(1);
        let mut block1 = cta_with_hf_eeodb(3);
        set_checksum(&mut block1);

        base[126] = 1;
        let count = ExtensionCount::new(&base, Some(&block1));
        assert_eq!(
            (count.declared(), count.hf_eeodb(), count.extensions()),
            (1, Some(3), 3)
        );

        // Smaller than byte 126 too.
        base[126] = 5;
        let count = ExtensionCount::new(&base, Some(&block1));
        assert_eq!(
            (count.declared(), count.hf_eeodb(), count.extensions()),
            (5, Some(3), 3)
        );

        // Block 1 not read yet: byte 126.
        assert_eq!(ExtensionCount::new(&base, None).extensions(), 5);

        // No extensions declared: whatever follows is not block 1.
        base[126] = 0;
        let count = ExtensionCount::new(&base, Some(&block1));
        assert_eq!((count.hf_eeodb(), count.extensions()), (None, 0));
    }

    #[test]
    fn test_extension_count_ignores_hf_eeodb_in_a_bad_block_1() {
        let mut base = base_with_version(1);
        base[126] = 1;
        let mut block1 = cta_with_hf_eeodb(3);
        set_checksum(&mut block1);
        block1[127] ^= 1;
        let count = ExtensionCount::new(&base, Some(&block1));
        assert_eq!((count.hf_eeodb(), count.extensions()), (None, 1));
    }

    #[test]
    fn test_parse_edid_parses_the_blocks_hf_eeodb_declares() {
        let (bytes, len) = edid_with_hf_eeodb(1, 3, 3);
        let parsed = parse_known(&bytes[..len]);
        assert_eq!(parsed.num_extensions, 3);
        assert_eq!(parsed.iter_warnings().count(), 0);
    }

    #[test]
    fn test_parse_edid_parses_byte_126s_blocks_when_hf_eeodbs_are_missing() {
        // As a reader that does not know HF-EEODB returns it: byte 126's one extension.
        let (bytes, len) = edid_with_hf_eeodb(1, 3, 1);
        let parsed = parse_known(&bytes[..len]);
        assert_eq!(parsed.num_extensions, 1);
        assert!(
            parsed
                .iter_warnings()
                .eq([&EdidWarning::HfEeodbBlocksMissing {
                    hf_eeodb: 3,
                    present: 1
                }])
        );

        // Part of the HF-EEODB's blocks.
        let (bytes, len) = edid_with_hf_eeodb(1, 3, 2);
        let parsed = parse_known(&bytes[..len]);
        assert_eq!(parsed.num_extensions, 2);
        assert!(
            parsed
                .iter_warnings()
                .eq([&EdidWarning::HfEeodbBlocksMissing {
                    hf_eeodb: 3,
                    present: 2
                }])
        );
    }

    #[test]
    fn test_parse_edid_stops_at_an_hf_eeodb_below_byte_126() {
        let (bytes, len) = edid_with_hf_eeodb(3, 1, 3);
        let parsed = parse_known(&bytes[..len]);
        assert_eq!(parsed.num_extensions, 1);
        // Byte 126's blocks are accounted for: no size mismatch.
        assert!(
            parsed
                .iter_warnings()
                .eq([&EdidWarning::HfEeodbBelowDeclared {
                    declared: 3,
                    hf_eeodb: 1
                }])
        );
    }

    #[test]
    fn test_parse_edid_takes_an_edid_that_ends_at_a_smaller_hf_eeodb_count() {
        // As a reader that reads the HF-EEODB count returns it (Amlogic).
        let (bytes, len) = edid_with_hf_eeodb(3, 1, 1);
        let parsed = parse_known(&bytes[..len]);
        assert_eq!(parsed.num_extensions, 1);
        assert!(
            parsed
                .iter_warnings()
                .eq([&EdidWarning::HfEeodbBelowDeclared {
                    declared: 3,
                    hf_eeodb: 1
                }])
        );

        // One of byte 126's two further blocks.
        let (bytes, len) = edid_with_hf_eeodb(3, 1, 2);
        assert_eq!(parse_known(&bytes[..len]).num_extensions, 1);
    }

    #[test]
    fn test_parse_edid_needs_block_1_when_byte_126_declares_it() {
        let (bytes, _) = edid_with_hf_eeodb(1, 3, 1);
        assert_eq!(
            parse_edid(&bytes[..200], &ExtensionTagRegistry::new()).unwrap_err(),
            EdidError::InvalidLength
        );
    }

    #[test]
    fn test_parse_edid_reports_bytes_beyond_both_counts() {
        // A fourth, all-zero block after the HF-EEODB's two.
        let (bytes, len) = edid_with_hf_eeodb(1, 2, 2);
        let parsed = parse_known(&bytes[..len + 128]);
        assert_eq!(parsed.num_extensions, 2);
        assert!(parsed.iter_warnings().eq([&EdidWarning::SizeMismatch {
            expected: 128 * 3,
            actual: 128 * 4
        }]));
    }

    #[test]
    fn test_parse_edid_still_needs_byte_126s_blocks() {
        let (bytes, len) = edid_with_hf_eeodb(2, 3, 1);
        assert_eq!(
            parse_edid(&bytes[..len], &ExtensionTagRegistry::new()).unwrap_err(),
            EdidError::InvalidLength
        );
    }

    /// Without alloc the parser keeps `MAX_PARSE_WARNINGS` and counts the rest; with alloc
    /// it keeps them all.
    #[test]
    fn test_parse_warnings_beyond_capacity_are_counted() {
        const EXTENSIONS: usize = MAX_PARSE_WARNINGS + 2;
        let mut bytes = [0u8; 128 * (1 + EXTENSIONS)];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[0x12] = 1; // EDID 1.x
        bytes[126] = EXTENSIONS as u8;
        for (i, block) in bytes.chunks_exact_mut(128).enumerate() {
            if i > 0 {
                block[0] = i as u8; // unknown tags 0x01..
            }
            let sum = block[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
            block[127] = 0u8.wrapping_sub(sum);
        }

        let parsed = parse_edid(&bytes, &ExtensionTagRegistry::new()).unwrap();

        #[cfg(any(feature = "alloc", feature = "std"))]
        let (stored, dropped) = (EXTENSIONS, 0);
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        let (stored, dropped) = (MAX_PARSE_WARNINGS, 2);
        assert_eq!(parsed.warnings().len(), stored);
        assert!(
            parsed
                .iter_warnings()
                .zip(1u8..)
                .all(|(w, tag)| *w == EdidWarning::UnknownExtension(tag))
        );
        assert_eq!(parsed.warnings_dropped(), dropped);
    }

    #[test]
    #[cfg(any(feature = "alloc", feature = "std"))]
    fn test_parse_edid_owned() {
        let mut bytes = [0u8; 128];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[127] = 6;
        bytes[0x12] = 1; // EDID 1.x, with the checksum above adjusted
        bytes[127] = bytes[127].wrapping_sub(1);
        let registry = ExtensionTagRegistry::new();
        let owned = parse_edid_owned(&bytes, &registry).unwrap();
        assert_eq!(owned.base_block[0..8], EDID_HEADER);
        assert_eq!(owned.extensions.len(), 0);
    }
}
