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

## Try It In The Browser IDE

Open [https://palmscript.dev/app/](https://palmscript.dev/app/), paste the example into the editor, and run it against the available BTCUSDT history in the app.

## What To Watch For

- source-aware scripts must use source-qualified market series
- `use hl 1h` is required before `hl.1h.close`
- the script still has one global base `interval`
- the runtime resolves each required `(source, interval)` feed before execution

Reference:

- [Intervals and Sources](../../reference/intervals-and-sources.md)
