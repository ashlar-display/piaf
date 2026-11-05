# Roadmap

## Planned

### Reading an EDID: a read plan and one block count

piaf parses bytes; reading them from a sink is the caller's. To help without taking on I/O,
piaf gets an **I/O-free read plan**: given the blocks read so far, it says which segment and
offset to read next (E-DDC: two 128-byte blocks per segment), or that the EDID is complete,
with typed warnings. It is `no_std` without `alloc` and adds no dependency; thin drivers run
it over a bus (for example `hdmi-hal`'s planned `DdcBus`), so the logic exists once.

The plan and `parse_edid` share **one rule for how many blocks an EDID has**:

- **Done, unreleased:** HF-EEODB counts only as the first data block of the
  first extension (bytes 4–6 of block 1, in a CTA extension), as Linux and Amlogic read it,
  for `parse_edid` and capability parsing alike (`ExtensionCount`,
  `hf_eeodb_extension_count`); the EDID's size is the HF-EEODB count when present, so an
  EDID that uses it has all its blocks parsed; `MAX_EXTENSION_BLOCKS` is 255.
- **How many blocks to read:** the larger of byte 126 and the HF-EEODB count, by default and
  configurable; a disagreement is a typed warning with both values.
- **Checksums:** the plan checks every block and reports a bad one as a typed warning with its
  index; whether to read it again is the caller's. A bad base block is a typed error, since
  its block count cannot be trusted.
- **The block cap:** the plan holds no blocks, the caller does; without `alloc` a fixed
  capacity, the overflow counted, not dropped.

### DisplayID 2.x follow-ups

The remaining gap in 2.x coverage:

- **Type IX (`0x24`) standard CVT evaluator** — descriptors with algorithm code `0`
  (`CvtAlgorithm::Cvt`) pass through with only `(width, height, refresh_rate, cvt_algorithm)`
  set; no pixel clock or blanking parameters are derived. Standard CVT (non-reduced-blanking)
  is a distinct VESA algorithm requiring the full VESA CVT formulae; deferred until a
  real-world fixture is available to validate the implementation.

### Consistency validation

Helpers to detect internally inconsistent EDIDs: modes whose pixel clock exceeds the
declared maximum, refresh rates outside the declared range, conflicting identity fields.
These surface as warnings rather than errors, since the underlying data may still be useful.

### Broader fixture corpus

Expanding the fixture corpus — particularly with edge cases, malformed inputs, and displays
from a wider range of manufacturers — will increase confidence in the normalization layer
and make refactoring safer.
