# Indicators Overview

This section defines PalmScript's executable indicator surface.

Use [Builtins](builtins.md) for shared callable rules, helper builtins, `plot`, and tuple-destructuring rules that apply across the language.

## Indicator Families

PalmScript currently documents indicators in these families:

- [Trend and Overlap](indicators-trend-and-overlap.md)
- [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md)
- [Math, Price, and Statistics](indicators-math-price-statistics.md)

## Shared Indicator Rules

Rules:

- indicator names are builtin identifiers, so they are called directly, for example `ema(spot.close, 20)`
- indicator inputs must still follow the source-qualified series rules from [Intervals and Sources](intervals-and-sources.md)
- optional length arguments use the TA-Lib defaults documented on the family pages
- length-like arguments that are described as literals must be integer literals in source code
- tuple-valued indicators must be destructured with `let (...) = ...` before further use
- indicator outputs follow the update clock implied by their series inputs
- indicators propagate `na` unless the specific indicator contract says otherwise

## Tuple-Valued Indicators

The current tuple-valued indicators are:

- `macd(series, fast_length, slow_length, signal_length)`
- `minmax(series[, length=30])`
- `minmaxindex(series[, length=30])`
- `aroon(high, low[, length=14])`
- `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])`
- `donchian(high, low[, length=20])`

These must be destructured immediately:

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let (line, signal, hist) = macd(spot.close, 12, 26, 9)
plot(line)
```

## Executable vs Reserved TA-Lib Names

PalmScript reserves a broader TA-Lib catalog than it executes today.

- these indicator pages define the executable subset
- [TA-Lib Surface](ta-lib.md) defines the broader reserved-name and metadata surface
- a reserved-but-not-yet-executable TA-Lib name produces a deterministic compile diagnostic
