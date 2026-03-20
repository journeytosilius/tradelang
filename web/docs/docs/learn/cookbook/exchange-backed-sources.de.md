# Rezept: Quellen Mit Exchange-Daten

Verwende benannte Quellen, wenn die Strategie historische Kerzen direkt von
unterstuetzten Exchanges laden soll.

```palmscript
interval 1m

source bn = binance.spot("BTCUSDT")
source bb = bybit.usdt_perps("BTCUSDT")
use bb 1h

plot(bn.close)
plot(bb.1h.close)
```

PalmScript unterstuetzt ausserdem Bybit- und Gate-Quell-Templates:

- `bybit.spot("BTCUSDT")`
- `bybit.usdt_perps("BTCUSDT")`
- `gate.spot("BTC_USDT")`
- `gate.usdt_perps("BTC_USDT")`

Relevante mit eingecheckte Beispiele:

- `crates/palmscript/examples/strategies/binance_spot_btcusdt_weekly_trend.ps`
- `crates/palmscript/examples/strategies/binance_usdm_auxiliary_fields.ps`
- `crates/palmscript/examples/strategies/bybit_spot.ps`
- `crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps`
- `crates/palmscript/examples/strategies/gate_spot.ps`
- `crates/palmscript/examples/strategies/gate_usdt_perps_backtest.ps`
- `crates/palmscript/examples/strategies/cross_exchange_bybit_gate_spread.ps`

## Probiere Es In Der Browser-IDE

Oeffne [https://palmscript.dev/](https://palmscript.dev/), fuege das
Beispiel in den Editor ein und fuehre es mit der verfuegbaren BTCUSDT-Historie
in der App aus.

## Worauf Du Achten Solltest

- quellenbewusste Skripte muessen quellqualifizierte Marktserien verwenden
- `use bb 1h` ist erforderlich, bevor `bb.1h.close` gueltig ist
- das Skript hat weiterhin genau ein globales Basis-`interval`
- der Runtime loest jeden benoetigten `(source, interval)`-Feed vor der
  Ausfuehrung auf
- `binance.usdm` unterstuetzt ausserdem die historischen Felder
  `funding_rate`, `mark_price`, `index_price`,
  `premium_index` und `basis`
- Bybit erwartet venue-native Symbole wie `BTCUSDT`
- Gate erwartet venue-native Symbole wie `BTC_USDT`
- `run paper` bootstrappt diese Binance-USD-M-Hilfsfelder jetzt ueber denselben
  historischen Feed-Pfad und uebernimmt sie in bewaffnete Paper-Sessions
- `run market`, `run backtest`, `run walk-forward`, `run walk-forward-sweep`
  und `run optimize` loesen dieselben exchangegestuetzten Quell-Deklarationen
  auf

Referenz:

- [Intervalle und Quellen](../../reference/intervals-and-sources.md)
