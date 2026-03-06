# Types and Values

PalmScript currently works with scalar and series forms of numbers, booleans, and missing values.

## Primitive Values

- numeric values are `f64`
- booleans are `true` or `false`
- `na` represents missing data

## Series Semantics

Market data and most derived computations are series.

- `x[0]` means the current sample
- `x[1]` means the previous sample
- `x[n]` means `n` samples ago

If insufficient history exists, indexing yields `na`.

## Missing Values

`na` is part of normal semantics, not a fatal runtime condition.

Common cases:

- insufficient lookback for `x[n]`
- indicator warm-up periods
- interval-qualified series before the first fully closed candle exists
- runtime gaps that intentionally materialize as missing values

## Boolean Logic

`and` and `or` implement PalmScript's three-valued logic. They do not simply coerce `na` to `false`. Use them when composing indicator and filter logic that may still be warming up.
