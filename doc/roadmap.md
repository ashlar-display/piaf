# Roadmap

## Planned

### DisplayID 2.x follow-ups

The remaining gap in 2.x coverage:

- **Type IX (`0x24`) standard CVT evaluator** — descriptors with algorithm code `0`
  (`CvtAlgorithm::Cvt`) pass through with only `(width, height, refresh_rate, cvt_algorithm)`
  set; no pixel clock or blanking parameters are derived. Standard CVT (non-reduced-blanking)
  is a distinct VESA algorithm requiring the full CVT spec formulae; deferred until a
  real-world fixture is available to validate the implementation.

### Consistency validation

Helpers to detect internally inconsistent EDIDs: modes whose pixel clock exceeds the
declared maximum, refresh rates outside the declared range, conflicting identity fields.
These surface as warnings rather than errors, since the underlying data may still be useful.

### Broader fixture corpus

Expanding the fixture corpus — particularly with edge cases, malformed inputs, and displays
from a wider range of manufacturers — will increase confidence in the normalization layer
and make refactoring safer.
