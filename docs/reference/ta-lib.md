# TA-Lib Surface

PalmScript now includes a typed TA-Lib integration layer anchored to upstream TA-Lib commit `1bdf54384036852952b8b4cb97c09359ae407bd0`.

This repository does not yet expose the entire TA-Lib function catalog at runtime, but it does pin the upstream metadata source and uses typed language features that are required for the broader port:

- `ma_type.<variant>` enum literals
- tuple destructuring for multi-output TA-Lib builtins
- TA-Lib metadata snapshot in `src/talib.rs`
- importer tooling under `tools/`
- a generated 161-function catalog in `src/talib_generated.rs`

Current metadata-driven surface behavior:

- all 161 TA-Lib function names are reserved as builtin names
- IDE completion and hover can show the generated TA-Lib signatures and summaries
- calling a catalog function that is not implemented yet produces a deterministic compile diagnostic instead of being treated as an unknown identifier

Implemented TA-Lib-style builtins in this change:

- `ma(series, length, ma_type)`
- `macd(series, fast_length, slow_length, signal_length)`

Current `ma_type` variants:

- `ma_type.sma`
- `ma_type.ema`
- `ma_type.wma`
- `ma_type.dema`
- `ma_type.tema`
- `ma_type.trima`
- `ma_type.kama`
- `ma_type.mama`
- `ma_type.t3`

Only `sma`, `ema`, and `wma` are currently executable through `ma(...)`. The remaining variants are reserved in the typed surface so later TA-Lib batches can extend behavior without changing syntax.

Tuple-return example:

```palm
interval 1m

let (line, signal, hist) = macd(close, 12, 26, 9)
plot(line)
plot(signal)
plot(hist)
```
