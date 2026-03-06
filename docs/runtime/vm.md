# VM Semantics

The PalmScript VM executes one compiled program against one base-step clock.

## Hot Path Rules

The VM hot path is intentionally conservative:

- no filesystem access
- no network access
- no wall clock access
- no heap-allocation-heavy dynamic execution model

## Series Execution

The VM works with:

- current values
- bounded history buffers
- sparse updates for slower interval-derived series

Indicator state is version-aware so slower source series such as `1w.close` are not accidentally double-counted on faster base clocks.

## Runtime Outputs

At each step the engine produces a `StepOutput`. Over a full run it accumulates `Outputs`.

These structures power:

- CLI JSON and text output
- pipeline outputs
- downstream trigger handling
