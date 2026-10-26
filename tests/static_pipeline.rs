use piaf::{
    EdidWarning, ExtensionLibrary, ExtensionTagRegistry, KnownExtensions, MAX_STATIC_WARNINGS,
    ModeSink, RefreshRate, STANDARD_HANDLERS, StaticDisplayCapabilities, capabilities_from_edid,
    capabilities_from_edid_static, parse_edid,
};

fn load(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("Failed to read fixture {path}: {e}"))
}

/// Build a minimal valid base-only EDID (128 bytes).
fn minimal_base_edid() -> [u8; 128] {
    let mut bytes = [0u8; 128];
    bytes[0..8].copy_from_slice(&piaf::parser::EDID_HEADER);
    // Fix checksum
    let sum: u8 = bytes[..127].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    bytes[127] = 0u8.wrapping_sub(sum);
    bytes
}

/// Build a 256-byte EDID with one CEA extension block containing a VDB with the given VICs.
fn edid_with_cea_vdb(vics: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0u8; 256];
    bytes[0..8].copy_from_slice(&piaf::parser::EDID_HEADER);
    bytes[126] = 1; // 1 extension block

    // Base block checksum
    let base_sum: u8 = bytes[..127].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    bytes[127] = 0u8.wrapping_sub(base_sum);

    // CEA extension block at offset 128
    bytes[128] = 0x02; // CEA-861 tag
    bytes[129] = 0x03; // CEA version 3
    bytes[130] = 4 + 1 + vics.len() as u8; // dtd_offset = after data blocks
    bytes[131] = 0x00; // no flags

    // Video Data Block header: tag=2 (bits 7:5), length=vics.len() (bits 4:0)
    bytes[132] = (0x02 << 5) | (vics.len() as u8 & 0x1F);
    for (i, &vic) in vics.iter().enumerate() {
        bytes[133 + i] = vic;
    }

    // Extension block checksum (last byte)
    let ext_sum: u8 = bytes[128..255].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    bytes[255] = 0u8.wrapping_sub(ext_sum);
    bytes
}

// ---------------------------------------------------------------------------
// test_static_base_block_modes
// Established timings (byte 0x23) set → modes appear in StaticDisplayCapabilities
// ---------------------------------------------------------------------------
#[test]
fn test_static_base_block_modes() {
    let mut bytes = minimal_base_edid();
    // Byte 0x23: established timings I — bit 0 = 800×600 @ 60 Hz
    bytes[0x23] = 0b0000_0001;
    // Re-fix checksum
    let sum: u8 = bytes[..127].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    bytes[127] = 0u8.wrapping_sub(sum);

    let registry = ExtensionTagRegistry::new();
    let parsed = parse_edid(&bytes, &registry).unwrap();

    let caps: StaticDisplayCapabilities<64> =
        capabilities_from_edid_static(&parsed, STANDARD_HANDLERS);

    assert!(
        caps.iter_modes().any(|m| m.width == 800
            && m.height == 600
            && m.refresh_rate == Some(RefreshRate::integral(60))),
        "expected 800×600@60 from established timings"
    );
}

// ---------------------------------------------------------------------------
// test_static_cea861_svds
// VIC 16 (1920×1080@60) from a Video Data Block → mode in static caps
// ---------------------------------------------------------------------------
#[test]
fn test_static_cea861_svds() {
    let bytes = edid_with_cea_vdb(&[16]);

    let registry = ExtensionTagRegistry::new();
    let parsed = parse_edid(&bytes, &registry).unwrap();

    let caps: StaticDisplayCapabilities<64> =
        capabilities_from_edid_static(&parsed, STANDARD_HANDLERS);

    assert!(
        caps.iter_modes().any(|m| m.width == 1920
            && m.height == 1080
            && m.refresh_rate == Some(RefreshRate::integral(60))
            && !m.interlaced),
        "expected VIC 16 (1080p60) in static caps"
    );
}

// ---------------------------------------------------------------------------
// test_static_mode_cap_exceeded
// More SVDs than MAX_MODES → num_modes == MAX_MODES, no panic
// ---------------------------------------------------------------------------
#[test]
fn test_static_mode_cap_exceeded() {
    // VICs 1–16: 16 distinct entries, all in the table
    let vics: Vec<u8> = (1..=16).collect();
    let bytes = edid_with_cea_vdb(&vics);

    let registry = ExtensionTagRegistry::new();
    let parsed = parse_edid(&bytes, &registry).unwrap();

    // Constrain to only 4 slots
    let caps: StaticDisplayCapabilities<4> =
        capabilities_from_edid_static(&parsed, STANDARD_HANDLERS);

    assert_eq!(
        caps.num_modes, 4,
        "expected exactly MAX_MODES modes, got {}",
        caps.num_modes
    );
}

// ---------------------------------------------------------------------------
// test_static_warning_cap
// A malformed CEA data block (length overflows the collection) → MalformedDataBlock warning
// ---------------------------------------------------------------------------
#[test]
fn test_static_warning_cap() {
    let mut bytes = vec![0u8; 256];
    bytes[0..8].copy_from_slice(&piaf::parser::EDID_HEADER);
    bytes[126] = 1;
    let base_sum: u8 = bytes[..127].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    bytes[127] = 0u8.wrapping_sub(base_sum);

    // CEA extension block with a data block whose declared length runs past the collection end.
    bytes[128] = 0x02; // CEA tag
    bytes[129] = 0x03;
    // dtd_offset = 4 + 1 (collection bytes only) = 5
    bytes[130] = 5; // collection ends at offset 5 (1 byte: just the header)
    bytes[131] = 0x00;
    // Data block header: tag=2, length=10 — but collection only has 0 more bytes
    bytes[132] = (0x02 << 5) | 10;
    // (No payload bytes follow — length 10 > 0 remaining → malformed)
    let ext_sum: u8 = bytes[128..255].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    bytes[255] = 0u8.wrapping_sub(ext_sum);

    let registry = ExtensionTagRegistry::new();
    let parsed = parse_edid(&bytes, &registry).unwrap();

    let caps: StaticDisplayCapabilities<64> =
        capabilities_from_edid_static(&parsed, STANDARD_HANDLERS);

    assert!(
        caps.iter_warnings()
            .any(|w| *w == EdidWarning::MalformedDataBlock),
        "expected MalformedDataBlock warning in static caps"
    );
}

// ---------------------------------------------------------------------------
// test_static_scalar_fields
// Verifies that base-block scalar fields are correctly populated in the static
// output after the struct-literal → individual-assignment refactor.
// ---------------------------------------------------------------------------
#[test]
#[allow(clippy::identity_op)] // PNP letter-to-bits encoding: (c - 'A' + 1) for symmetry.
fn test_static_scalar_fields() {
    let mut bytes = minimal_base_edid();
    // Manufacturer "SAM" (Samsung):  each letter encodes as (c - 'A' + 1) in 5 bits.
    let ca = (b'S' - b'A' + 1) as u16;
    let cb = (b'A' - b'A' + 1) as u16;
    let cc = (b'M' - b'A' + 1) as u16;
    let packed: u16 = (ca << 10) | (cb << 5) | cc;
    bytes[8] = (packed >> 8) as u8;
    bytes[9] = (packed & 0xFF) as u8;
    // Product code 0x1234 (little-endian)
    bytes[10] = 0x34;
    bytes[11] = 0x12;
    // EDID version 1.4
    bytes[18] = 1;
    bytes[19] = 4;
    // Digital input flag
    bytes[0x14] = 0x80;
    // Re-fix checksum
    let sum: u8 = bytes[..127].iter().fold(0u8, |a, &x| a.wrapping_add(x));
    bytes[127] = 0u8.wrapping_sub(sum);

    let registry = ExtensionTagRegistry::new();
    let parsed = parse_edid(&bytes, &registry).unwrap();
    let caps: StaticDisplayCapabilities<64> =
        capabilities_from_edid_static(&parsed, STANDARD_HANDLERS);

    assert_eq!(
        caps.manufacturer.map(|m| m.to_string()),
        Some("SAM".to_string()),
        "manufacturer should be SAM"
    );
    assert_eq!(
        caps.product_code,
        Some(0x1234),
        "product code should be 0x1234"
    );
    assert_eq!(
        caps.edid_version.map(|v| (v.version, v.revision)),
        Some((1, 4)),
        "EDID version should be 1.4"
    );
    assert!(caps.digital, "digital flag should be set");
}

// ---------------------------------------------------------------------------
// test_static_base_block_warning
// An invalid manufacturer ID in the base block must appear in StaticDisplayCapabilities
// warnings. In alloc builds this comes from the alloc pipeline's handler warnings
// being dropped intentionally, so this test is only meaningful in bare no_std
// builds; the alloc variant below verifies the warning appears via capabilities_from_edid.
// ---------------------------------------------------------------------------
#[test]
fn test_static_base_block_warning_via_alloc_pipeline() {
    // An all-zero manufacturer ID is invalid (not uppercase ASCII A–Z).
    let bytes = minimal_base_edid(); // bytes 8–9 are 0x00, 0x00 → invalid

    let library = ExtensionLibrary::with_standard_handlers();
    let parsed = parse_edid(&bytes, &library).unwrap();
    let alloc_caps = capabilities_from_edid(&parsed, &library);

    assert!(
        alloc_caps
            .iter_warnings()
            .any(|w| (**w).downcast_ref::<EdidWarning>()
                == Some(&EdidWarning::InvalidManufacturerId)),
        "expected InvalidManufacturerId warning in alloc caps"
    );
}

// ---------------------------------------------------------------------------
// test_static_known_extensions
// STANDARD_HANDLERS implements KnownExtensions: CEA-861 (0x02) and DisplayID (0x70) are known
// ---------------------------------------------------------------------------
#[test]
fn test_static_known_extensions() {
    assert!(STANDARD_HANDLERS.is_known(0x02), "CEA-861 should be known");
    assert!(
        STANDARD_HANDLERS.is_known(0x70),
        "DisplayID should be known"
    );
}

// ---------------------------------------------------------------------------
// test_static_same_modes_as_alloc
// LG UltraGear fixture: both pipelines produce identical mode sets
// ---------------------------------------------------------------------------
#[test]
fn test_static_same_modes_as_alloc() {
    let bytes = load("testdata/valid/lg_ultragear_gsm.bin");

    let library = ExtensionLibrary::with_standard_handlers();
    let parsed_alloc = parse_edid(&bytes, &library).unwrap();
    let alloc_caps = capabilities_from_edid(&parsed_alloc, &library);

    let registry = ExtensionTagRegistry::new();
    let parsed_static = parse_edid(&bytes, &registry).unwrap();
    let static_caps: StaticDisplayCapabilities<64> =
        capabilities_from_edid_static(&parsed_static, STANDARD_HANDLERS);

    let mut alloc_modes: Vec<_> = alloc_caps.supported_modes.iter().collect();
    let mut static_modes: Vec<_> = static_caps.iter_modes().collect();

    // Sort both by (width, height, refresh_rate, interlaced) for stable comparison
    alloc_modes.sort_by_key(|m| (m.width, m.height, m.refresh_rate, m.interlaced));
    static_modes.sort_by_key(|m| (m.width, m.height, m.refresh_rate, m.interlaced));

    assert_eq!(
        alloc_modes, static_modes,
        "alloc and static pipelines produced different mode lists"
    );
}

// --- Warning storage ---

#[test]
fn static_warnings_beyond_capacity_are_counted() {
    let mut caps = StaticDisplayCapabilities::<4>::default();
    assert!(caps.warnings().is_empty());
    assert_eq!(caps.warnings_dropped(), 0);

    for tag in 0..(MAX_STATIC_WARNINGS as u8 + 2) {
        caps.push_warning(EdidWarning::UnknownExtension(tag));
    }

    let expected: Vec<EdidWarning> = (0..MAX_STATIC_WARNINGS as u8)
        .map(EdidWarning::UnknownExtension)
        .collect();
    assert_eq!(caps.warnings(), expected.as_slice());
    assert!(caps.iter_warnings().eq(expected.iter()));
    assert_eq!(caps.warnings_dropped(), 2);
}

#[test]
fn static_warnings_dropped_saturates() {
    let mut caps = StaticDisplayCapabilities::<4>::default();
    for _ in 0..(MAX_STATIC_WARNINGS + u16::MAX as usize + 1) {
        caps.push_warning(EdidWarning::DescriptorParseFailed);
    }
    assert_eq!(caps.warnings().len(), MAX_STATIC_WARNINGS);
    assert_eq!(caps.warnings_dropped(), u16::MAX);
}

/// The unused slots hold a placeholder warning; `Debug` must not print it.
#[test]
fn static_warnings_debug_omits_unused_slots() {
    let mut caps = StaticDisplayCapabilities::<4>::default();
    assert!(!format!("{caps:?}").contains("DescriptorParseFailed"));

    caps.push_warning(EdidWarning::UnknownExtension(0x42));
    let debug = format!("{caps:?}");
    assert!(
        debug.contains("items: [UnknownExtension(66)], dropped: 0"),
        "{debug}"
    );
    assert!(!debug.contains("DescriptorParseFailed"), "{debug}");
}
