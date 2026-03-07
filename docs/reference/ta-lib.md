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
- committed oracle fixtures under `tests/data/ta_lib/` now validate the implemented subset against the upstream C library

Implemented TA-Lib-style builtins in this change:

- `ma(series, length, ma_type)`
- `apo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`
- `ppo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`
- `macd(series, fast_length, slow_length, signal_length)`
- unary math transforms: `acos`, `asin`, `atan`, `ceil`, `cos`, `cosh`, `exp`, `floor`, `ln`, `log10`, `sin`, `sinh`, `sqrt`, `tan`, `tanh`
- math operators: `add`, `div`, `mult`, `sub`, `max`, `min`, `sum`, `maxindex`, `minindex`, `minmax`, `minmaxindex`
- price transforms: `avgprice`, `medprice`, `typprice`, `wclprice`
- overlap helpers: `midpoint`, `midprice`, `wma`
- statistics helpers: `avgdev`, `stddev`, `var`, `linearreg`, `linearreg_angle`, `linearreg_intercept`, `linearreg_slope`, `tsf`, `beta`, `correl`
- momentum helpers: `apo`, `ppo`, `aroon`, `aroonosc`, `bop`, `cci`, `cmo`, `mom`, `roc`, `rocp`, `rocr`, `rocr100`, `willr`
- volume and volatility helpers: `obv`, `trange`

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

Only `sma`, `ema`, and `wma` are currently executable through `ma(...)`, `apo(...)`, and `ppo(...)`. The remaining variants are reserved in the typed surface so later TA-Lib batches can extend behavior without changing syntax.

Current TA-Lib defaults now honored in the executable surface:

- `max`, `min`, and `sum` default to a window of `30`
- `midpoint` and `midprice` default to a window of `14`
- `wma`, `maxindex`, `minindex`, `minmax`, and `minmaxindex` default to a window of `30`
- `avgdev` defaults to a window of `14`
- `stddev` and `var` default to `length=5`
- `linearreg`, `linearreg_angle`, `linearreg_intercept`, `linearreg_slope`, and `tsf` default to `length=14`
- `beta` defaults to `length=5` and uses TA-Lib's return-based beta calculation
- `correl` defaults to `length=30`
- `apo` and `ppo` default to `fast_length=12`, `slow_length=26`, and `ma_type.sma`
- `aroon` and `aroonosc` default to `length=14`
- `cci` defaults to `length=14`
- `cmo` defaults to `length=14`
- `mom`, `roc`, `rocp`, `rocr`, and `rocr100` default to `length=10`
- `willr` defaults to `length=14`

Oracle fixture refresh for the implemented subset:

```bash
python3 tools/generate_talib_fixtures.py
cargo test --test ta_lib_parity
```

Tuple-return example:

```palm
interval 1m

let (line, signal, hist) = macd(close, 12, 26, 9)
plot(line)
plot(signal)
plot(hist)
```
