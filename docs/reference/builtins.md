# Builtins

This page defines PalmScript's shared builtin rules and the non-indicator builtin helpers.

Indicator-specific contracts live in the dedicated [Indicators](indicators.md) section.

## Executable Builtins vs Reserved Names

PalmScript exposes three related surfaces:

- executable builtin helpers and outputs documented on this page
- executable indicators documented in the [Indicators](indicators.md) section
- a broader reserved TA-Lib catalog described in [TA-Lib Surface](ta-lib.md)

Not every reserved TA-Lib name is executable today. Reserved-but-not-yet-executable names produce deterministic compile diagnostics instead of being treated as unknown identifiers.

## Builtin Categories

PalmScript currently exposes these builtin categories:

- indicators: [Trend and Overlap](indicators-trend-and-overlap.md), [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md), and [Math, Price, and Statistics](indicators-math-price-statistics.md)
- relational helpers: `above`, `below`, `between`, `outside`
- crossing helpers: `cross`, `crossover`, `crossunder`
- null helpers: `na(value)`, `nz(value[, fallback])`, `coalesce(value, fallback)`
- series and window helpers: `change`, `highest`, `lowest`, `highestbars`, `lowestbars`, `rising`, `falling`, `cum`
- event-memory helpers: `activated`, `deactivated`, `barssince`, `valuewhen`, `highest_since`, `lowest_since`, `highestbars_since`, `lowestbars_since`, `valuewhen_since`, `count_since`
- outputs: `plot`

Market fields are selected through source-qualified series such as `spot.open`, `spot.close`, or `hl.1h.volume`. Only identifiers are callable, so `spot.close()` is rejected.

## Tuple-Valued Builtins

The current executable tuple-valued builtins are:

- `macd(series, fast_length, slow_length, signal_length)` documented in [Trend and Overlap](indicators-trend-and-overlap.md)
- `minmax(series[, length=30])` documented in [Math, Price, and Statistics](indicators-math-price-statistics.md)
- `minmaxindex(series[, length=30])` documented in [Math, Price, and Statistics](indicators-math-price-statistics.md)
- `aroon(high, low[, length=14])` documented in [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md)

All tuple-valued builtin results must be destructured immediately with `let (...) = ...` before further use.

## Common Builtin Rules

Rules:

- all builtins are deterministic
- builtins must not perform I/O, access time, or access the network
- `plot` writes to the output stream; all other builtins are pure
- builtin helpers and indicators propagate `na` unless a more specific rule overrides that behavior
- builtin results follow the update clocks implied by their series arguments

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

### `highest(series, length)` and `lowest(series, length)`

Rules:

- the first argument must be `series<float>`
- the second argument must be a positive integer literal
- the window includes the current sample
- if insufficient history exists, the result is `na`
- if any sample in the required window is `na`, the result is `na`
- the result type is `series<float>`

The `length` argument may be a positive integer literal or a top-level immutable numeric binding declared with `const` or `input`.

### `highestbars(series, length)` and `lowestbars(series, length)`

Rules:

- the first argument must be `series<float>`
- the second argument follows the same positive-integer rule as `highest` / `lowest`
- the window includes the current sample
- the result is the number of bars since the highest or lowest sample in the active window
- if insufficient history exists, the result is `na`
- if any sample in the required window is `na`, the result is `na`
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

### `cum(value)`

Rules:

- it requires exactly one numeric or `series<float>` argument
- it returns the cumulative running sum on the argument's update clock
- if the current input sample is `na`, the current output sample is `na`
- later non-`na` samples continue accumulating from the prior running total
- the result type is `series<float>`

## Null Helpers

### `na(value)`

Rules:

- it requires exactly one argument
- it returns `true` when the current argument sample is `na`
- it returns `false` when the current argument sample is a concrete scalar value
- if the argument is series-backed, the result type is `series<bool>`
- otherwise the result type is `bool`

### `nz(value[, fallback])`

Rules:

- it accepts one or two arguments
- with one argument, numeric inputs use `0` and boolean inputs use `false` as the fallback
- with two arguments, the second argument is returned when the first is `na`
- both arguments must be type-compatible numeric or bool values
- the result type follows the lifted type of the operands

### `coalesce(value, fallback)`

Rules:

- it requires exactly two arguments
- it returns the first argument when it is not `na`
- otherwise it returns the second argument
- both arguments must be type-compatible numeric or bool values
- the result type follows the lifted type of the operands

## Event Memory Helpers

### `activated(condition)` and `deactivated(condition)`

Rules:

- both require exactly one argument
- the argument must be `series<bool>`
- `activated` returns `true` when the current condition sample is `true` and the prior sample was `false` or `na`
- `deactivated` returns `true` when the current condition sample is `false` and the prior sample was `true`
- if the current sample is `na`, both helpers return `false`
- the result type is `series<bool>`

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

### `highest_since(anchor, source)` and `lowest_since(anchor, source)`

Rules:

- both require exactly two arguments
- the first argument must be `series<bool>`
- the second argument must be `series<float>`
- when the current anchor sample is `true`, a new anchored epoch starts on the current bar
- the current bar contributes immediately to the new epoch
- before the first anchor, the result is `na`
- later true anchors discard the prior anchored epoch and start a fresh one
- the result type is `series<float>`

### `highestbars_since(anchor, source)` and `lowestbars_since(anchor, source)`

Rules:

- both require exactly two arguments
- the first argument must be `series<bool>`
- the second argument must be `series<float>`
- they follow the same anchored-epoch reset rules as `highest_since` / `lowest_since`
- the result is the number of bars since the highest or lowest sample inside the current anchored epoch
- before the first anchor, the result is `na`
- the result type is `series<float>`

### `valuewhen_since(anchor, condition, source, occurrence)`

Rules:

- it requires exactly four arguments
- the first and second arguments must be `series<bool>`
- the third argument must be `series<float>` or `series<bool>`
- the fourth argument must be a non-negative integer literal
- when the current anchor sample is `true`, prior `condition` matches are forgotten and a new anchored epoch starts on the current bar
- occurrence `0` means the most recent matching event inside the current anchored epoch
- before the first anchor, the result is `na`
- the result type matches the third argument type

### `count_since(anchor, condition)`

Rules:

- it requires exactly two arguments
- both arguments must be `series<bool>`
- when the current anchor sample is `true`, the running count resets and a new anchored epoch starts on the current bar
- the current bar contributes immediately to the new anchored epoch
- the count increments only on bars where the current `condition` sample is `true`
- before the first anchor, the result is `na`
- later true anchors discard the prior anchored epoch and start a fresh one
- the result type is `series<float>`

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

- `ema(spot.close, 20)` advances on the base clock
- `highest(spot.1w.close, 5)` advances on the weekly clock
- `cum(spot.1w.close - spot.1w.close[1])` advances on the weekly clock
- `crossover(hl.close, bn.close)` advances when either referenced source series advances
- `activated(trend_long)` advances on the clock of `trend_long`
- `barssince(spot.close > spot.close[1])` advances on the clock of that condition series
- `valuewhen(trigger_series, hl.1h.close, 0)` advances on the clock of `trigger_series`
- `highest_since(position_event.long_entry_fill, spot.high)` advances on the clock shared by the anchor and source series
