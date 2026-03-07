# Cookbook: Multi-Interval Strategy

This pattern adds slower context to a faster or equal base strategy.

```palmscript
interval 1d
source spot = binance.spot("BTCUSDT")
use spot 1w

let weekly_basis = ema(spot.1w.close, 8)

if spot.close > weekly_basis {
    plot(1)
} else {
    plot(0)
}
```

## Run It

```bash
palmscript run market strategy.palm \
  --from 1704067200000 \
  --to 1705276800000
```

## What To Watch For

- `use spot 1w` is required before `spot.1w.close`
- higher-interval values appear only after the higher candle fully closes
- no partial weekly candle is exposed
- indexing composes on the slower interval clock, not the base clock

Reference:

- [Intervals and Sources](../../reference/intervals-and-sources.md)
- [Series and Indexing](../../reference/series-and-indexing.md)
- [Evaluation Semantics](../../reference/evaluation-semantics.md)
