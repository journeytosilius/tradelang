# Mathematik-, Preis- Und Statistikindikatoren

Diese Seite definiert PalmScripts ausfuehrbare Mathematik-Transformationen,
Preis-Transformationen und statistikbezogenen Indikatoren.

## TA-Lib-Mathematik-Transformationen

Diese Builtins sind derzeit ausfuehrbar:

- `acos(real)`
- `asin(real)`
- `atan(real)`
- `ceil(real)`
- `cos(real)`
- `cosh(real)`
- `exp(real)`
- `floor(real)`
- `ln(real)`
- `log10(real)`
- `sin(real)`
- `sinh(real)`
- `sqrt(real)`
- `tan(real)`
- `tanh(real)`

Regeln:

- jedes erfordert genau ein numerisches oder `series<float>`-Argument
- wenn die Eingabe eine Serie ist, ist der Ergebnistyp `series<float>`
- wenn die Eingabe skalar ist, ist der Ergebnistyp `float`
- wenn die Eingabe `na` ist, ist das Ergebnis `na`

## TA-Lib-Arithmetik Und Preis-Transformationen

Diese Builtins sind derzeit ausfuehrbar:

- `add(a, b)`
- `div(a, b)`
- `mult(a, b)`
- `sub(a, b)`
- `avgprice(open, high, low, close)`
- `bop(open, high, low, close)`
- `medprice(high, low)`
- `typprice(high, low, close)`
- `wclprice(high, low, close)`

Regeln:

- alle Argumente muessen numerisch, `series<float>` oder `na` sein
- wenn eines der Argumente eine Serie ist, ist der Ergebnistyp `series<float>`
- andernfalls ist der Ergebnistyp `float`
- wenn irgendein benoetigter Eingang `na` ist, ist das Ergebnis `na`

Zusaetzliche OHLC-Regel:

- `bop` liefert `(close - open) / (high - low)` und liefert `0`, wenn
  `high - low <= 0`

## `max(series[, length=30])`, `min(series[, length=30])` und `sum(series[, length=30])`

Regeln:

- das erste Argument muss `series<float>` sein
- das optionale trailing window verwendet standardmaessig `30`
- wenn angegeben, muss das Fenster ein Integer-Literal groesser oder gleich `2`
  sein
- das Fenster enthaelt das aktuelle Sample
- wenn nicht genug Historie vorhanden ist, ist das Ergebnis `na`
- wenn irgendein Sample im benoetigten Fenster `na` ist, ist das Ergebnis `na`
- der Ergebnistyp ist `series<float>`

## `avgdev(series[, length=14])`

Regeln:

- das erste Argument muss `series<float>` sein
- das optionale `length` verwendet standardmaessig `14`
- wenn angegeben, muss `length` ein Integer-Literal groesser oder gleich `2`
  sein
- der Ergebnistyp ist `series<float>`
- wenn nicht genug Historie vorhanden ist, ist das aktuelle Sample `na`
- wenn das benoetigte Fenster `na` enthaelt, ist das aktuelle Sample `na`

## `maxindex(series[, length=30])` und `minindex(series[, length=30])`

Regeln:

- das erste Argument muss `series<float>` sein
- das optionale `length` verwendet standardmaessig `30`
- wenn angegeben, muss `length` ein Integer-Literal groesser oder gleich `2`
  sein
- `maxindex` und `minindex` liefern `series<float>` mit dem absoluten
  Bar-Index als `f64`
- wenn nicht genug Historie vorhanden ist, ist das aktuelle Sample `na`
- wenn das benoetigte Fenster `na` enthaelt, ist das aktuelle Sample `na`

## `minmax(series[, length=30])` und `minmaxindex(series[, length=30])`

Regeln:

- das erste Argument muss `series<float>` sein
- das optionale `length` verwendet standardmaessig `30`
- wenn angegeben, muss `length` ein Integer-Literal groesser oder gleich `2`
  sein
- `minmax` liefert ein 2-Tupel `(min_value, max_value)` in
  TA-Lib-Ausgabereihenfolge
- `minmaxindex` liefert ein 2-Tupel `(min_index, max_index)` in
  TA-Lib-Ausgabereihenfolge
- tupelwertige Ausgaben muessen vor weiterer Verwendung destrukturiert werden
- wenn nicht genug Historie vorhanden ist, ist das aktuelle Sample `na`
- wenn das benoetigte Fenster `na` enthaelt, ist das aktuelle Sample `na`

## `stddev(series[, length=5[, deviations=1.0]])` und `var(series[, length=5[, deviations=1.0]])`

Regeln:

- das erste Argument muss `series<float>` sein
- das optionale `length` verwendet standardmaessig `5`
- wenn angegeben, muss `length` ein Integer-Literal sein
- `stddev` erfordert `length >= 2`
- `var` erlaubt `length >= 1`
- `deviations` verwendet standardmaessig `1.0`
- `stddev` multipliziert die Quadratwurzel der rollierenden Varianz mit
  `deviations`
- `var` ignoriert das Argument `deviations`, um TA-Lib zu entsprechen
- der Ergebnistyp ist `series<float>`
- wenn nicht genug Historie vorhanden ist, ist das aktuelle Sample `na`
- wenn das benoetigte Fenster `na` enthaelt, ist das aktuelle Sample `na`

## `beta(series0, series1[, length=5])` und `correl(series0, series1[, length=30])`

Regeln:

- beide Eingaenge muessen `series<float>` sein
- `beta` verwendet standardmaessig `length=5`
- `correl` verwendet standardmaessig `length=30`
- wenn angegeben, muss `length` ein Integer-Literal sein, das das TA-Lib-
  Minimum fuer das jeweilige Builtin erfuellt
- `beta` folgt der TA-Lib-Formulierung auf Basis von Return-Ratios und liefert
  daher erst nach `length + 1` Quell-Samples einen Wert
- `correl` liefert die Pearson-Korrelation der gepaarten Roh-Eingabeserien
- der Ergebnistyp ist `series<float>`
- wenn nicht genug Historie vorhanden ist, ist das aktuelle Sample `na`
- wenn das benoetigte Fenster `na` enthaelt, ist das aktuelle Sample `na`

## `percentile(series[, length=20[, percentage=50.0]])`

Rules:

- the first argument must be `series<float>`
- omitted `length` defaults to `20`
- omitted `percentage` defaults to `50.0`
- if provided, `length` must be an integer literal greater than or equal to `1`
- if provided, `percentage` must be a numeric scalar
- `percentage` is clamped into the inclusive `0..100` range
- the trailing window is sorted and sampled with linear interpolation between adjacent ranks
- if insufficient history exists, or any required sample is `na`, the result is `na`
- the result type is `series<float>`

## `zscore(series[, length=20])`

Rules:

- the first argument must be `series<float>`
- omitted `length` defaults to `20`
- if provided, `length` must be an integer literal greater than or equal to `1`
- `zscore` evaluates the current sample against the trailing-window mean and population standard deviation
- if the trailing variance is `0`, `zscore` returns `0`
- if insufficient history exists, or any required sample is `na`, the result is `na`
- the result type is `series<float>`

## `ulcer_index(series[, length=14])`

Rules:

- the first argument must be `series<float>`
- omitted `length` defaults to `14`
- if provided, `length` must be an integer literal greater than or equal to `1`
- `ulcer_index` measures rolling drawdown severity in percentage terms over the trailing window
- it tracks the running peak across the window from oldest to newest, squares percentage drawdowns, averages them, and returns the square root
- if insufficient history exists, or any required sample is `na`, the result is `na`
- the result type is `series<float>`
