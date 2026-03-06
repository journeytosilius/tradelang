# Builtins

TradeLang currently provides four builtins:

- `sma(series, length)`
- `ema(series, length)`
- `rsi(series, length)`
- `plot(value)`

## Indicator Builtins

`sma`, `ema`, and `rsi` are deterministic and side-effect free.

They operate on series values and respect sparse update clocks:

- a weekly source series does not get re-counted on every minute bar
- indicator state advances only when the source series advances
- warm-up periods yield `na`

## `plot`

`plot` is terminal output. It produces plot series in runtime outputs but is not a composable upstream input in pipeline mode.

Example:

```tradelang
plot(sma(close, 14))
```
