# Builtins

Diese Seite definiert PalmScripts gemeinsame Builtin-Regeln und die
nicht-indikatorischen Builtin-Helper.

Indikatorspezifische Vertraege befinden sich im eigenen Abschnitt
[Indikatoren](indicators.md).

## Ausfuehrbare Builtins Gegenueber Reservierten Namen

PalmScript exponiert drei verwandte Oberflaechen:

- ausfuehrbare Builtin-Helper und Ausgaben, die auf dieser Seite dokumentiert
  sind
- ausfuehrbare Indikatoren, die im Abschnitt [Indikatoren](indicators.md)
  dokumentiert sind
- einen breiteren reservierten TA-Lib-Katalog, der in
  [TA-Lib-Oberflache](ta-lib.md) beschrieben ist

Nicht jeder reservierte TA-Lib-Name ist heute ausfuehrbar. Reservierte, aber
noch nicht ausfuehrbare Namen erzeugen deterministische Compile-Diagnosen,
anstatt als unbekannte Bezeichner behandelt zu werden.

## Builtin-Kategorien

PalmScript exponiert derzeit diese Builtin-Kategorien:

- Indikatoren: [Trend und Uberlagerung](indicators-trend-and-overlap.md),
  [Momentum, Volumen und Volatilitat](indicators-momentum-volume-volatility.md)
  und [Mathematik, Preis und Statistik](indicators-math-price-statistics.md)
- relationale Helper: `above`, `below`, `between`, `outside`
- Kreuzungs-Helper: `cross`, `crossover`, `crossunder`
- Null-Helper: `na(value)`, `nz(value[, fallback])`,
  `coalesce(value, fallback)`
- Serien- und Fenster-Helper: `change`, `highest`, `lowest`, `highestbars`,
  `lowestbars`, `rising`, `falling`, `cum`
- Event-Memory-Helper: `state`, `activated`, `deactivated`, `barssince`, `valuewhen`,
  `highest_since`, `lowest_since`, `highestbars_since`, `lowestbars_since`,
  `valuewhen_since`, `count_since`
- Ausgaben: `plot`

Marktfelder werden ueber quellqualifizierte Serien wie `spot.open`,
`spot.close` oder `bb.1h.volume` ausgewaehlt. Nur Identifikatoren sind
aufrufbar, daher wird `spot.close()` abgelehnt.

## Tupelwertige Builtins

Die aktuell ausfuehrbaren tupelwertigen Builtins sind:

- `macd(series, fast_length, slow_length, signal_length)` dokumentiert unter
  [Trend und Uberlagerung](indicators-trend-and-overlap.md)
- `minmax(series[, length=30])` dokumentiert unter
  [Mathematik, Preis und Statistik](indicators-math-price-statistics.md)
- `minmaxindex(series[, length=30])` dokumentiert unter
  [Mathematik, Preis und Statistik](indicators-math-price-statistics.md)
- `aroon(high, low[, length=14])` dokumentiert unter
  [Momentum, Volumen und Volatilitat](indicators-momentum-volume-volatility.md)

Alle tupelwertigen Builtin-Ergebnisse muessen unmittelbar mit `let (...) = ...`
destrukturiert werden, bevor sie weiterverwendet werden.

## Gemeinsame Builtin-Regeln

Regeln:

- alle Builtins sind deterministisch
- Builtins duerfen weder I/O ausfuehren noch auf Zeit oder Netzwerk zugreifen
- `plot` schreibt in den Ausgabestrom; alle anderen Builtins sind rein
- Builtin-Helper und Indikatoren propagieren `na`, sofern keine spezifischere
  Regel dieses Verhalten ueberschreibt
- Builtin-Ergebnisse folgen den Aktualisierungstakten, die durch ihre
  Serienargumente impliziert werden

## Relationale Helper

### `above(a, b)` und `below(a, b)`

Regeln:

- beide Argumente muessen numerisch, `series<float>` oder `na` sein
- `above(a, b)` wird als `a > b` ausgewertet
- `below(a, b)` wird als `a < b` ausgewertet
- wenn irgendein benoetigter Eingang `na` ist, ist das Ergebnis `na`
- wenn einer der Eingaenge eine Serie ist, ist der Ergebnistyp `series<bool>`
- andernfalls ist der Ergebnistyp `bool`

### `between(x, low, high)` und `outside(x, low, high)`

Regeln:

- alle Argumente muessen numerisch, `series<float>` oder `na` sein
- `between(x, low, high)` wird als `low < x and x < high` ausgewertet
- `outside(x, low, high)` wird als `x < low or x > high` ausgewertet
- wenn irgendein benoetigter Eingang `na` ist, ist das Ergebnis `na`
- wenn irgendein Argument eine Serie ist, ist der Ergebnistyp `series<bool>`
- andernfalls ist der Ergebnistyp `bool`

## Kreuzungs-Helper

### `crossover(a, b)`

Regeln:

- beide Argumente muessen numerisch, `series<float>` oder `na` sein
- mindestens ein Argument muss `series<float>` sein
- skalare Argumente werden als Schwellwerte behandelt, daher ist ihr vorheriges
  Sample ihr aktueller Wert
- ausgewertet wird als aktuelles `a > b` und vorheriges `a[1] <= b[1]`
- wenn irgendein benoetigtes aktuelles oder vorheriges Sample `na` ist, ist
  das Ergebnis `na`
- der Ergebnistyp ist `series<bool>`

### `crossunder(a, b)`

Regeln:

- beide Argumente muessen numerisch, `series<float>` oder `na` sein
- mindestens ein Argument muss `series<float>` sein
- skalare Argumente werden als Schwellwerte behandelt, daher ist ihr vorheriges
  Sample ihr aktueller Wert
- ausgewertet wird als aktuelles `a < b` und vorheriges `a[1] >= b[1]`
- wenn irgendein benoetigtes aktuelles oder vorheriges Sample `na` ist, ist
  das Ergebnis `na`
- der Ergebnistyp ist `series<bool>`

### `cross(a, b)`

Regeln:

- beide Argumente folgen demselben Vertrag wie `crossover` und `crossunder`
- ausgewertet wird als `crossover(a, b) or crossunder(a, b)`
- wenn irgendein benoetigtes aktuelles oder vorheriges Sample `na` ist, ist
  das Ergebnis `na`
- der Ergebnistyp ist `series<bool>`

## Serien- Und Fenster-Helper

### `change(series, length)`

Regeln:

- es erfordert genau zwei Argumente
- das erste Argument muss `series<float>` sein
- das zweite Argument muss ein positives Integer-Literal sein
- ausgewertet wird als `series - series[length]`
- wenn das aktuelle oder referenzierte Sample `na` ist, ist das Ergebnis `na`
- der Ergebnistyp ist `series<float>`

### `highest(series, length)` und `lowest(series, length)`

Regeln:

- das erste Argument muss `series<float>` sein
- das zweite Argument muss ein positives Integer-Literal sein
- das Fenster enthaelt das aktuelle Sample
- wenn nicht genug Historie vorhanden ist, ist das Ergebnis `na`
- wenn irgendein Sample im benoetigten Fenster `na` ist, ist das Ergebnis `na`
- der Ergebnistyp ist `series<float>`

Das Argument `length` darf ein positives Integer-Literal oder eine
unveraenderliche numerische Top-Level-Bindung sein, die mit `const` oder
`input` deklariert wurde.

### `highestbars(series, length)` und `lowestbars(series, length)`

Regeln:

- das erste Argument muss `series<float>` sein
- das zweite Argument folgt derselben Positiv-Integer-Regel wie
  `highest` / `lowest`
- das Fenster enthaelt das aktuelle Sample
- das Ergebnis ist die Anzahl der Bars seit dem hoechsten oder niedrigsten
  Sample im aktiven Fenster
- wenn nicht genug Historie vorhanden ist, ist das Ergebnis `na`
- wenn irgendein Sample im benoetigten Fenster `na` ist, ist das Ergebnis `na`
- der Ergebnistyp ist `series<float>`

### `rising(series, length)` und `falling(series, length)`

Regeln:

- das erste Argument muss `series<float>` sein
- das zweite Argument muss ein positives Integer-Literal sein
- `rising(series, length)` bedeutet, dass das aktuelle Sample strikt groesser
  ist als jedes fruehere Sample in den letzten `length` Bars
- `falling(series, length)` bedeutet, dass das aktuelle Sample strikt kleiner
  ist als jedes fruehere Sample in den letzten `length` Bars
- wenn nicht genug Historie vorhanden ist, ist das Ergebnis `na`
- wenn irgendein benoetigtes Sample `na` ist, ist das Ergebnis `na`
- der Ergebnistyp ist `series<bool>`

### `cum(value)`

Regeln:

- es erfordert genau ein numerisches oder `series<float>`-Argument
- es liefert die kumulative laufende Summe auf dem Aktualisierungstakt des
  Arguments
- wenn das aktuelle Eingabe-Sample `na` ist, ist das aktuelle Ausgabe-Sample
  `na`
- spaetere nicht-`na`-Samples akkumulieren weiter vom vorherigen Laufwert aus
- der Ergebnistyp ist `series<float>`

## Null-Helper

### `na(value)`

Regeln:

- es erfordert genau ein Argument
- es liefert `true`, wenn das aktuelle Argument-Sample `na` ist
- es liefert `false`, wenn das aktuelle Argument-Sample ein konkreter skalarer
  Wert ist
- wenn das Argument seriengestuetzt ist, ist der Ergebnistyp `series<bool>`
- andernfalls ist der Ergebnistyp `bool`

### `nz(value[, fallback])`

Regeln:

- es akzeptiert ein oder zwei Argumente
- mit einem Argument verwenden numerische Eingaenge `0` und boolesche
  Eingaenge `false` als Fallback
- mit zwei Argumenten wird das zweite Argument zurueckgegeben, wenn das erste
  `na` ist
- beide Argumente muessen typkompatible numerische oder boolesche Werte sein
- der Ergebnistyp folgt dem angehobenen Typ der Operanden

### `coalesce(value, fallback)`

Regeln:

- es erfordert genau zwei Argumente
- es liefert das erste Argument, wenn dieses nicht `na` ist
- andernfalls liefert es das zweite Argument
- beide Argumente muessen typkompatible numerische oder boolesche Werte sein
- der Ergebnistyp folgt dem angehobenen Typ der Operanden

## Event-Memory-Helper

### `activated(condition)` und `deactivated(condition)`

Regeln:

- beide erfordern genau ein Argument
- das Argument muss `series<bool>` sein
- `activated` liefert `true`, wenn das aktuelle Condition-Sample `true` ist und
  das vorherige Sample `false` oder `na` war
- `deactivated` liefert `true`, wenn das aktuelle Condition-Sample `false` ist
  und das vorherige Sample `true` war
- wenn das aktuelle Sample `na` ist, liefern beide Helper `false`
- der Ergebnistyp ist `series<bool>`

### `state(enter, exit)`

Regeln:

- es erfordert genau zwei Argumente
- beide Argumente muessen `series<bool>` sein
- es liefert einen persistenten `series<bool>`-Zustand, der mit `false` beginnt
- `enter = true` bei `exit = false` schaltet den Zustand ein
- `exit = true` bei `enter = false` schaltet den Zustand aus
- wenn beide Argumente auf derselben Bar `true` sind, bleibt der vorherige Zustand erhalten
- wenn irgendein aktuelles Eingabe-Sample `na` ist, wird dieser Eingang auf der aktuellen Bar als inaktiver Uebergang behandelt
- der Ergebnistyp ist `series<bool>`

Dies ist die vorgesehene Grundlage fuer erstklassige `regime`-Deklarationen:

```palmscript
regime trend_long = state(close > ema(close, 20), close < ema(close, 20))
export trend_started = activated(trend_long)
```

### `barssince(condition)`

Regeln:

- es erfordert genau ein Argument
- das Argument muss `series<bool>` sein
- es liefert `0` auf Bars, auf denen das aktuelle Condition-Sample `true` ist
- es wird bei jedem Update des eigenen Takts der Bedingung nach dem letzten
  wahren Ereignis inkrementiert
- es liefert `na` bis zum ersten wahren Ereignis
- wenn das aktuelle Condition-Sample `na` ist, ist auch die aktuelle Ausgabe
  `na`
- der Ergebnistyp ist `series<float>`

### `valuewhen(condition, source, occurrence)`

Regeln:

- es erfordert genau drei Argumente
- das erste Argument muss `series<bool>` sein
- das zweite Argument muss `series<float>` oder `series<bool>` sein
- das dritte Argument muss ein nicht-negatives Integer-Literal sein
- Auftreten `0` bedeutet das juengste wahre Ereignis
- der Ergebnistyp entspricht dem Typ des zweiten Arguments
- es liefert `na`, bis genug passende wahre Ereignisse vorhanden sind
- wenn das aktuelle Condition-Sample `na` ist, ist die aktuelle Ausgabe `na`
- wenn das aktuelle Condition-Sample `true` ist, wird das aktuelle `source`-
  Sample fuer spaetere Auftreten gespeichert

### `highest_since(anchor, source)` und `lowest_since(anchor, source)`

Regeln:

- beide erfordern genau zwei Argumente
- das erste Argument muss `series<bool>` sein
- das zweite Argument muss `series<float>` sein
- wenn das aktuelle Anchor-Sample `true` ist, beginnt eine neue verankerte
  Epoche auf der aktuellen Bar
- die aktuelle Bar zaehlt sofort zur neuen Epoche
- vor dem ersten Anchor ist das Ergebnis `na`
- spaetere wahre Anchors verwerfen die vorherige verankerte Epoche und starten
  eine neue
- der Ergebnistyp ist `series<float>`

### `highestbars_since(anchor, source)` und `lowestbars_since(anchor, source)`

Regeln:

- beide erfordern genau zwei Argumente
- das erste Argument muss `series<bool>` sein
- das zweite Argument muss `series<float>` sein
- sie folgen denselben Reset-Regeln verankerter Epochen wie
  `highest_since` / `lowest_since`
- das Ergebnis ist die Anzahl der Bars seit dem hoechsten oder niedrigsten
  Sample innerhalb der aktuellen verankerten Epoche
- vor dem ersten Anchor ist das Ergebnis `na`
- der Ergebnistyp ist `series<float>`

### `valuewhen_since(anchor, condition, source, occurrence)`

Regeln:

- es erfordert genau vier Argumente
- das erste und zweite Argument muessen `series<bool>` sein
- das dritte Argument muss `series<float>` oder `series<bool>` sein
- das vierte Argument muss ein nicht-negatives Integer-Literal sein
- wenn das aktuelle Anchor-Sample `true` ist, werden fruehere
  `condition`-Treffer vergessen und eine neue verankerte Epoche beginnt auf der
  aktuellen Bar
- Auftreten `0` bedeutet das juengste passende Ereignis innerhalb der aktuellen
  verankerten Epoche
- vor dem ersten Anchor ist das Ergebnis `na`
- der Ergebnistyp entspricht dem Typ des dritten Arguments

### `count_since(anchor, condition)`

Regeln:

- es erfordert genau zwei Argumente
- beide Argumente muessen `series<bool>` sein
- wenn das aktuelle Anchor-Sample `true` ist, wird der laufende Zaehlwert
  zurueckgesetzt und eine neue verankerte Epoche beginnt auf der aktuellen Bar
- die aktuelle Bar zaehlt sofort zur neuen verankerten Epoche
- der Zaehler wird nur auf Bars erhoeht, auf denen das aktuelle
  `condition`-Sample `true` ist
- vor dem ersten Anchor ist das Ergebnis `na`
- spaetere wahre Anchors verwerfen die vorherige verankerte Epoche und starten
  eine neue
- der Ergebnistyp ist `series<float>`

## `plot(value)`

`plot` emittiert einen Plot-Punkt fuer den aktuellen Schritt.

Regeln:

- es erfordert genau ein Argument
- das Argument muss numerisch, `series<float>` oder `na` sein
- der Ausdrucksergebnistyp ist `void`
- `plot` darf nicht innerhalb eines benutzerdefinierten Funktionskoerpers
  aufgerufen werden

Zur Laufzeit:

- numerische Werte werden als Plot-Punkte aufgezeichnet
- `na` zeichnet einen Plot-Punkt ohne numerischen Wert auf

## Aktualisierungstakte

Builtin-Ergebnisse folgen den Aktualisierungstakten ihrer Eingaenge.

Beispiele:

- `ema(spot.close, 20)` schreitet auf dem Basis-Takt fort
- `highest(spot.1w.close, 5)` schreitet auf dem Wochen-Takt fort
- `cum(spot.1w.close - spot.1w.close[1])` schreitet auf dem Wochen-Takt fort
- `crossover(bb.close, bn.close)` schreitet fort, wenn eine der referenzierten
  Quellserien fortschreitet
- `activated(trend_long)` schreitet auf dem Takt von `trend_long` fort
- `barssince(spot.close > spot.close[1])` schreitet auf dem Takt dieser
  Bedingungsserie fort
- `valuewhen(trigger_series, bb.1h.close, 0)` schreitet auf dem Takt von
  `trigger_series` fort
- `highest_since(position_event.long_entry_fill, spot.high)` schreitet auf dem
  gemeinsam genutzten Takt von Anchor- und Quellserie fort
