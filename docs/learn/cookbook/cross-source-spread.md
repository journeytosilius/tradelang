# Cookbook: Cross-Source Spread

This pattern compares two named markets on the same base clock.

```palmscript
interval 1m

source spot = binance.spot("BTCUSDT")
source perp = binance.usdm("BTCUSDT")

let spread = spot.close - perp.close
plot(spread)
```

## Why It Matters

Source-aware execution builds the base clock from the union of declared-source base timestamps.

That means:

- the strategy still executes once per base-interval step
- if one source is missing on a step, that source contributes `na`
- expressions depending on that missing input also propagate `na` through normal semantics

Reference:

- [Evaluation Semantics](../../reference/evaluation-semantics.md)
- [Intervals and Sources](../../reference/intervals-and-sources.md)
