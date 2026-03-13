# Erste Strategie

Diese Strategie lauft auf Ein-Minuten-Balken, berechnet zwei gleitende
Durchschnitte und macht aus deren Kreuzung einen einfachen Long-only-Einstiegs-
und Ausstiegsfluss.

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")
execution spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 5)
let slow = sma(spot.close, 10)

export trend = fast > slow
entry long = crossover(fast, slow)
exit long = crossunder(fast, slow)

order entry long = market()
order exit long = market()
```

## Was Das Einfuhrt

- `interval 1m` setzt den Basis-Ausfuhrungstakt
- `source spot = ...` bindet einen exchange-gestutzten Markt
- `execution spot = ...` bindet das Venue-Ziel fur Backtest-, Walk-Forward-, Optimize- und Paper-Befehle
- `spot.close` ist eine quellqualifizierte Basisserie
- `let` bindet wiederverwendbare Ausdrucke
- `export` veroffentlicht eine benannte Ausgabeserie
- `entry long = ...` emittiert ein Long-Einstiegssignal
- `exit long = ...` emittiert ein Long-Ausstiegssignal
- `order entry long = market()` und `order exit long = market()` sagen den Ausfuehrungsmodi, wie Ein- und Ausstiegssignale ausgefuehrt werden

## In Der Browser-IDE Ausprobieren

Offne [https://palmscript.dev/](https://palmscript.dev/), fige das
Skript in den Editor ein und fuhre es mit den Datumssteuerungen im Header uber
den verfugbaren BTCUSDT-Verlauf aus. Du solltest sehen, dass das
Diagnose-Panel sauber bleibt und danach die Backtest-Zusammenfassung, Trades
und Orders aus den Kreuzungssignalen gefullt werden.

## Mit Hoherem Zeitrahmen-Kontext Erweitern

```palmscript
interval 1d
source spot = binance.spot("BTCUSDT")
execution spot = binance.spot("BTCUSDT")
use spot 1w

let weekly_basis = ema(spot.1w.close, 8)
export bullish = spot.close > weekly_basis
entry long = bullish and crossover(spot.close, weekly_basis)
exit long = crossunder(spot.close, weekly_basis)
order entry long = market()
order exit long = market()
```

Die exakten Regeln hinter `spot.1w.close`, erstklassigen `entry` / `exit`-
Signalen, Indexierung und No-Lookahead-Verhalten findest du in:

- [Serien und Indexierung](../reference/series-and-indexing.md)
- [Intervalle und Quellen](../reference/intervals-and-sources.md)
- [Ausgaben](../reference/outputs.md)
- [Auswertungssemantik](../reference/evaluation-semantics.md)
