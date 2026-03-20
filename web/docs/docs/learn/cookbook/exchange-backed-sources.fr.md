# Cookbook: Sources Adossees A Un Exchange

Utilisez des sources nommees lorsque la strategie doit recuperer directement
des chandelles historiques depuis des exchanges pris en charge.

```palmscript
interval 1m

source bn = binance.spot("BTCUSDT")
source bb = bybit.usdt_perps("BTCUSDT")
use bb 1h

plot(bn.close)
plot(bb.1h.close)
```

PalmScript prend aussi en charge les templates de source Bybit et Gate :

- `bybit.spot("BTCUSDT")`
- `bybit.usdt_perps("BTCUSDT")`
- `gate.spot("BTC_USDT")`
- `gate.usdt_perps("BTC_USDT")`

Exemples representatifs inclus dans le depot :

- `crates/palmscript/examples/strategies/binance_spot_btcusdt_weekly_trend.ps`
- `crates/palmscript/examples/strategies/binance_usdm_auxiliary_fields.ps`
- `crates/palmscript/examples/strategies/bybit_spot.ps`
- `crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps`
- `crates/palmscript/examples/strategies/gate_spot.ps`
- `crates/palmscript/examples/strategies/gate_usdt_perps_backtest.ps`
- `crates/palmscript/examples/strategies/cross_exchange_bybit_gate_spread.ps`

## L'Essayer Dans L'IDE Navigateur

Ouvrez [https://palmscript.dev/](https://palmscript.dev/), collez
l'exemple dans l'editeur, puis executez-le sur l'historique BTCUSDT
disponible dans l'application.

## Points A Surveiller

- les scripts source-aware doivent utiliser des series de marche qualifiees par
  source
- `use bb 1h` est requis avant `bb.1h.close`
- le script conserve un seul `interval` de base global
- le runtime resout chaque flux `(source, interval)` requis avant l'execution
- `binance.usdm` prend aussi en charge les champs historiques `funding_rate`,
  `mark_price`, `index_price`, `premium_index` et `basis`
- Bybit attend des symboles natifs de venue comme `BTCUSDT`
- Gate attend des symboles natifs de venue comme `BTC_USDT`
- `run paper` initialise maintenant ces champs auxiliaires Binance USD-M via le
  meme chemin historique et les conserve dans les sessions paper armees
- `run market`, `run backtest`, `run walk-forward`, `run walk-forward-sweep`
  et `run optimize` resolvent tous les memes declarations de source adossees a
  un exchange

Reference :

- [Intervalles et sources](../../reference/intervals-and-sources.md)
