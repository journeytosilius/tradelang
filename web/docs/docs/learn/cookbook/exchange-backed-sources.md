# Cookbook: Exchange-Backed Sources

Use named sources when the strategy should fetch historical candles directly from supported exchanges.

```palmscript
interval 1m

source bn = binance.spot("BTCUSDT")
source bb = bybit.usdt_perps("BTCUSDT")
use bb 1h

plot(bn.close)
plot(bb.1h.close)
```

PalmScript also supports Bybit and Gate source templates:

- `bybit.spot("BTCUSDT")`
- `bybit.usdt_perps("BTCUSDT")`
- `gate.spot("BTC_USDT")`
- `gate.usdt_perps("BTC_USDT")`

Representative checked-in examples:

- `crates/palmscript/examples/strategies/binance_spot_btcusdt_weekly_trend.ps`
- `crates/palmscript/examples/strategies/binance_usdm_auxiliary_fields.ps`
- `crates/palmscript/examples/strategies/bybit_spot.ps`
- `crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps`
- `crates/palmscript/examples/strategies/gate_spot.ps`
- `crates/palmscript/examples/strategies/gate_usdt_perps_backtest.ps`
- `crates/palmscript/examples/strategies/cross_exchange_bybit_gate_spread.ps`

Binance USD-M sources can also expose historical auxiliary series when the script references them:

```palmscript
interval 1h
source perp = binance.usdm("BTCUSDT")
use perp 4h

plot(perp.mark_price - perp.index_price)
plot(nz(perp.funding_rate, 0))
plot(perp.basis)
plot(ema(perp.4h.premium_index, 4))
```

## Try It In The Browser IDE

Open [https://palmscript.dev/](https://palmscript.dev/), paste the example into the editor, and run it against the available BTCUSDT history in the app.

## What To Watch For

- source-aware scripts must use source-qualified market series
- `use bb 1h` is required before `bb.1h.close`
- the script still has one global base `interval`
- the runtime resolves each required `(source, interval)` feed before execution
- `binance.usdm` also supports historical-only `funding_rate`, `mark_price`, `index_price`, `premium_index`, and `basis` source fields in `run market`, `run backtest`, `run walk-forward`, `run walk-forward-sweep`, and `run optimize`
- Bybit expects venue-native symbols like `BTCUSDT`
- Gate expects venue-native symbols like `BTC_USDT`
- `run paper` now bootstraps those Binance USD-M auxiliary fields from the same historical feed path and carries them into armed paper sessions
- `run market`, `run backtest`, `run walk-forward`, `run walk-forward-sweep`, and `run optimize` all resolve the same exchange-backed source declarations

Reference:

- [Intervals and Sources](../../reference/intervals-and-sources.md)
