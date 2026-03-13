# Trend and Overlap Indicators

This page defines PalmScript's executable trend and overlap indicators.

## `sma(series, length)`

Rules:

- it requires exactly two arguments
- the first argument must be `series<float>`
- the second argument must be a positive integer literal
- the result type is `series<float>`
- if insufficient history exists, the current sample is `na`
- if the required window contains `na`, the current sample is `na`

## `ema(series, length)`

Rules:

- it requires exactly two arguments
- the first argument must be `series<float>`
- the second argument must be a positive integer literal
- the result type is `series<float>`
- the series returns `na` until the seed window is available

## `ma(series, length, ma_type)`

Rules:

- it requires exactly three arguments
- the first argument must be `series<float>`
- the second argument must be a positive integer literal
- the third argument must be a typed `ma_type.<variant>` value
- the result type is `series<float>`
- all `ma_type` variants are implemented
- `ma_type.mama` matches upstream TA-Lib behavior and ignores the explicit `length` parameter, using MAMA defaults `fast_limit=0.5` and `slow_limit=0.05`

## `apo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])` and `ppo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`

Rules:

- the first argument must be `series<float>`
- `fast_length` and `slow_length` default to `12` and `26`
- if provided, `fast_length` and `slow_length` must be integer literals greater than or equal to `2`
- if provided, the fourth argument must be a typed `ma_type.<variant>` value
- omitted `ma_type` defaults to `ma_type.sma`
- `apo` returns `fast_ma - slow_ma`
- `ppo` returns `((fast_ma - slow_ma) / slow_ma) * 100`
- if the slow moving average is `0`, `ppo` returns `0`
- the same executable `ma_type` variants as `ma(...)` are supported
- the result type is `series<float>`

## `macd(series, fast_length, slow_length, signal_length)`

Rules:

- it requires exactly four arguments
- the first argument must be `series<float>`
- the remaining arguments must be positive integer literals
- the result type is a 3-tuple of series values in TA-Lib order: `(macd_line, signal, histogram)`
- the result must be destructured before it can be used in `plot`, `export`, conditions, or further expressions

## `macdfix(series[, signal_length=9])`

Rules:

- the first argument must be `series<float>`
- the optional `signal_length` defaults to `9`
- if provided, `signal_length` must be a positive integer literal
- the result type is a 3-tuple of series values in TA-Lib order: `(macd_line, signal, histogram)`
- the result must be destructured before it can be used in `plot`, `export`, conditions, or further expressions

## `macdext(series[, fast_length=12[, fast_ma=ma_type.sma[, slow_length=26[, slow_ma=ma_type.sma[, signal_length=9[, signal_ma=ma_type.sma]]]]]])`

Rules:

- the first argument must be `series<float>`
- omitted lengths use TA-Lib defaults `12`, `26`, and `9`
- `fast_length` and `slow_length` must be integer literals greater than or equal to `2`
- `signal_length` must be an integer literal greater than or equal to `1`
- each MA argument must be a typed `ma_type.<variant>` value
- the same executable `ma_type` variants as `ma(...)` are supported
- the result type is a 3-tuple of series values in TA-Lib order: `(macd_line, signal, histogram)`
- the result must be destructured before further use

## `bbands(series[, length=5[, deviations_up=2.0[, deviations_down=2.0[, ma_type=ma_type.sma]]]])`

Rules:

- the first argument must be `series<float>`
- the optional `length` defaults to `5`
- if provided, `length` must be a positive integer literal
- if provided, `deviations_up` and `deviations_down` must be numeric scalars
- if provided, the fifth argument must be a typed `ma_type.<variant>` value
- the result type is a 3-tuple of series values in TA-Lib order: `(upper, middle, lower)`
- the result must be destructured before it can be used in `plot`, `export`, conditions, or further expressions

## `accbands(high, low, close[, length=20])`

Rules:

- the first three arguments must be `series<float>`
- omitted `length` uses the TA-Lib default of `20`
- if provided, `length` must be an integer literal greater than or equal to `2`
- the result type is a 3-tuple of series values in TA-Lib order: `(upper, middle, lower)`
- the result must be destructured before further use

## `dema(series[, length=30])`, `tema(series[, length=30])`, `trima(series[, length=30])`, `kama(series[, length=30])`, `t3(series[, length=5[, volume_factor=0.7]])`, and `trix(series[, length=30])`

Rules:

- the first argument must be `series<float>`
- the optional `length` defaults to `30` for `dema`, `tema`, `trima`, `kama`, and `trix`
- `t3` defaults to `length=5` and `volume_factor=0.7`
- if provided, `length` must be a positive integer literal
- if provided, `volume_factor` must be a numeric scalar
- the result type is `series<float>`

## `mavp(series, periods, minimum_period, maximum_period, ma_type)`

Rules:

- the first two arguments must be `series<float>`
- `minimum_period` and `maximum_period` must be integer literals greater than or equal to `2`
- the fifth argument must be a typed `ma_type.<variant>` value
- the moving-average family is the same executable `ma_type` subset as `ma(...)`
- `periods` is clamped per bar into `[minimum_period, maximum_period]`
- the result type is `series<float>`

## `mama(series[, fast_limit=0.5[, slow_limit=0.05]])`

Rules:

- the first argument must be `series<float>`
- `fast_limit` and `slow_limit` default to `0.5` and `0.05`
- if provided, both optional arguments must be numeric scalars
- the result type is a 2-tuple of series values in TA-Lib order: `(mama, fama)`
- the result must be destructured before further use

## `ht_dcperiod(series)`, `ht_dcphase(series)`, `ht_phasor(series)`, `ht_sine(series)`, `ht_trendline(series)`, and `ht_trendmode(series)`

Rules:

- each function requires exactly one `series<float>` argument
- `ht_dcperiod`, `ht_dcphase`, and `ht_trendline` return `series<float>`
- `ht_trendmode` returns `series<float>` with TA-Lib's `0`/`1` trend-mode values
- `ht_phasor` returns a 2-tuple `(inphase, quadrature)`
- `ht_sine` returns a 2-tuple `(sine, lead_sine)`
- tuple results must be destructured before further use
- these indicators follow TA-Lib's Hilbert-transform warmup behavior and yield `na` until the upstream lookback is satisfied

## `sar(high, low[, acceleration=0.02[, maximum=0.2]])` and `sarext(high, low[, ...])`

Rules:

- `high` and `low` must be `series<float>`
- all optional SAR parameters are numeric scalars
- `sar` returns the standard Parabolic SAR
- `sarext` exposes the extended TA-Lib SAR controls and returns negative values while short, matching upstream TA-Lib behavior
- the result type is `series<float>`

## `wma(series[, length=30])`

Rules:

- the first argument must be `series<float>`
- the optional `length` defaults to `30`
- if provided, `length` must be an integer literal greater than or equal to `2`
- the result type is `series<float>`
- if insufficient history exists, the current sample is `na`
- if the required window contains `na`, the current sample is `na`

## `midpoint(series[, length=14])` and `midprice(high, low[, length=14])`

Rules:

- `midpoint` requires `series<float>` as the first argument
- `midprice` requires `series<float>` for both `high` and `low`
- the optional trailing window defaults to `14`
- if provided, the window must be an integer literal greater than or equal to `2`
- the window includes the current sample
- if insufficient history exists, the result is `na`
- if any required sample in the window is `na`, the result is `na`
- the result type is `series<float>`

## `linearreg(series[, length=14])`, `linearreg_angle(series[, length=14])`, `linearreg_intercept(series[, length=14])`, `linearreg_slope(series[, length=14])`, and `tsf(series[, length=14])`

Rules:

- the first argument must be `series<float>`
- the optional `length` defaults to `14`
- if provided, `length` must be an integer literal greater than or equal to `2`
- if insufficient history exists, the current sample is `na`
- if the required window contains `na`, the current sample is `na`
- `linearreg` returns the fitted value at the current bar
- `linearreg_angle` returns the fitted slope angle
- `linearreg_intercept` returns the fitted intercept
- `linearreg_slope` returns the fitted slope
- `tsf` returns the one-step-ahead forecast
- the result type is `series<float>`

## `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])`

Rules:

- the first three arguments must be `series<float>`
- omitted `atr_length` defaults to `10`
- omitted `multiplier` defaults to `3.0`
- if provided, `atr_length` must be an integer literal greater than or equal to `1`
- if provided, `multiplier` must be a numeric scalar
- `supertrend` returns a 2-tuple `(line, bullish)`
- `line` is the active carried band and `bullish` is the persistent regime direction
- the ATR component uses Wilder smoothing and requires prior-close history, so the result is `na` until the lookback is satisfied
- tuple-valued outputs must be destructured before further use

## `donchian(high, low[, length=20])`

Rules:

- the first two arguments must be `series<float>`
- omitted `length` defaults to `20`
- if provided, `length` must be an integer literal greater than or equal to `1`
- `donchian` returns a 3-tuple `(upper, middle, lower)`
- `upper` is the trailing highest high, `lower` is the trailing lowest low, and `middle` is `(upper + lower) / 2`
- if insufficient history exists, or any required sample is `na`, the current tuple is `(na, na, na)`
- tuple-valued outputs must be destructured before further use
