//! Reading an EDID from a sink: an I/O-free plan that says which block to read next.
//!
//! piaf does no I/O. [`EdidReadPlan`] holds the structure knowledge a reader needs: which
//! block comes next, at which E-DDC segment and offset, how many blocks the EDID has (byte
//! 126 and block 1's HF-EEODB, the rule [`parse_edid`](crate::parse_edid) sizes the EDID
//! by: [`ExtensionCount`]), and what went wrong along the way. A driver runs it over a bus:
//! it reads the block [`next`](EdidReadPlan::next) names, hands it to
//! [`push`](EdidReadPlan::push), and repeats until `next` returns `None`. The blocks are the
//! driver's; the plan keeps no copy, so it works without `alloc`.
//!
//! Whether to read a block again is the driver's choice: a base block that fails its checks
//! is an error and the plan waits for it, an extension block with a bad checksum is kept
//! and reported ([`ReadWarning::ChecksumMismatch`]). Check one before handing it over with
//! [`block_checksum_valid`]. The plan never gives up on the base block, so a driver bounds
//! its own retries: a sink with a corrupt or blank EDID returns a bad base block every time.
//!
//! How a segment is selected on the bus (the E-DDC segment pointer) is the bus's, for
//! example `hdmi-hal`'s `DdcBus::read_segmented`; the plan only names the segment and offset.
//!
//! The blocks read go to [`parse_edid`](crate::parse_edid) as they are. Both report a smaller
//! HF-EEODB count (`HfEeodbBelowDeclared`), the plan because it explains the blocks
//! [`ReadCount::Larger`] read beyond the EDID.
//!
//! ```
//! use piaf::read::{EdidReadPlan, ReadOptions};
//!
//! // A sink whose EDID is one base block without extensions.
//! fn read_block(segment: u8, offset: u8, block: &mut [u8; 128]) {
//!     assert_eq!((segment, offset), (0, 0));
//!     block[..8].copy_from_slice(&piaf::parser::EDID_HEADER);
//!     block[0x12] = 1; // EDID 1.x
//!     let sum = block[..127].iter().fold(0u8, |a, &b| a.wrapping_add(b));
//!     block[127] = 0u8.wrapping_sub(sum);
//! }
//!
//! let mut edid = [[0u8; 128]; 4];
//! let mut plan = EdidReadPlan::new(ReadOptions::new().with_capacity(4));
//! while let Some(at) = plan.next() {
//!     let block = &mut edid[usize::from(at.block())];
//!     read_block(at.segment(), at.offset(), block);
//!     plan.push(block).expect("a valid base block");
//! }
//! assert_eq!(plan.blocks_read(), 1);
//! assert_eq!(plan.warnings().count(), 0);
//! ```

use crate::model::diagnostics::EdidError;
use crate::parser::{
    ExtensionCount, block_checksum_valid, check_base_block, hf_eeodb_extension_count,
};

/// The most blocks an EDID has: the base block and 255 extensions.
const MAX_BLOCKS: u16 = 256;

/// How many extension blocks the plan reads when byte 126 and block 1's HF-EEODB differ.
///
/// The reference implementations differ here: Linux reads the larger of the two, Amlogic
/// the HF-EEODB count. [`Larger`](Self::Larger) is the chosen default.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadCount {
    /// The larger of byte 126 and the HF-EEODB count, as Linux reads: no block either
    /// declares is missed. [`parse_edid`](crate::parse_edid) parses the EDID's blocks among
    /// them.
    #[default]
    Larger,
    /// The EDID's count, [`ExtensionCount::extensions`]: the HF-EEODB count when present,
    /// as Amlogic reads.
    Edid,
}

impl ExtensionCount {
    /// The extension blocks to read under `rule`.
    pub const fn to_read(&self, rule: ReadCount) -> u8 {
        match (rule, self.hf_eeodb()) {
            (ReadCount::Larger, Some(hf_eeodb)) if hf_eeodb < self.declared() => self.declared(),
            _ => self.extensions(),
        }
    }
}

/// Choices the caller makes about how an EDID is read.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadOptions {
    /// How many extension blocks to read when byte 126 and the HF-EEODB differ.
    pub read_count: ReadCount,
    /// The most blocks the caller holds, the base block included: the plan asks for no
    /// more, and reports the EDID's blocks it did not read
    /// ([`ReadWarning::BlocksNotRead`]). 256, every block an EDID can have, by default; read
    /// as at least 1, and a larger value changes nothing.
    ///
    /// A read cut short before byte 126's blocks is not a complete EDID:
    /// [`parse_edid`](crate::parse_edid) refuses it.
    pub capacity: u16,
}

impl ReadOptions {
    /// The default options: [`ReadCount::Larger`], room for every block.
    pub const fn new() -> Self {
        ReadOptions {
            read_count: ReadCount::Larger,
            capacity: MAX_BLOCKS,
        }
    }

    /// Sets how many extension blocks to read, returning the updated options.
    pub const fn with_read_count(mut self, rule: ReadCount) -> Self {
        self.read_count = rule;
        self
    }

    /// Sets the most blocks the caller holds, the base block included, returning the
    /// updated options.
    pub const fn with_capacity(mut self, blocks: u16) -> Self {
        self.capacity = blocks;
        self
    }
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Where a block is: its number in the EDID and its E-DDC address, two 128-byte blocks per
/// segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockAddress {
    block: u8,
}

impl BlockAddress {
    /// The address of block `block`: 0 for the base block, `n` for extension `n`.
    pub const fn new(block: u8) -> Self {
        BlockAddress { block }
    }

    /// The block's number: 0 for the base block, `n` for extension `n`.
    pub const fn block(self) -> u8 {
        self.block
    }

    /// The E-DDC segment holding the block: `block / 2`.
    pub const fn segment(self) -> u8 {
        self.block / 2
    }

    /// The block's offset within its segment: 0 or 128.
    pub const fn offset(self) -> u8 {
        (self.block % 2) * 128
    }
}

/// Why a block was not taken.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ReadError {
    /// The base block failed the checks [`parse_edid`](crate::parse_edid) makes: its block
    /// count cannot be trusted. The plan still waits for the base block.
    #[error("base block: {0}")]
    BaseBlock(EdidError),
    /// The plan asks for no more blocks.
    #[error("the read is complete; no block was asked for")]
    Complete,
}

/// What a read reports, from the blocks handed to the plan.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReadWarning {
    /// Extension block `block` was handed over with a bad checksum; it is kept.
    /// [`parse_edid`](crate::parse_edid) refuses an EDID with such a block among the ones
    /// it parses.
    #[error("block {block} has a bad checksum")]
    ChecksumMismatch {
        /// The block's number.
        block: u8,
    },
    /// Block 1's HF-EEODB declares fewer extension blocks than byte 126.
    ///
    /// The EDID is the HF-EEODB's count; [`ReadCount::Larger`] reads byte 126's.
    #[error("HF-EEODB declares {hf_eeodb} extension block(s), byte 126 declares {declared}")]
    HfEeodbBelowDeclared {
        /// Byte 126 of the base block.
        declared: u8,
        /// The extension count the HF-EEODB declares.
        hf_eeodb: u8,
    },
    /// The EDID has more blocks than the caller holds ([`ReadOptions::capacity`]);
    /// `edid_blocks - read` of them were not read.
    ///
    /// The EDID's blocks as far as the blocks read tell: byte 126's count when the read
    /// stopped before block 1, [`ExtensionCount::extensions`] after it. Blocks
    /// [`ReadCount::Larger`] would read beyond the EDID are not counted.
    #[error("the EDID has {edid_blocks} block(s), {read} were read; the rest were not")]
    BlocksNotRead {
        /// The EDID's blocks, the base block included.
        edid_blocks: u16,
        /// The blocks read: the caller's capacity.
        read: u16,
    },
}

/// An I/O-free plan for reading an EDID: which block to read next, and what the blocks
/// read so far report. See the [module documentation](self).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdidReadPlan {
    options: ReadOptions,
    /// Blocks taken, from 0.
    read: u16,
    /// Byte 126, once the base block is taken.
    declared: u8,
    /// Block 1's HF-EEODB count, once block 1 is taken with a good checksum.
    hf_eeodb: Option<u8>,
    /// Extension blocks taken with a bad checksum, one bit per block number.
    bad: [u128; 2],
}

impl EdidReadPlan {
    /// A plan that has read nothing yet.
    pub const fn new(options: ReadOptions) -> Self {
        EdidReadPlan {
            options,
            read: 0,
            declared: 0,
            hf_eeodb: None,
            bad: [0; 2],
        }
    }

    /// The block to read next, or `None` when the read is complete.
    pub fn next(&self) -> Option<BlockAddress> {
        (self.read < self.to_read()).then_some(BlockAddress {
            block: self.read as u8,
        })
    }

    /// Takes the block [`next`](Self::next) named.
    ///
    /// The base block passes [`parse_edid`](crate::parse_edid)'s checks or is refused
    /// ([`ReadError::BaseBlock`]), and the plan waits for it again. An extension block is
    /// taken whatever its checksum; a bad one is reported by [`warnings`](Self::warnings),
    /// and block 1's HF-EEODB is not trusted from it (byte 126's count stands).
    ///
    /// # Errors
    ///
    /// [`ReadError::BaseBlock`] for a base block that fails its checks,
    /// [`ReadError::Complete`] when the plan asks for no block.
    pub fn push(&mut self, block: &[u8; 128]) -> Result<(), ReadError> {
        let Some(at) = self.next() else {
            return Err(ReadError::Complete);
        };
        match at.block {
            0 => {
                check_base_block(block).map_err(ReadError::BaseBlock)?;
                self.declared = block[126];
            }
            n => {
                if block_checksum_valid(block) {
                    if n == 1 {
                        self.hf_eeodb = hf_eeodb_extension_count(block);
                    }
                } else {
                    self.bad[usize::from(n / 128)] |= 1 << (n % 128);
                }
            }
        }
        self.read += 1;
        Ok(())
    }

    /// The blocks taken so far, the base block included.
    pub const fn blocks_read(&self) -> u16 {
        self.read
    }

    /// The EDID's extension count, once it is known: after the base block when byte 126
    /// declares no extension, after block 1 otherwise.
    pub const fn extension_count(&self) -> Option<ExtensionCount> {
        if self.read >= 2 || (self.read == 1 && self.declared == 0) {
            Some(ExtensionCount::from_parts(self.declared, self.hf_eeodb))
        } else {
            None
        }
    }

    /// What the blocks taken so far report, in block order: bad checksums, then a smaller
    /// HF-EEODB count, then blocks beyond the caller's capacity.
    pub fn warnings(&self) -> impl Iterator<Item = ReadWarning> + '_ {
        let bad = (1..=u8::MAX)
            .filter(|&n| self.bad[usize::from(n / 128)] & (1 << (n % 128)) != 0)
            .map(|block| ReadWarning::ChecksumMismatch { block });
        let below = self
            .extension_count()
            .and_then(|count| match count.hf_eeodb() {
                Some(hf_eeodb) if hf_eeodb < count.declared() => {
                    Some(ReadWarning::HfEeodbBelowDeclared {
                        declared: count.declared(),
                        hf_eeodb,
                    })
                }
                _ => None,
            });
        let edid_blocks = self.edid_blocks();
        let not_read = (self.next().is_none() && edid_blocks > self.read).then_some(
            ReadWarning::BlocksNotRead {
                edid_blocks,
                read: self.read,
            },
        );
        bad.chain(below).chain(not_read)
    }

    /// The caller's capacity, at least the base block. A capacity above 256 is never
    /// reached: the read wants at most 256 blocks.
    const fn capacity(&self) -> u16 {
        if self.options.capacity == 0 {
            1
        } else {
            self.options.capacity
        }
    }

    /// The EDID's blocks, base block included, as far as the blocks taken tell: byte 126's
    /// count until block 1 is taken, the EDID's count after it.
    fn edid_blocks(&self) -> u16 {
        let extensions = match (self.read, self.extension_count()) {
            (0, _) => 0,
            (_, Some(count)) => count.extensions(),
            (_, None) => self.declared,
        };
        1 + u16::from(extensions)
    }

    /// The blocks the read wants, base block included, as far as the blocks taken tell:
    /// the base block first, then byte 126's count until block 1 is taken, then the
    /// [`ReadCount`] rule's.
    fn wanted(&self) -> u16 {
        let extensions = match (self.read, self.extension_count()) {
            (0, _) => 0,
            (_, Some(count)) => count.to_read(self.options.read_count),
            (_, None) => self.declared,
        };
        1 + u16::from(extensions)
    }

    /// The blocks to read: what the read wants, up to the caller's capacity.
    fn to_read(&self) -> u16 {
        let wanted = self.wanted();
        let capacity = self.capacity();
        if wanted < capacity { wanted } else { capacity }
    }
}

#[cfg(test)]
mod tests;
