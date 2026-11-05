use super::*;
use crate::model::diagnostics::EdidWarning;
use crate::model::extension::ExtensionTagRegistry;
use crate::parse_edid;
use crate::parser::EDID_HEADER;

/// Makes `block`'s last byte its checksum.
fn set_checksum(block: &mut [u8; 128]) {
    let sum = block[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
    block[127] = 0u8.wrapping_sub(sum);
}

fn base(declared: u8) -> [u8; 128] {
    let mut block = [0u8; 128];
    block[..8].copy_from_slice(&EDID_HEADER);
    block[0x12] = 1;
    block[126] = declared;
    set_checksum(&mut block);
    block
}

/// A CTA-861 revision 3 block whose first data block is an HF-EEODB declaring `count`.
fn cta_with_hf_eeodb(count: u8) -> [u8; 128] {
    let mut block = [0u8; 128];
    block[..7].copy_from_slice(&[0x02, 3, 7, 0, (7 << 5) | 2, 0x78, count]);
    set_checksum(&mut block);
    block
}

/// An extension block numbered `n` in its second byte, tagged `0x55`.
fn extension(n: u8) -> [u8; 128] {
    let mut block = [0u8; 128];
    block[0] = 0x55;
    block[1] = n;
    set_checksum(&mut block);
    block
}

/// A sink's EDID: `blocks[n]` is block `n`; blocks past the end read as zeros.
struct Sink<'a> {
    blocks: &'a [[u8; 128]],
}

impl Sink<'_> {
    fn read(&self, at: BlockAddress) -> [u8; 128] {
        let n = usize::from(at.segment()) * 2 + usize::from(at.offset() / 128);
        self.blocks.get(n).copied().unwrap_or([0; 128])
    }
}

/// Runs `plan` against `sink` into `buffer`, and returns how many blocks it took.
fn run(plan: &mut EdidReadPlan, sink: &Sink<'_>, buffer: &mut [[u8; 128]]) -> usize {
    while let Some(at) = plan.next() {
        let block = &mut buffer[usize::from(at.block())];
        *block = sink.read(at);
        plan.push(block).unwrap();
    }
    usize::from(plan.blocks_read())
}

fn warnings(plan: &EdidReadPlan) -> ([ReadWarning; 4], usize) {
    let mut out = [ReadWarning::ChecksumMismatch { block: 0 }; 4];
    let mut n = 0;
    for w in plan.warnings() {
        out[n] = w;
        n += 1;
    }
    (out, n)
}

#[test]
fn addresses_are_two_blocks_per_segment() {
    let at = |block| BlockAddress { block };
    assert_eq!((at(0).segment(), at(0).offset()), (0, 0));
    assert_eq!((at(1).segment(), at(1).offset()), (0, 128));
    assert_eq!((at(2).segment(), at(2).offset()), (1, 0));
    assert_eq!((at(255).segment(), at(255).offset()), (127, 128));
}

#[test]
fn a_bad_base_block_is_refused_and_asked_for_again() {
    let mut plan = EdidReadPlan::new(ReadOptions::new());

    let mut corrupt = base(1);
    corrupt[127] ^= 1;
    assert_eq!(
        plan.push(&corrupt),
        Err(ReadError::BaseBlock(EdidError::ChecksumMismatch))
    );
    let mut headless = base(1);
    headless[0] = 0x01;
    assert_eq!(
        plan.push(&headless),
        Err(ReadError::BaseBlock(EdidError::InvalidHeader))
    );
    let mut version_2 = base(1);
    version_2[0x12] = 2;
    set_checksum(&mut version_2);
    assert_eq!(
        plan.push(&version_2),
        Err(ReadError::BaseBlock(EdidError::UnsupportedVersion {
            version: 2
        }))
    );

    assert_eq!(plan.next().map(BlockAddress::block), Some(0));
    assert_eq!(plan.extension_count(), None);
    plan.push(&base(1)).unwrap();
    assert_eq!(plan.next().map(BlockAddress::block), Some(1));
}

#[test]
fn byte_126_alone_sizes_an_edid_without_hf_eeodb() {
    let sink = Sink {
        blocks: &[base(3), extension(1), extension(2), extension(3)],
    };
    let mut plan = EdidReadPlan::new(ReadOptions::new());
    let mut buffer = [[0u8; 128]; 8];
    assert_eq!(run(&mut plan, &sink, &mut buffer), 4);
    assert_eq!(buffer[3], extension(3));
    let count = plan.extension_count().unwrap();
    assert_eq!((count.declared(), count.hf_eeodb()), (3, None));
    assert_eq!(warnings(&plan).1, 0);
    assert_eq!(plan.push(&extension(4)), Err(ReadError::Complete));
}

#[test]
fn hf_eeodb_extends_the_read() {
    let sink = Sink {
        blocks: &[base(1), cta_with_hf_eeodb(3), extension(2), extension(3)],
    };
    for rule in [ReadCount::Larger, ReadCount::Edid] {
        let mut plan = EdidReadPlan::new(ReadOptions::new().with_read_count(rule));
        let mut buffer = [[0u8; 128]; 8];
        assert_eq!(run(&mut plan, &sink, &mut buffer), 4, "{rule:?}");
        assert_eq!(plan.extension_count().unwrap().extensions(), 3);
        assert_eq!(warnings(&plan).1, 0);

        // `parse_edid` takes what the plan read.
        let bytes = buffer.as_flattened();
        let parsed = parse_edid(&bytes[..128 * 4], &ExtensionTagRegistry::new()).unwrap();
        assert_eq!(parsed.num_extensions, 3);
    }
}

#[test]
fn an_hf_eeodb_below_byte_126_reads_by_the_rule() {
    let sink = Sink {
        blocks: &[base(3), cta_with_hf_eeodb(1), extension(2), extension(3)],
    };
    let below = ReadWarning::HfEeodbBelowDeclared {
        declared: 3,
        hf_eeodb: 1,
    };

    let mut plan = EdidReadPlan::new(ReadOptions::new());
    let mut buffer = [[0u8; 128]; 8];
    assert_eq!(run(&mut plan, &sink, &mut buffer), 4);
    assert_eq!(warnings(&plan).0[..warnings(&plan).1], [below]);

    let mut plan = EdidReadPlan::new(ReadOptions::new().with_read_count(ReadCount::Edid));
    assert_eq!(run(&mut plan, &sink, &mut buffer), 2);
    assert_eq!(warnings(&plan).0[..warnings(&plan).1], [below]);
}

#[test]
fn a_bad_extension_block_is_kept_and_reported() {
    let mut bad = extension(2);
    bad[127] ^= 1;
    let sink = Sink {
        blocks: &[base(3), extension(1), bad, extension(3)],
    };
    let mut plan = EdidReadPlan::new(ReadOptions::new());
    let mut buffer = [[0u8; 128]; 8];
    assert_eq!(run(&mut plan, &sink, &mut buffer), 4);
    assert_eq!(buffer[2], bad);
    let (found, n) = warnings(&plan);
    assert_eq!(found[..n], [ReadWarning::ChecksumMismatch { block: 2 }]);
}

#[test]
fn a_bad_block_1_does_not_size_the_read() {
    let mut bad = cta_with_hf_eeodb(3);
    bad[127] ^= 1;
    let sink = Sink {
        blocks: &[base(1), bad, extension(2), extension(3)],
    };
    let mut plan = EdidReadPlan::new(ReadOptions::new());
    let mut buffer = [[0u8; 128]; 8];
    assert_eq!(run(&mut plan, &sink, &mut buffer), 2);
    assert_eq!(plan.extension_count().unwrap().hf_eeodb(), None);
    let (found, n) = warnings(&plan);
    assert_eq!(found[..n], [ReadWarning::ChecksumMismatch { block: 1 }]);
}

#[test]
fn every_block_number_has_its_own_bad_checksum() {
    let mut blocks = [extension(0); 256];
    blocks[0] = base(255);
    for (n, block) in blocks.iter_mut().enumerate().skip(1) {
        *block = extension(n as u8);
        if [1, 127, 128, 255].contains(&n) {
            block[127] ^= 1;
        }
    }
    let sink = Sink { blocks: &blocks };
    let mut plan = EdidReadPlan::new(ReadOptions::new());
    let mut buffer = [[0u8; 128]; 256];
    assert_eq!(run(&mut plan, &sink, &mut buffer), 256);
    assert!(
        plan.warnings()
            .map(|w| match w {
                ReadWarning::ChecksumMismatch { block } => block,
                _ => 0,
            })
            .eq([1, 127, 128, 255])
    );
}

#[test]
fn the_read_stops_at_the_callers_capacity() {
    let sink = Sink {
        blocks: &[base(1), cta_with_hf_eeodb(3), extension(2), extension(3)],
    };
    let mut plan = EdidReadPlan::new(ReadOptions::new().with_capacity(2));
    let mut buffer = [[0u8; 128]; 2];
    assert_eq!(run(&mut plan, &sink, &mut buffer), 2);
    let (found, n) = warnings(&plan);
    assert_eq!(
        found[..n],
        [ReadWarning::BlocksNotRead { wanted: 4, read: 2 }]
    );

    // Room for the base block only: byte 126's count is all the read knows.
    let mut plan = EdidReadPlan::new(ReadOptions::new().with_capacity(0));
    assert_eq!(run(&mut plan, &sink, &mut buffer), 1);
    let (found, n) = warnings(&plan);
    assert_eq!(
        found[..n],
        [ReadWarning::BlocksNotRead { wanted: 2, read: 1 }]
    );
    assert_eq!(plan.extension_count(), None);

    // A capacity above 256 is 256.
    let mut plan = EdidReadPlan::new(ReadOptions::new().with_capacity(1000));
    assert_eq!(run(&mut plan, &sink, &mut [[0u8; 128]; 4]), 4);
}

/// Every rule, with the HF-EEODB above, below and without byte 126: what the plan reads,
/// `parse_edid` takes, as the EDID's blocks.
#[test]
fn parse_edid_takes_every_read() {
    let above = [base(1), cta_with_hf_eeodb(3), extension(2), extension(3)];
    let below = [base(3), cta_with_hf_eeodb(1), extension(2), extension(3)];
    let without = [base(3), extension(1), extension(2), extension(3)];
    for (blocks, edid) in [(&above, 3), (&below, 1), (&without, 3)] {
        for rule in [ReadCount::Larger, ReadCount::Edid] {
            let sink = Sink { blocks };
            let mut plan = EdidReadPlan::new(ReadOptions::new().with_read_count(rule));
            let mut buffer = [[0u8; 128]; 4];
            let read = run(&mut plan, &sink, &mut buffer);
            let bytes = &buffer.as_flattened()[..128 * read];
            let parsed = parse_edid(bytes, &ExtensionTagRegistry::new())
                .unwrap_or_else(|e| panic!("{rule:?}, {read} blocks: {e}"));
            assert_eq!(parsed.num_extensions, edid, "{rule:?}");
        }
    }
}

#[test]
fn an_edid_without_extensions_is_one_block() {
    let mut plan = EdidReadPlan::new(ReadOptions::default());
    assert_eq!(plan.extension_count(), None);
    plan.push(&base(0)).unwrap();
    assert_eq!(plan.next(), None);
    let count = plan.extension_count().unwrap();
    assert_eq!((count.declared(), count.extensions()), (0, 0));
}

#[test]
fn options_default_to_larger_and_room_for_every_block() {
    assert_eq!(ReadOptions::default(), ReadOptions::new());
    assert_eq!(ReadOptions::new().read_count, ReadCount::Larger);
    assert_eq!(ReadOptions::new().capacity, 256);
    assert_eq!(ReadCount::default(), ReadCount::Larger);
}

#[test]
fn errors_and_warnings_describe_themselves() {
    fn text(value: impl core::fmt::Display) -> Text {
        use core::fmt::Write;
        let mut text = Text([0; 160], 0);
        write!(text, "{value}").unwrap();
        text
    }
    let cases: [(Text, &str); 7] = [
        (
            text(EdidWarning::HfEeodbMisplaced { block: Some(2) }),
            "HF-EEODB in block 2, not the first data block of block 1; it declares nothing",
        ),
        (
            text(EdidWarning::HfEeodbMisplaced { block: None }),
            "HF-EEODB in a DisplayID CTA block, not the first data block of block 1; it declares nothing",
        ),
        (
            text(ReadError::Complete),
            "the read is complete; no block was asked for",
        ),
        (
            text(ReadError::BaseBlock(EdidError::InvalidHeader)),
            "base block: EDID header not found at offset 0",
        ),
        (
            text(ReadWarning::ChecksumMismatch { block: 2 }),
            "block 2 has a bad checksum",
        ),
        (
            text(ReadWarning::HfEeodbBelowDeclared {
                declared: 3,
                hf_eeodb: 1,
            }),
            "HF-EEODB declares 1 extension block(s), byte 126 declares 3",
        ),
        (
            text(ReadWarning::BlocksNotRead {
                edid_blocks: 4,
                read: 2,
            }),
            "the EDID has 4 block(s), 2 were read; the rest were not",
        ),
    ];
    for (text, expected) in cases {
        assert_eq!(&text.0[..text.1], expected.as_bytes());
    }
}

/// A fixed-size text buffer, for `Display` without `alloc`.
struct Text([u8; 160], usize);

impl core::fmt::Write for Text {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let end = self.1 + s.len();
        self.0
            .get_mut(self.1..end)
            .ok_or(core::fmt::Error)?
            .copy_from_slice(s.as_bytes());
        self.1 = end;
        Ok(())
    }
}
