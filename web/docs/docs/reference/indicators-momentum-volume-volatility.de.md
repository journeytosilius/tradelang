# Momentum-, Volumen- Und Volatilitaetsindikatoren

Diese Seite definiert PalmScripts ausfuehrbare Momentum-, Oszillator-,
Volumen- und Volatilitaetsindikatoren.

## `rsi(series, length)`

Regeln:

- erfordert genau zwei Argumente
- das erste Argument muss `series<float>` sein
- das zweite Argument muss ein positives Integer-Literal sein
- der Ergebnistyp ist `series<float>`
- die Serie liefert `na`, bis genug Historie vorhanden ist, um den
  Indikatorzustand zu initialisieren

## `roc(series[, length=10])`, `mom(series[, length=10])`, `rocp(series[, length=10])`, `rocr(series[, length=10])` und `rocr100(series[, length=10])`

Regeln:

- das erste Argument muss `series<float>` sein
- das optionale `length` muss ein positives Integer-Literal sein
- ausgelassenes `length` verwendet den TA-Lib-Default `10`
- `roc` wird als `((series - series[length]) / series[length]) * 100`
  ausgewertet
- `mom` wird als `series - series[length]` ausgewertet
- `rocp` wird als `(series - series[length]) / series[length]` ausgewertet
- `rocr` wird als `series / series[length]` ausgewertet
- `rocr100` wird als `(series / series[length]) * 100` ausgewertet
- wenn das aktuelle oder referenzierte Sample `na` ist, ist das Ergebnis `na`
- wenn `series[length]` den Wert `0` hat, liefern `roc`, `rocp`, `rocr` und
  `rocr100` den Wert `na`

## `cmo(series[, length=14])`

Regeln:

- das erste Argument muss `series<float>` sein
- ausgelassenes `length` verwendet den TA-Lib-Default `14`
- wenn angegeben, muss `length` ein Integer-Literal groesser oder gleich `2`
  sein
- `cmo` verwendet den Wilder-artig geglaetteten Gewinn-/Verlustzustand
- der Ergebnistyp ist `series<float>`
- wenn die Summe aus geglaetteten Gewinnen und Verlusten `0` ist, liefert
  `cmo` den Wert `0`

## `cci(high, low, close[, length=14])`

Regeln:

- die ersten drei Argumente muessen `series<float>` sein
- ausgelassenes `length` verwendet den TA-Lib-Default `14`
- wenn angegeben, muss `length` ein Integer-Literal groesser oder gleich `2`
  sein
- `cci` verwendet den gleitenden Typical-Price-Durchschnitt und die mittlere
  Abweichung ueber das angeforderte Fenster
- wenn die aktuelle Typical-Price-Differenz oder die mittlere Abweichung `0`
  ist, liefert `cci` den Wert `0`
- der Ergebnistyp ist `series<float>`

## `aroon(high, low[, length=14])` und `aroonosc(high, low[, length=14])`

Regeln:

- die ersten beiden Argumente muessen `series<float>` sein
- ausgelassenes `length` verwendet den TA-Lib-Default `14`
- wenn angegeben, muss `length` ein Integer-Literal groesser oder gleich `2`
  sein
- `aroon` verwendet ein `length + 1`-Fenster fuer Hoch/Tief, um dem
  TA-Lib-Lookback zu entsprechen
- `aroon` liefert ein 2-Tupel `(aroon_down, aroon_up)` in
  TA-Lib-Ausgabereihenfolge
- `aroonosc` liefert `aroon_up - aroon_down`
- tupelwertige Ausgaben muessen vor weiterer Verwendung destrukturiert werden

## `plus_dm(high, low[, length=14])`, `minus_dm(high, low[, length=14])`, `plus_di(high, low, close[, length=14])`, `minus_di(high, low, close[, length=14])`, `dx(high, low, close[, length=14])`, `adx(high, low, close[, length=14])` und `adxr(high, low, close[, length=14])`

Regeln:

- alle Preisargumente muessen `series<float>` sein
- ausgelassenes `length` verwendet den TA-Lib-Default `14`
- wenn angegeben, muss `length` ein positives Integer-Literal sein
- `plus_dm` und `minus_dm` liefern Wilder-geglaettete Richtungsbewegung
- `plus_di` und `minus_di` liefern Wilder-Richtungsindikatoren
- `dx` liefert den absoluten Richtungsabstand skaliert mit 100
- `adx` liefert den Wilder-Durchschnitt von `dx`
- `adxr` liefert den Mittelwert aus aktuellem `adx` und verzögertem `adx`
- ist auf dem aktiven Balken ein erforderliches Preisargument `na`, ist das Ergebnis fuer diesen Balken `na`
- der Ergebnistyp ist `series<float>`

## `atr(high, low, close[, length=14])` und `natr(high, low, close[, length=14])`

Regeln:

- alle Argumente muessen `series<float>` sein
- ausgelassenes `length` verwendet den TA-Lib-Default `14`
- wenn angegeben, muss `length` ein positives Integer-Literal sein
- `atr` wird aus dem initialen Average True Range geseedet und anschliessend
  mit Wilder-Smoothing fortgefuehrt
- `natr` liefert `(atr / close) * 100`
- ist auf dem aktiven Balken ein erforderliches Preisargument `na`, ist das Ergebnis fuer diesen Balken `na`
- der Ergebnistyp ist `series<float>`

## `willr(high, low, close[, length=14])`

Regeln:

- die ersten drei Argumente muessen `series<float>` sein
- ausgelassenes `length` verwendet den TA-Lib-Default `14`
- wenn angegeben, muss `length` ein Integer-Literal groesser oder gleich `2`
  sein
- `willr` verwendet das hoechste Hoch und das niedrigste Tief ueber das
  angeforderte Fenster
- der Ergebnistyp ist `series<float>`
- wenn die Hoch-Tief-Spanne des Fensters `0` ist, liefert `willr` den Wert `0`

## `mfi(high, low, close, volume[, length=14])` und `imi(open, close[, length=14])`

Regeln:

- alle Argumente muessen `series<float>` sein
- ausgelassenes `length` verwendet den TA-Lib-Default `14`
- wenn angegeben, muss `length` ein positives Integer-Literal sein
- `mfi` verwendet Typical Price und Money Flow ueber ein gleitendes Fenster
- `imi` verwendet die intraday Open-Close-Bewegung ueber das angeforderte
  Fenster
- der Ergebnistyp ist `series<float>`

## `stoch(high, low, close[, fast_k=5[, slow_k=3[, slow_k_ma=ma_type.sma[, slow_d=3[, slow_d_ma=ma_type.sma]]]]])`, `stochf(high, low, close[, fast_k=5[, fast_d=3[, fast_d_ma=ma_type.sma]]])` und `stochrsi(series[, time_period=14[, fast_k=5[, fast_d=3[, fast_d_ma=ma_type.sma]]]])`

Regeln:

- alle Preis- oder Quellargumente muessen `series<float>` sein
- ausgelassene Perioden verwenden die TA-Lib-Defaults
- `fast_k`, `slow_k` und `fast_d`/`slow_d`-Laengen muessen positive
  Integer-Literale sein
- `time_period` fuer `stochrsi` muss ein Integer-Literal groesser oder gleich
  `2` sein
- alle MA-Argumente muessen typisierte `ma_type.<variant>`-Werte sein
- `stoch` liefert `(slowk, slowd)` in TA-Lib-Reihenfolge
- `stochf` liefert `(fastk, fastd)` in TA-Lib-Reihenfolge
- `stochrsi` liefert `(fastk, fastd)` in TA-Lib-Reihenfolge
- tupelwertige Ausgaben muessen vor weiterer Verwendung destrukturiert werden

## `ad(high, low, close, volume)`, `adosc(high, low, close, volume[, fast_length=3[, slow_length=10]])` und `obv(series, volume)`

Regeln:

- alle Argumente muessen `series<float>` sein
- `ad` liefert die kumulative Accumulation/Distribution-Linie
- `adosc` liefert die Differenz zwischen schneller und langsamer EMA der
  Accumulation/Distribution-Linie
- ausgelassenes `fast_length` und `slow_length` verwenden die TA-Lib-Defaults
  `3` und `10`
- `obv` wird mit dem aktuellen `volume` initialisiert und addiert oder
  subtrahiert danach Volumen anhand der Kursrichtung
- wenn ein benoetigtes Preis- oder Volumen-Sample `na` ist, ist das Ergebnis
  `na`
- der Ergebnistyp ist `series<float>`

## `trange(high, low, close)`

Regeln:

- alle Argumente muessen `series<float>` sein
- das erste Ausgabesample ist `na`
- spaetere Samples verwenden die True-Range-Semantik von TA-Lib auf Basis von
  aktuellem `high`, aktuellem `low` und vorherigem `close`
- wenn irgendein benoetigtes Sample `na` ist, ist das Ergebnis `na`
- der Ergebnistyp ist `series<float>`

## `anchored_vwap(anchor, price, volume)`

Rules:

- `anchor` must be `series<bool>`
- `price` and `volume` must be `series<float>`
- when the current `anchor` sample is `true`, the running VWAP resets on that same bar
- the anchor bar is included in the new anchored accumulation window
- if the current anchor, price, or volume sample is `na`, the current output sample is `na`
- if cumulative anchored volume is `0`, the current output sample is `na`
- the result type is `series<float>`
