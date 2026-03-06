# First Strategy

This minimal strategy runs on 1-minute bars and exports a trend state while plotting the closing price.

```palmscript
interval 1m

let fast = ema(close, 5)
let slow = sma(close, 10)

export trend = fast > slow

if trend {
    plot(close)
} else {
    plot(na)
}
```

## What It Shows

- `interval 1m` binds unqualified market series like `close` to 1-minute bars
- `let` binds reusable expressions
- `export` publishes a named output series in the runtime outputs
- `plot` emits chart-oriented output

## Run It

```bash
target/debug/palmscript run csv examples/strategies/sma_cross.palm \
  --bars examples/data/minute_bars.csv
```

## Extend It

To add higher-interval context:

```palmscript
interval 1d
use 1w

let weekly_basis = ema(1w.close, 8)
export bullish = close > weekly_basis
plot(close)
```

See [Intervals and Multi-Interval Semantics](../language/intervals.md) for the rules that govern `1w.close`.
