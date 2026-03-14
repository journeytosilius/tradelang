# First Strategy

This strategy runs on one-minute bars, computes two moving averages, and turns that crossover into a simple long-only entry and exit flow.

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")
execution spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 5)
let slow = sma(spot.close, 10)

export trend = fast > slow
entry long = crossover(fast, slow)
exit long = crossunder(fast, slow)

order_template market_order = market()
order entry long = market_order
order exit long = market_order
```

## What This Introduces

- `interval 1m` sets the base execution clock
- `source spot = ...` binds one exchange-backed market
- `execution spot = ...` binds the venue target used by backtest, walk-forward, optimize, and paper execution commands
- `spot.close` is a source-qualified base series
- `let` binds reusable expressions
- `export` emits a named output series
- `entry long = ...` emits a long-entry signal
- `exit long = ...` emits a long-exit signal
- `order_template market_order = market()` declares one reusable order spec
- `order entry long = market_order` and `order exit long = market_order` reuse that explicit order config in every executable mode

## Try It In The Browser IDE

Open [https://palmscript.dev/](https://palmscript.dev/), paste the script into the editor, and run it over the available BTCUSDT history with the date controls in the header. You should see the diagnostics panel stay clean, then the backtest summary, trades, and orders populate from the crossover signals.

## Extend It With Higher-Timeframe Context

```palmscript
interval 1d
source spot = binance.spot("BTCUSDT")
execution spot = binance.spot("BTCUSDT")
use spot 1w

let weekly_basis = ema(spot.1w.close, 8)
export bullish = spot.close > weekly_basis
entry long = bullish and crossover(spot.close, weekly_basis)
exit long = crossunder(spot.close, weekly_basis)
order_template market_order = market()
order entry long = market_order
order exit long = market_order
```

For the exact rules behind `spot.1w.close`, first-class `entry` / `exit` signals, indexing, and no-lookahead behavior, see:

- [Series and Indexing](../reference/series-and-indexing.md)
- [Intervals and Sources](../reference/intervals-and-sources.md)
- [Outputs](../reference/outputs.md)
- [Evaluation Semantics](../reference/evaluation-semantics.md)
