# Trend- Und Uberlagerungsindikatoren

Diese Seite definiert PalmScripts ausfuehrbare Trend- und
Uberlagerungsindikatoren.

## `sma(series, length)`

Regeln:

- erfordert genau zwei Argumente
- das erste Argument muss `series<float>` sein
- das zweite Argument muss ein positives Integer-Literal sein
- der Ergebnistyp ist `series<float>`
- wenn nicht genug Historie vorhanden ist, ist das aktuelle Sample `na`
- wenn das benoetigte Fenster `na` enthaelt, ist das aktuelle Sample `na`

## `ema(series, length)`

Regeln:

- erfordert genau zwei Argumente
- das erste Argument muss `series<float>` sein
- das zweite Argument muss ein positives Integer-Literal sein
- der Ergebnistyp ist `series<float>`
- die Serie liefert `na`, bis das Seed-Fenster verfuegbar ist

## `ma(series, length, ma_type)`

Regeln:

- erfordert genau drei Argumente
- das erste Argument muss `series<float>` sein
- das zweite Argument muss ein positives Integer-Literal sein
- das dritte Argument muss ein typisierter `ma_type.<variant>`-Wert sein
- der Ergebnistyp ist `series<float>`
- alle `ma_type`-Varianten sind implementiert
- `ma_type.mama` folgt dem Upstream-TA-Lib-Verhalten und ignoriert den
  expliziten `length`-Parameter; stattdessen werden die MAMA-Defaults
  `fast_limit=0.5` und `slow_limit=0.05` verwendet

## `apo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])` und `ppo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`

Regeln:

- das erste Argument muss `series<float>` sein
- `fast_length` und `slow_length` verwenden standardmaessig `12` und `26`
- wenn angegeben, muessen `fast_length` und `slow_length` Integer-Literale
  groesser oder gleich `2` sein
- wenn angegeben, muss das vierte Argument ein typisierter
  `ma_type.<variant>`-Wert sein
- ausgelassenes `ma_type` verwendet standardmaessig `ma_type.sma`
- `apo` liefert `fast_ma - slow_ma`
- `ppo` liefert `((fast_ma - slow_ma) / slow_ma) * 100`
- wenn der langsame gleitende Durchschnitt `0` ist, liefert `ppo` den Wert `0`
- es werden dieselben ausfuehrbaren `ma_type`-Varianten wie bei `ma(...)`
  unterstuetzt
- der Ergebnistyp ist `series<float>`

## `macd(series, fast_length, slow_length, signal_length)`

Regeln:

- erfordert genau vier Argumente
- das erste Argument muss `series<float>` sein
- die restlichen Argumente muessen positive Integer-Literale sein
- der Ergebnistyp ist ein 3-Tupel aus Serienwerten in TA-Lib-Reihenfolge:
  `(macd_line, signal, histogram)`
- das Ergebnis muss destrukturiert werden, bevor es in `plot`, `export`,
  Bedingungen oder weiteren Ausdruecken verwendet werden kann

## `macdfix(series[, signal_length=9])`

Regeln:

- das erste Argument muss `series<float>` sein
- das optionale `signal_length` verwendet standardmaessig `9`
- wenn angegeben, muss `signal_length` ein positives Integer-Literal sein
- der Ergebnistyp ist ein 3-Tupel aus Serienwerten in TA-Lib-Reihenfolge:
  `(macd_line, signal, histogram)`
- das Ergebnis muss destrukturiert werden, bevor es in `plot`, `export`,
  Bedingungen oder weiteren Ausdruecken verwendet werden kann

## `macdext(series[, fast_length=12[, fast_ma=ma_type.sma[, slow_length=26[, slow_ma=ma_type.sma[, signal_length=9[, signal_ma=ma_type.sma]]]]]])`

Regeln:

- das erste Argument muss `series<float>` sein
- ausgelassene Laengen verwenden die TA-Lib-Defaults `12`, `26` und `9`
- `fast_length` und `slow_length` muessen Integer-Literale groesser oder gleich
  `2` sein
- `signal_length` muss ein Integer-Literal groesser oder gleich `1` sein
- jedes MA-Argument muss ein typisierter `ma_type.<variant>`-Wert sein
- es werden dieselben ausfuehrbaren `ma_type`-Varianten wie bei `ma(...)`
  unterstuetzt
- der Ergebnistyp ist ein 3-Tupel aus Serienwerten in TA-Lib-Reihenfolge:
  `(macd_line, signal, histogram)`
- das Ergebnis muss vor weiterer Verwendung destrukturiert werden

## `bbands(series[, length=5[, deviations_up=2.0[, deviations_down=2.0[, ma_type=ma_type.sma]]]])`

Regeln:

- das erste Argument muss `series<float>` sein
- das optionale `length` verwendet standardmaessig `5`
- wenn angegeben, muss `length` ein positives Integer-Literal sein
- wenn angegeben, muessen `deviations_up` und `deviations_down` numerische
  Skalare sein
- wenn angegeben, muss das fuenfte Argument ein typisierter
  `ma_type.<variant>`-Wert sein
- der Ergebnistyp ist ein 3-Tupel aus Serienwerten in TA-Lib-Reihenfolge:
  `(upper, middle, lower)`
- das Ergebnis muss destrukturiert werden, bevor es in `plot`, `export`,
  Bedingungen oder weiteren Ausdruecken verwendet werden kann

## `accbands(high, low, close[, length=20])`

Regeln:

- die ersten drei Argumente muessen `series<float>` sein
- ausgelassenes `length` verwendet den TA-Lib-Default `20`
- wenn angegeben, muss `length` ein Integer-Literal groesser oder gleich `2`
  sein
- der Ergebnistyp ist ein 3-Tupel aus Serienwerten in TA-Lib-Reihenfolge:
  `(upper, middle, lower)`
- das Ergebnis muss vor weiterer Verwendung destrukturiert werden

## `dema(series[, length=30])`, `tema(series[, length=30])`, `trima(series[, length=30])`, `kama(series[, length=30])`, `t3(series[, length=5[, volume_factor=0.7]])` und `trix(series[, length=30])`

Regeln:

- das erste Argument muss `series<float>` sein
- das optionale `length` verwendet standardmaessig `30` fuer `dema`, `tema`,
  `trima`, `kama` und `trix`
- `t3` verwendet standardmaessig `length=5` und `volume_factor=0.7`
- wenn angegeben, muss `length` ein positives Integer-Literal sein
- wenn angegeben, muss `volume_factor` ein numerischer Skalar sein
- der Ergebnistyp ist `series<float>`

## `mavp(series, periods, minimum_period, maximum_period, ma_type)`

Regeln:

- die ersten beiden Argumente muessen `series<float>` sein
- `minimum_period` und `maximum_period` muessen Integer-Literale groesser oder
  gleich `2` sein
- das fuenfte Argument muss ein typisierter `ma_type.<variant>`-Wert sein
- die gleitende Durchschnittsfamilie ist dieselbe ausfuehrbare
  `ma_type`-Teilmenge wie bei `ma(...)`
- `periods` wird pro Bar in den Bereich `[minimum_period, maximum_period]`
  geklemmt
- der Ergebnistyp ist `series<float>`

## `mama(series[, fast_limit=0.5[, slow_limit=0.05]])`

Regeln:

- das erste Argument muss `series<float>` sein
- `fast_limit` und `slow_limit` verwenden standardmaessig `0.5` und `0.05`
- wenn angegeben, muessen beide optionalen Argumente numerische Skalare sein
- der Ergebnistyp ist ein 2-Tupel aus Serienwerten in TA-Lib-Reihenfolge:
  `(mama, fama)`
- das Ergebnis muss vor weiterer Verwendung destrukturiert werden

## `ht_dcperiod(series)`, `ht_dcphase(series)`, `ht_phasor(series)`, `ht_sine(series)`, `ht_trendline(series)` und `ht_trendmode(series)`

Regeln:

- jede Funktion erfordert genau ein `series<float>`-Argument
- `ht_dcperiod`, `ht_dcphase` und `ht_trendline` liefern `series<float>`
- `ht_trendmode` liefert `series<float>` mit TA-Libs `0`/`1`-Trendmode-Werten
- `ht_phasor` liefert ein 2-Tupel `(inphase, quadrature)`
- `ht_sine` liefert ein 2-Tupel `(sine, lead_sine)`
- Tupel-Ergebnisse muessen vor weiterer Verwendung destrukturiert werden
- diese Indikatoren folgen dem Hilbert-Transform-Warmup von TA-Lib und liefern
  `na`, bis der Upstream-Lookback erfuellt ist

## `sar(high, low[, acceleration=0.02[, maximum=0.2]])` und `sarext(high, low[, ...])`

Regeln:

- `high` und `low` muessen `series<float>` sein
- alle optionalen SAR-Parameter sind numerische Skalare
- `sar` liefert den standardmaessigen Parabolic SAR
- `sarext` exponiert die erweiterten TA-Lib-SAR-Kontrollen und liefert waehrend
  Short-Phasen negative Werte, wie im TA-Lib-Upstream
- der Ergebnistyp ist `series<float>`

## `wma(series[, length=30])`

Regeln:

- das erste Argument muss `series<float>` sein
- das optionale `length` verwendet standardmaessig `30`
- wenn angegeben, muss `length` ein Integer-Literal groesser oder gleich `2`
  sein
- der Ergebnistyp ist `series<float>`
- wenn nicht genug Historie vorhanden ist, ist das aktuelle Sample `na`
- wenn das benoetigte Fenster `na` enthaelt, ist das aktuelle Sample `na`

## `midpoint(series[, length=14])` und `midprice(high, low[, length=14])`

Regeln:

- `midpoint` erfordert `series<float>` als erstes Argument
- `midprice` erfordert `series<float>` fuer sowohl `high` als auch `low`
- das optionale Abschlussfenster verwendet standardmaessig `14`
- wenn angegeben, muss das Fenster ein Integer-Literal groesser oder gleich `2`
  sein
- das Fenster enthaelt das aktuelle Sample
- wenn nicht genug Historie vorhanden ist, ist das Ergebnis `na`
- wenn irgendein benoetigtes Sample im Fenster `na` ist, ist das Ergebnis `na`
- der Ergebnistyp ist `series<float>`

## `linearreg(series[, length=14])`, `linearreg_angle(series[, length=14])`, `linearreg_intercept(series[, length=14])`, `linearreg_slope(series[, length=14])` und `tsf(series[, length=14])`

Regeln:

- das erste Argument muss `series<float>` sein
- das optionale `length` verwendet standardmaessig `14`
- wenn angegeben, muss `length` ein Integer-Literal groesser oder gleich `2`
  sein
- wenn nicht genug Historie vorhanden ist, ist das aktuelle Sample `na`
- wenn das benoetigte Fenster `na` enthaelt, ist das aktuelle Sample `na`
- `linearreg` liefert den gefitteten Wert auf der aktuellen Bar
- `linearreg_angle` liefert den Winkel der gefitteten Steigung
- `linearreg_intercept` liefert den gefitteten Achsenabschnitt
- `linearreg_slope` liefert die gefittete Steigung
- `tsf` liefert die Ein-Schritt-Vorhersage
- der Ergebnistyp ist `series<float>`

## `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])`

Rules:

- the first three arguments must be `series<float>`
- omitted `atr_length` defaults to `10`
- omitted `multiplier` defaults to `3.0`
- if provided, `atr_length` must be an integer literal greater than or equal to `1`
- if provided, `multiplier` must be a numeric scalar
- `supertrend` returns a 2-tuple `(line, bullish)`
- `line` is the active carried band and `bullish` is the persistent regime direction
- the ATR component uses Wilder smoothing and requires prior-close history, so the result is `na` until the lookback is satisfied
- tuple-valued outputs must be destructured before further use

## `donchian(high, low[, length=20])`

Rules:

- the first two arguments must be `series<float>`
- omitted `length` defaults to `20`
- if provided, `length` must be an integer literal greater than or equal to `1`
- `donchian` returns a 3-tuple `(upper, middle, lower)`
- `upper` is the trailing highest high, `lower` is the trailing lowest low, and `middle` is `(upper + lower) / 2`
- if insufficient history exists, or any required sample is `na`, the current tuple is `(na, na, na)`
- tuple-valued outputs must be destructured before further use
