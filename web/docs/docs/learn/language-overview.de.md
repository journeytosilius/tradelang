# Sprachuberblick

PalmScript-Skripte sind Quelltextdateien auf Top-Level-Ebene und bestehen aus
Deklarationen und Anweisungen.

Haufige Bausteine:

- `interval <...>` fur den Basis-Ausfuhrungstakt
- `source`-Deklarationen fur marktgestutzte Serien
- optionale zusatzliche `use <alias> <interval>`-Deklarationen
- Top-Level-Funktionen
- `let`, `const`, `input`, Tupel-Destrukturierung, `export`, `regime`, `trigger`, `entry` / `exit` und `order`
- deklarative Backtest-Kontrollen wie `cooldown long = 12` und `max_bars_in_trade short = 48`
- `if / else if / else`
- Ausdrucke aus Operatoren, Aufrufen und Indexierung
- Helper-Builtins wie `crossover`, `state`, `activated`, `barssince` und `valuewhen`
- typisierte Enum-Literale `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>` und `exit_kind.<variant>`

## Skriptform

Ausfuhrbare PalmScript-Skripte benennen ihre Datenquellen explizit:

```palmscript
interval 1m
source bn = binance.spot("BTCUSDT")
source bb = bybit.usdt_perps("BTCUSDT")

plot(bn.close - bb.close)
```

## Mentales Modell

- jedes Skript hat ein Basisintervall
- ausfuhrbare Skripte deklarieren eine oder mehrere `source`-Bindungen
- Marktserien sind immer quellqualifiziert
- Serienwerte entwickeln sich uber die Zeit
- hohere Intervalle aktualisieren sich nur, wenn diese Kerzen vollstandig schliessen
- fehlender Verlauf oder fehlende ausgerichtete Quelldaten erscheinen als `na`
- `plot`, `export`, `regime`, `trigger` und Strategiedeklarationen emittieren nach jedem Ausfuhrungsschritt Ergebnisse
- `cooldown` und `max_bars_in_trade` sind Compile-Time-Balkenzaehler, die Re-Entry und zeitbasierte Exits explizit machen

## Wohin Fur Exakte Regeln

- Syntax und Tokens: [Lexikalische Struktur](../reference/lexical-structure.md) und [Grammatik](../reference/grammar.md)
- Deklarationen und Sichtbarkeit: [Deklarationen und Gultigkeitsbereich](../reference/declarations-and-scope.md)
- Ausdrucke und Semantik: [Auswertungssemantik](../reference/evaluation-semantics.md)
- Regeln fur Marktserien: [Intervalle und Quellen](../reference/intervals-and-sources.md)
- Indikatoren und Helper-Builtins: [Indikatoren](../reference/indicators.md) und [Builtins](../reference/builtins.md)
- Ausgaben: [Ausgaben](../reference/outputs.md)

## Optimierungsmetadaten

Numerische `input`-Deklarationen koennen jetzt Suchraum-Metadaten direkt im Skript tragen:

```palmscript
input fast_len = 21 optimize(int, 8, 34, 1)
```

Dadurch koennen `run optimize` und `runs submit optimize` den Suchraum aus dem Skript selbst ableiten, wenn kein `--param` uebergeben wird.
