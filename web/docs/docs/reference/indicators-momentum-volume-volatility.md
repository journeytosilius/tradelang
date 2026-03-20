# Momentum, Volume, and Volatility Indicators

This page defines PalmScript's executable momentum, oscillator, volume, and volatility indicators.

## `rsi(series, length)`

Rules:

- it requires exactly two arguments
- the first argument must be `series<float>`
- the second argument must be a positive integer literal
- the result type is `series<float>`
- the series returns `na` until enough history exists to seed the indicator state

## `roc(series[, length=10])`, `mom(series[, length=10])`, `rocp(series[, length=10])`, `rocr(series[, length=10])`, and `rocr100(series[, length=10])`

Rules:

- the first argument must be `series<float>`
- the optional `length` must be a positive integer literal
- omitted `length` uses the TA-Lib default of `10`
- `roc` evaluates as `((series - series[length]) / series[length]) * 100`
- `mom` evaluates as `series - series[length]`
- `rocp` evaluates as `(series - series[length]) / series[length]`
- `rocr` evaluates as `series / series[length]`
- `rocr100` evaluates as `(series / series[length]) * 100`
- if the current or referenced sample is `na`, the result is `na`
- if `series[length]` is `0`, `roc`, `rocp`, `rocr`, and `rocr100` return `na`

## `cmo(series[, length=14])`

Rules:

- the first argument must be `series<float>`
- omitted `length` uses the TA-Lib default of `14`
- if provided, `length` must be an integer literal greater than or equal to `2`
- `cmo` uses TA-Lib's Wilder-style smoothed gain and loss state
- the result type is `series<float>`
- if the smoothed gain and loss sum to `0`, `cmo` returns `0`

## `cci(high, low, close[, length=14])`

Rules:

- the first three arguments must be `series<float>`
- omitted `length` uses the TA-Lib default of `14`
- if provided, `length` must be an integer literal greater than or equal to `2`
- `cci` uses the trailing typical-price average and mean deviation over the requested window
- if the current typical-price delta or mean deviation is `0`, `cci` returns `0`
- the result type is `series<float>`

## `aroon(high, low[, length=14])` and `aroonosc(high, low[, length=14])`

Rules:

- the first two arguments must be `series<float>`
- omitted `length` uses the TA-Lib default of `14`
- if provided, `length` must be an integer literal greater than or equal to `2`
- `aroon` uses a trailing `length + 1` high/low window to match TA-Lib lookback
- `aroon` returns a 2-tuple `(aroon_down, aroon_up)` in TA-Lib output order
- `aroonosc` returns `aroon_up - aroon_down`
- tuple-valued outputs must be destructured before further use

## `plus_dm(high, low[, length=14])`, `minus_dm(high, low[, length=14])`, `plus_di(high, low, close[, length=14])`, `minus_di(high, low, close[, length=14])`, `dx(high, low, close[, length=14])`, `adx(high, low, close[, length=14])`, and `adxr(high, low, close[, length=14])`

Rules:

- all price arguments must be `series<float>`
- omitted `length` uses the TA-Lib default of `14`
- if provided, `length` must be a positive integer literal
- `plus_dm` and `minus_dm` return Wilder-smoothed directional movement
- `plus_di` and `minus_di` return Wilder directional indicators
- `dx` returns the absolute directional spread scaled by 100
- `adx` returns the Wilder average of `dx`
- `adxr` returns the average of the current `adx` and the lagged `adx`
- if any required price input on the active bar is `na`, the result is `na` for that bar
- the result type is `series<float>`

## `atr(high, low, close[, length=14])` and `natr(high, low, close[, length=14])`

Rules:

- all arguments must be `series<float>`
- omitted `length` uses the TA-Lib default of `14`
- if provided, `length` must be a positive integer literal
- `atr` seeds from the initial average true range and then applies Wilder smoothing
- `natr` returns `(atr / close) * 100`
- if any required price input on the active bar is `na`, the result is `na` for that bar
- the result type is `series<float>`

## `willr(high, low, close[, length=14])`

Rules:

- the first three arguments must be `series<float>`
- omitted `length` uses the TA-Lib default of `14`
- if provided, `length` must be an integer literal greater than or equal to `2`
- `willr` uses the trailing highest high and lowest low over the requested window
- the result type is `series<float>`
- if the trailing high-low range is `0`, `willr` returns `0`

## `mfi(high, low, close, volume[, length=14])` and `imi(open, close[, length=14])`

Rules:

- all arguments must be `series<float>`
- omitted `length` uses the TA-Lib default of `14`
- if provided, `length` must be a positive integer literal
- `mfi` uses typical price and money flow over a trailing window
- `imi` uses trailing intraday open-close movement over the requested window
- the result type is `series<float>`

## `stoch(high, low, close[, fast_k=5[, slow_k=3[, slow_k_ma=ma_type.sma[, slow_d=3[, slow_d_ma=ma_type.sma]]]]])`, `stochf(high, low, close[, fast_k=5[, fast_d=3[, fast_d_ma=ma_type.sma]]])`, and `stochrsi(series[, time_period=14[, fast_k=5[, fast_d=3[, fast_d_ma=ma_type.sma]]]])`

Rules:

- all price or source arguments must be `series<float>`
- omitted periods use TA-Lib defaults
- `fast_k`, `slow_k`, and `fast_d`/`slow_d` lengths must be positive integer literals
- `time_period` for `stochrsi` must be an integer literal greater than or equal to `2`
- all MA arguments must be typed `ma_type.<variant>` values
- `stoch` returns `(slowk, slowd)` in TA-Lib order
- `stochf` returns `(fastk, fastd)` in TA-Lib order
- `stochrsi` returns `(fastk, fastd)` in TA-Lib order
- tuple-valued outputs must be destructured before further use

## `ad(high, low, close, volume)`, `adosc(high, low, close, volume[, fast_length=3[, slow_length=10]])`, and `obv(series, volume)`

Rules:

- all arguments must be `series<float>`
- `ad` returns the cumulative accumulation/distribution line
- `adosc` returns the difference between fast and slow EMAs of the accumulation/distribution line
- omitted `fast_length` and `slow_length` use the TA-Lib defaults `3` and `10`
- `obv` seeds from the current `volume` and later adds or subtracts volume based on price direction
- if the required price or volume sample is `na`, the result is `na`
- the result type is `series<float>`

## `trange(high, low, close)`

Rules:

- all arguments must be `series<float>`
- the first output sample is `na`
- later samples use TA-Lib true range semantics based on current `high`, current `low`, and prior `close`
- if any required sample is `na`, the result is `na`
- the result type is `series<float>`

## `anchored_vwap(anchor, price, volume)`

Rules:

- `anchor` must be `series<bool>`
- `price` and `volume` must be `series<float>`
- when the current `anchor` sample is `true`, the running VWAP resets on that same bar
- the anchor bar is included in the new anchored accumulation window
- if the current anchor, price, or volume sample is `na`, the current output sample is `na`
- if cumulative anchored volume is `0`, the current output sample is `na`
- the result type is `series<float>`
