# Indikatoren-Ueberblick

Dieser Abschnitt definiert die ausfuehrbare Indikator-Oberflaeche von
PalmScript.

Verwende [Builtins](builtins.md) fuer gemeinsame Aufrufregeln, Helper-Builtins,
`plot` und Tupel-Destrukturierungsregeln, die fuer die gesamte Sprache gelten.

## Indikator-Familien

PalmScript dokumentiert Indikatoren derzeit in diesen Familien:

- [Trend und Uberlagerung](indicators-trend-and-overlap.md)
- [Momentum, Volumen und Volatilitat](indicators-momentum-volume-volatility.md)
- [Mathematik, Preis und Statistik](indicators-math-price-statistics.md)

## Gemeinsame Indikator-Regeln

Regeln:

- Indikatornamen sind Builtin-Identifikatoren und werden direkt aufgerufen,
  zum Beispiel `ema(spot.close, 20)`
- Indikator-Eingaenge muessen weiterhin die quellqualifizierten Serienregeln
  aus [Intervalle und Quellen](intervals-and-sources.md) einhalten
- optionale Laengenargumente verwenden die auf den Familienseiten
  dokumentierten TA-Lib-Defaults
- laengenartige Argumente, die als Literale beschrieben sind, muessen im
  Quelltext Integer-Literale sein
- tupelwertige Indikatoren muessen mit `let (...) = ...` destrukturiert werden,
  bevor sie weiterverwendet werden
- Indikator-Ausgaben folgen dem Aktualisierungstakt, der durch ihre
  Serien-Eingaenge vorgegeben ist
- Indikatoren propagieren `na`, sofern der spezifische Vertrag nichts anderes
  sagt

## Tupelwertige Indikatoren

Die aktuell tupelwertigen Indikatoren sind:

- `macd(series, fast_length, slow_length, signal_length)`
- `minmax(series[, length=30])`
- `minmaxindex(series[, length=30])`
- `aroon(high, low[, length=14])`
- `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])`
- `donchian(high, low[, length=20])`

Sie muessen sofort destrukturiert werden:

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let (line, signal, hist) = macd(spot.close, 12, 26, 9)
plot(line)
```

## Ausfuehrbare Gegenueber Reservierten TA-Lib-Namen

PalmScript reserviert heute einen groesseren TA-Lib-Katalog, als es wirklich
ausfuehrt.

- diese Indikatorseiten definieren die ausfuehrbare Teilmenge
- [TA-Lib-Oberflache](ta-lib.md) definiert die breitere reservierte Namens- und
  Metadaten-Oberflaeche
- ein reservierter, aber noch nicht ausfuehrbarer TA-Lib-Name erzeugt eine
  deterministische Compile-Diagnose
