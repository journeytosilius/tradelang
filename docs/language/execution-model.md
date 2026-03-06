# Execution Model

PalmScript scripts compile once and execute once per fully closed base-interval candle.

## Per-Bar Execution

For each base bar:

1. base market series are loaded
2. referenced higher or equal intervals are advanced up to the current fully closed base boundary
3. external inputs are injected if the host or pipeline provides them
4. bytecode executes
5. bounded series state is updated
6. outputs are emitted for the current bar

## Determinism

PalmScript execution is deterministic:

- no filesystem access
- no network access
- no system time access
- no randomness

The same compiled program and the same input bars produce the same outputs.

## Base Interval Ownership

Every script must declare exactly one base interval:

```palmscript
interval 1m
```

Unqualified series like `close` and `volume` always mean the current candle of that declared base interval.

## Output Timing

`plot`, `export`, and `trigger` all materialize per-bar outputs after the current instruction stream finishes. Triggers also emit discrete trigger events when their current sample is `true`.
