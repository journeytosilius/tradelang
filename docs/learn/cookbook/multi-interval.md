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

## Try It In The Browser IDE

Open [https://palmscript.dev/app/](https://palmscript.dev/app/), paste the example into the editor, and run it over a date range that covers multiple weekly closes.

## What To Watch For

- `use spot 1w` is required before `spot.1w.close`
- higher-interval values appear only after the higher candle fully closes
- no partial weekly candle is exposed
- indexing composes on the slower interval clock, not the base clock

Reference:

- [Intervals and Sources](../../reference/intervals-and-sources.md)
- [Series and Indexing](../../reference/series-and-indexing.md)
- [Evaluation Semantics](../../reference/evaluation-semantics.md)
