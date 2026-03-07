# Builtins

This page defines the builtin functions and predefined market names implemented by PalmScript.

## Builtin Function Set

PalmScript currently provides these callable builtins:

- `sma(series, length)`
- `ema(series, length)`
- `rsi(series, length)`
- `ma(series, length, ma_type)`
- `apo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`
- `ppo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`
- `macd(series, fast_length, slow_length, signal_length)`
- `acos(real)`
- `asin(real)`
- `atan(real)`
- `ceil(real)`
- `cos(real)`
- `cosh(real)`
- `exp(real)`
- `floor(real)`
- `ln(real)`
- `log10(real)`
- `sin(real)`
- `sinh(real)`
- `sqrt(real)`
- `tan(real)`
- `tanh(real)`
- `add(a, b)`
- `div(a, b)`
- `mult(a, b)`
- `sub(a, b)`
- `avgprice(open, high, low, close)`
- `medprice(high, low)`
- `typprice(high, low, close)`
- `wclprice(high, low, close)`
- `max(series[, length=30])`
- `min(series[, length=30])`
- `sum(series[, length=30])`
- `midpoint(series[, length=14])`
- `midprice(high, low[, length=14])`
- `wma(series[, length=30])`
- `avgdev(series[, length=14])`
- `maxindex(series[, length=30])`
- `minindex(series[, length=30])`
- `minmax(series[, length=30])`
- `minmaxindex(series[, length=30])`
- `stddev(series[, length=5[, deviations=1.0]])`
- `var(series[, length=5[, deviations=1.0]])`
- `linearreg(series[, length=14])`
- `linearreg_angle(series[, length=14])`
- `linearreg_intercept(series[, length=14])`
- `linearreg_slope(series[, length=14])`
- `tsf(series[, length=14])`
- `beta(series0, series1[, length=5])`
- `correl(series0, series1[, length=30])`
- `cmo(series[, length=14])`
- `willr(high, low, close[, length=14])`
- `obv(series, volume)`
- `trange(high, low, close)`
- `plot(value)`
- `above(a, b)`
- `below(a, b)`
- `between(x, low, high)`
- `outside(x, low, high)`
- `cross(a, b)`
- `crossover(a, b)`
- `crossunder(a, b)`
- `change(series, length)`
- `roc(series[, length=10])`
- `mom(series[, length=10])`
- `rocp(series[, length=10])`
- `rocr(series[, length=10])`
- `rocr100(series[, length=10])`
- `highest(series, length)`
- `lowest(series, length)`
- `rising(series, length)`
- `falling(series, length)`
- `barssince(condition)`
- `valuewhen(condition, source, occurrence)`

PalmScript also reserves these predefined market names:

- `open`
- `high`
- `low`
- `close`
- `volume`
- `time`

The predefined market names are identifiers, not callable functions. `close()` is rejected.

## Common Builtin Rules

Rules:

- all builtins are deterministic
- builtins must not perform I/O, access time, or access the network
- `plot` writes to the output stream; all other builtins are pure
- helper builtins propagate `na` unless a more specific rule below overrides that behavior
- helper builtins follow the update clocks implied by their series arguments

## Indicators

### `sma(series, length)`

Rules:

- it requires exactly two arguments
- the first argument must be `series<float>`
- the second argument must be a positive integer literal
- the result type is `series<float>`
- if insufficient history exists, the current sample is `na`
- if the required window contains `na`, the current sample is `na`

### `ema(series, length)`

Rules:

- it requires exactly two arguments
- the first argument must be `series<float>`
- the second argument must be a positive integer literal
- the result type is `series<float>`
- the series returns `na` until the seed window is available

### `rsi(series, length)`

Rules:

- it requires exactly two arguments
- the first argument must be `series<float>`
- the second argument must be a positive integer literal
- the result type is `series<float>`
- the series returns `na` until enough history exists to seed the indicator state

### `ma(series, length, ma_type)`

Rules:

- it requires exactly three arguments
- the first argument must be `series<float>`
- the second argument must be a positive integer literal
- the third argument must be a typed `ma_type.<variant>` value
- the result type is `series<float>`
- `ma_type.sma`, `ma_type.ema`, and `ma_type.wma` are currently implemented

### `apo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])` and `ppo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`

Rules:

- the first argument must be `series<float>`
- `fast_length` and `slow_length` default to `12` and `26`
- if provided, `fast_length` and `slow_length` must be integer literals greater than or equal to `2`
- if provided, the fourth argument must be a typed `ma_type.<variant>` value
- omitted `ma_type` defaults to `ma_type.sma`
- `apo` returns `fast_ma - slow_ma`
- `ppo` returns `((fast_ma - slow_ma) / slow_ma) * 100`
- if the slow moving average is `0`, `ppo` returns `0`
- `ma_type.sma`, `ma_type.ema`, and `ma_type.wma` are currently implemented
- the result type is `series<float>`

### `macd(series, fast_length, slow_length, signal_length)`

Rules:

- it requires exactly four arguments
- the first argument must be `series<float>`
- the remaining arguments must be positive integer literals
- the result type is a 3-tuple of series values in TA-Lib order: `(macd_line, signal, histogram)`
- the result must be destructured before it can be used in `plot`, `export`, conditions, or further expressions

### TA-Lib math transforms

These builtins are currently executable:

- `acos(real)`
- `asin(real)`
- `atan(real)`
- `ceil(real)`
- `cos(real)`
- `cosh(real)`
- `exp(real)`
- `floor(real)`
- `ln(real)`
- `log10(real)`
- `sin(real)`
- `sinh(real)`
- `sqrt(real)`
- `tan(real)`
- `tanh(real)`

Rules:

- each requires exactly one numeric or `series<float>` argument
- if the input is a series, the result type is `series<float>`
- if the input is scalar, the result type is `float`
- if the input is `na`, the result is `na`

### TA-Lib arithmetic and price transforms

These builtins are currently executable:

- `add(a, b)`
- `div(a, b)`
- `mult(a, b)`
- `sub(a, b)`
- `avgprice(open, high, low, close)`
- `medprice(high, low)`
- `typprice(high, low, close)`
- `wclprice(high, low, close)`

Rules:

- all arguments must be numeric, `series<float>`, or `na`
- if any argument is a series, the result type is `series<float>`
- otherwise the result type is `float`
- if any required input is `na`, the result is `na`

### TA-Lib rolling window helpers

These builtins are currently executable:

- `wma(series[, length=30])`
- `avgdev(series[, length=14])`
- `maxindex(series[, length=30])`
- `minindex(series[, length=30])`
- `minmax(series[, length=30])`
- `minmaxindex(series[, length=30])`
- `stddev(series[, length=5[, deviations=1.0]])`
- `var(series[, length=5[, deviations=1.0]])`
- `linearreg(series[, length=14])`
- `linearreg_angle(series[, length=14])`
- `linearreg_intercept(series[, length=14])`
- `linearreg_slope(series[, length=14])`
- `tsf(series[, length=14])`
- `beta(series0, series1[, length=5])`
- `correl(series0, series1[, length=30])`

Rules:

- the first argument must be `series<float>`
- `beta` and `correl` require `series<float>` as both inputs
- the optional `length` must be an integer literal that satisfies the TA-Lib minimum for that builtin
- omitted `length` uses the TA-Lib default for that builtin
- `wma` and `avgdev` return `series<float>`
- `maxindex` and `minindex` return `series<float>` containing the absolute bar index as `f64`
- `minmax` returns a 2-tuple `(min_value, max_value)` in TA-Lib output order
- `minmaxindex` returns a 2-tuple `(min_index, max_index)` in TA-Lib output order
- tuple-valued outputs must be destructured before further use
- if insufficient history exists, the current sample is `na`
- if the required window contains `na`, the current sample is `na`

Additional statistics rules:

- `stddev` defaults to `length=5` and `deviations=1.0`
- `stddev` requires `length >= 2`
- `var` defaults to `length=5`, allows `length >= 1`, and ignores the `deviations` argument to match TA-Lib
- `stddev` multiplies the square root of the rolling variance by `deviations`
- `linearreg`, `linearreg_angle`, `linearreg_intercept`, `linearreg_slope`, and `tsf` default to `length=14`
- `linearreg` returns the fitted value at the current bar
- `tsf` returns the one-step-ahead forecast
- `beta` defaults to `length=5` and follows TA-Lib's return-ratio formulation, so it first yields output after `length + 1` source samples
- `correl` defaults to `length=30` and returns the Pearson correlation of the paired raw input series

## Relational Helpers

### `above(a, b)` and `below(a, b)`

Rules:

- both arguments must be numeric, `series<float>`, or `na`
- `above(a, b)` evaluates as `a > b`
- `below(a, b)` evaluates as `a < b`
- if any required input is `na`, the result is `na`
- if either input is a series, the result type is `series<bool>`
- otherwise the result type is `bool`

### `between(x, low, high)` and `outside(x, low, high)`

Rules:

- all arguments must be numeric, `series<float>`, or `na`
- `between(x, low, high)` evaluates as `low < x and x < high`
- `outside(x, low, high)` evaluates as `x < low or x > high`
- if any required input is `na`, the result is `na`
- if any argument is a series, the result type is `series<bool>`
- otherwise the result type is `bool`

## Crossing Helpers

### `crossover(a, b)`

Rules:

- both arguments must be numeric, `series<float>`, or `na`
- at least one argument must be `series<float>`
- scalar arguments are treated as thresholds, so their prior sample is their current value
- it evaluates as current `a > b` and prior `a[1] <= b[1]`
- if any required current or prior sample is `na`, the result is `na`
- the result type is `series<bool>`

### `crossunder(a, b)`

Rules:

- both arguments must be numeric, `series<float>`, or `na`
- at least one argument must be `series<float>`
- scalar arguments are treated as thresholds, so their prior sample is their current value
- it evaluates as current `a < b` and prior `a[1] >= b[1]`
- if any required current or prior sample is `na`, the result is `na`
- the result type is `series<bool>`

### `cross(a, b)`

Rules:

- both arguments follow the same contract as `crossover` and `crossunder`
- it evaluates as `crossover(a, b) or crossunder(a, b)`
- if any required current or prior sample is `na`, the result is `na`
- the result type is `series<bool>`

## Series and Window Helpers

### `change(series, length)`

Rules:

- it requires exactly two arguments
- the first argument must be `series<float>`
- the second argument must be a positive integer literal
- it evaluates as `series - series[length]`
- if the current or referenced sample is `na`, the result is `na`
- the result type is `series<float>`

### `roc(series[, length=10])`, `mom(series[, length=10])`, `rocp(series[, length=10])`, `rocr(series[, length=10])`, and `rocr100(series[, length=10])`

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

### `cmo(series[, length=14])`

Rules:

- the first argument must be `series<float>`
- omitted `length` uses the TA-Lib default of `14`
- if provided, `length` must be an integer literal greater than or equal to `2`
- `cmo` uses TA-Lib's Wilder-style smoothed gain and loss state
- the result type is `series<float>`
- if the smoothed gain and loss sum to `0`, `cmo` returns `0`

### `willr(high, low, close[, length=14])`

Rules:

- the first three arguments must be `series<float>`
- omitted `length` uses the TA-Lib default of `14`
- if provided, `length` must be an integer literal greater than or equal to `2`
- `willr` uses the trailing highest high and lowest low over the requested window
- the result type is `series<float>`
- if the trailing high-low range is `0`, `willr` returns `0`
- the result type is `series<float>`

### `highest(series, length)` and `lowest(series, length)`

Rules:

- the first argument must be `series<float>`
- the second argument must be a positive integer literal
- the window includes the current sample
- if insufficient history exists, the result is `na`
- if any sample in the required window is `na`, the result is `na`
- the result type is `series<float>`

### `max(series[, length=30])`, `min(series[, length=30])`, and `sum(series[, length=30])`

Rules:

- the first argument must be `series<float>`
- the optional trailing window defaults to `30`
- if provided, the window must be an integer literal greater than or equal to `2`
- the window includes the current sample
- if insufficient history exists, the result is `na`
- if any sample in the required window is `na`, the result is `na`
- the result type is `series<float>`

### `midpoint(series[, length=14])` and `midprice(high, low[, length=14])`

Rules:

- `midpoint` requires `series<float>` as the first argument
- `midprice` requires `series<float>` for both `high` and `low`
- the optional trailing window defaults to `14`
- if provided, the window must be an integer literal greater than or equal to `2`
- the window includes the current sample
- if insufficient history exists, the result is `na`
- if any required sample in the window is `na`, the result is `na`
- the result type is `series<float>`

### `rising(series, length)` and `falling(series, length)`

Rules:

- the first argument must be `series<float>`
- the second argument must be a positive integer literal
- `rising(series, length)` means the current sample is strictly greater than every prior sample in the trailing `length` bars
- `falling(series, length)` means the current sample is strictly less than every prior sample in the trailing `length` bars
- if insufficient history exists, the result is `na`
- if any required sample is `na`, the result is `na`
- the result type is `series<bool>`

### `obv(series, volume)`

Rules:

- both arguments must be `series<float>`
- the first output sample seeds from the current `volume`
- later samples add or subtract the current `volume` based on whether `series` rose or fell from the prior bar
- if the current price or volume sample is `na`, the result is `na`
- the result type is `series<float>`

### `trange(high, low, close)`

Rules:

- all arguments must be `series<float>`
- the first output sample is `na`
- later samples use TA-Lib true range semantics based on current `high`, current `low`, and prior `close`
- if any required sample is `na`, the result is `na`
- the result type is `series<float>`

## Event Memory Helpers

### `barssince(condition)`

Rules:

- it requires exactly one argument
- the argument must be `series<bool>`
- it returns `0` on bars where the current condition sample is `true`
- it increments on each update of the condition's own clock after the last true event
- it returns `na` until the first true event
- if the current condition sample is `na`, the current output is `na`
- the result type is `series<float>`

### `valuewhen(condition, source, occurrence)`

Rules:

- it requires exactly three arguments
- the first argument must be `series<bool>`
- the second argument must be `series<float>` or `series<bool>`
- the third argument must be a non-negative integer literal
- occurrence `0` means the most recent true event
- the result type matches the second argument type
- it returns `na` until enough matching true events exist
- if the current condition sample is `na`, the current output is `na`
- when the current condition sample is `true`, the current `source` sample is captured for future occurrences

## `plot(value)`

`plot` emits a plot point for the current step.

Rules:

- it requires exactly one argument
- the argument must be numeric, `series<float>`, or `na`
- the expression result type is `void`
- `plot` must not be called inside a user-defined function body

At runtime:

- numeric values are recorded as plot points
- `na` records a plot point with no numeric value

## Update Clocks

Builtin results follow the update clocks of their inputs.

Examples:

- `ema(close, 20)` advances on the base clock
- `highest(1w.close, 5)` advances on the weekly clock
- `crossover(hl.close, bn.close)` advances when either referenced source series advances
- `barssince(close > close[1])` advances on the clock of that condition series
- `valuewhen(trigger_series, hl.1h.close, 0)` advances on the clock of `trigger_series`
