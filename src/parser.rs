use crate::model::diagnostics::EdidError;
use crate::model::diagnostics::EdidWarning;
#[cfg(any(feature = "alloc", feature = "std"))]
use crate::model::edid::ParsedEdid;
use crate::model::edid::ParsedEdidRef;
use crate::model::extension::KnownExtensions;
use crate::model::warnings::ParseWarnings;

/// The fixed 8-byte magic header at offset 0 of every valid EDID block.
pub const EDID_HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

/// Maximum number of extension blocks the parser will store and process.
///
/// The EDID spec allows up to 255 extension blocks, but real displays have at most a handful.
/// Inputs claiming more than this limit are most likely malformed or hostile; the parser
/// emits [`EdidWarning::ExtensionBlockLimitReached`] and ignores the excess blocks.
pub const MAX_EXTENSION_BLOCKS: usize = 64;

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
/// block and all declared extension blocks. Extension blocks with tags not known to `tags`
/// generate an [`EdidWarning::UnknownExtension`] warning.
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

    if base_block[0..8] != EDID_HEADER {
        return Err(EdidError::InvalidHeader);
    }

    let checksum: u8 = base_block.iter().fold(0u8, |acc, &x| acc.wrapping_add(x));
    if checksum != 0 {
        return Err(EdidError::ChecksumMismatch);
    }

    let extension_count = base_block[126] as usize;
    let total_required = 128 * (1 + extension_count);

    if bytes.len() < total_required {
        return Err(EdidError::InvalidLength);
    }

    let mut warnings = ParseWarnings::default();

    if bytes.len() > total_required {
        warnings.push(EdidWarning::SizeMismatch {
            expected: total_required,
            actual: bytes.len(),
        });
    }

    let blocks_to_parse = if extension_count > MAX_EXTENSION_BLOCKS {
        warnings.push(EdidWarning::ExtensionBlockLimitReached {
            declared: extension_count,
            limit: MAX_EXTENSION_BLOCKS,
        });
        MAX_EXTENSION_BLOCKS
    } else {
        extension_count
    };

    for i in 0..blocks_to_parse {
        let start = 128 + i * 128;
        let ext_block: &[u8; 128] = bytes[start..start + 128].try_into().unwrap();
        let ext_sum: u8 = ext_block.iter().fold(0u8, |acc, &x| acc.wrapping_add(x));
        if ext_sum != 0 {
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

    #[test]
    fn test_parse_checksum_mismatch() {
        let mut bytes = [0u8; 128];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[127] = 0x01; // Wrong checksum (should be 6 for all-zeros block with header)
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
    fn test_extension_block_limit() {
        // Build an EDID that declares MAX_EXTENSION_BLOCKS + 1 extension blocks.
        let declared = MAX_EXTENSION_BLOCKS + 1;
        let mut bytes = [0u8; 128 * (2 + MAX_EXTENSION_BLOCKS)];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
        bytes[126] = declared as u8;

        // Fix up base block checksum.
        let base_sum: u8 = bytes[0..127]
            .iter()
            .fold(0u8, |acc, &x| acc.wrapping_add(x));
        bytes[127] = 0u8.wrapping_sub(base_sum);

        // Each extension block needs a valid checksum (all-zero payload → checksum 0).
        // They are already zero-initialised, so no further fixup needed.

        let registry = ExtensionTagRegistry::new();
        let parsed = parse_edid(&bytes, &registry).unwrap();

        assert_eq!(parsed.num_extensions, MAX_EXTENSION_BLOCKS);
        assert!(parsed.iter_warnings().any(|w| {
            *w == EdidWarning::ExtensionBlockLimitReached {
                declared,
                limit: MAX_EXTENSION_BLOCKS,
            }
        }));
    }

    /// Without alloc the parser keeps `MAX_PARSE_WARNINGS` and counts the rest; with alloc
    /// it keeps them all.
    #[test]
    fn test_parse_warnings_beyond_capacity_are_counted() {
        const EXTENSIONS: usize = MAX_PARSE_WARNINGS + 2;
        let mut bytes = [0u8; 128 * (1 + EXTENSIONS)];
        bytes[0..8].copy_from_slice(&EDID_HEADER);
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
        let registry = ExtensionTagRegistry::new();
        let owned = parse_edid_owned(&bytes, &registry).unwrap();
        assert_eq!(owned.base_block[0..8], EDID_HEADER);
        assert_eq!(owned.extensions.len(), 0);
    }
}
