# Bounded History and Memory

PalmScript is designed for deterministic bounded-memory execution.

## History Caps

The compiler computes required history from:

- explicit indexing such as `x[3]`
- indicator windows such as `ema(close, 14)`
- output/materialization needs

The runtime enforces `VmLimits`, including `max_history_capacity`. If a compiled program requires more history than the configured limit allows, execution fails deterministically instead of silently truncating data.

## Ring Buffers

Series history is stored in bounded buffers. The runtime does not allow unbounded growth during normal strategy execution.

## Sparse Series Updates

Multi-interval execution and external inputs can advance at different effective clocks. PalmScript tracks those update clocks so derived series and indicators only advance when their source data actually advances.
