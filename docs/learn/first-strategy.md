# First Strategy

This strategy runs on one-minute bars, computes two moving averages, exports a trend state, and plots the close only when the fast average is above the slow average.

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 5)
let slow = sma(spot.close, 10)

export trend = fast > slow

if trend {
    plot(spot.close)
} else {
    plot(na)
}
```

## What This Introduces

- `interval 1m` sets the base execution clock
- `source spot = ...` binds one exchange-backed market
- `spot.close` is a source-qualified base series
- `let` binds reusable expressions
- `export` emits a named output series
- `plot` emits chart-style numeric output
- `if / else` controls which values are emitted

## Try It In The Browser IDE

Open [https://palmscript.dev/app/](https://palmscript.dev/app/), paste the script into the editor, and run it over the curated dataset window with the date controls in the header.

## Extend It With Higher-Timeframe Context

```palmscript
interval 1d
source spot = binance.spot("BTCUSDT")
use spot 1w

let weekly_basis = ema(spot.1w.close, 8)
export bullish = spot.close > weekly_basis
plot(spot.close)
```

For the exact rules behind `spot.1w.close`, indexing, and no-lookahead behavior, see:

- [Series and Indexing](../reference/series-and-indexing.md)
- [Intervals and Sources](../reference/intervals-and-sources.md)
- [Evaluation Semantics](../reference/evaluation-semantics.md)
