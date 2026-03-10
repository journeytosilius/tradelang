# Cookbook: Exchange-Backed Sources

Use named sources when the strategy should fetch historical candles directly from supported exchanges.

```palmscript
interval 1m

source bn = binance.spot("BTCUSDT")
source hl = hyperliquid.perps("BTC")
use hl 1h

plot(bn.close)
plot(hl.1h.close)
```

## Run It

```bash
palmscript run market strategy.palm \
  --from 1704067200000 \
  --to 1704153600000
```

## What To Watch For

- source-aware scripts must use source-qualified market series
- `use hl 1h` is required before `hl.1h.close`
- the script still has one global base `interval`
- market mode fetches each required `(source, interval)` directly from the venue

Reference:

- [Intervals and Sources](../../reference/intervals-and-sources.md)
