# Deklarationen Und Gultigkeitsbereich

Diese Seite definiert die Bindungsformen, die PalmScript akzeptiert, sowie die
damit verbundenen Sichtbarkeitsregeln.

## Nur-Top-Level-Formen

Die folgenden Formen duerfen nur auf Top-Level eines Skripts erscheinen:

- `interval`
- `source`
- `use`
- `fn`
- `const`
- `input`
- `export`
- `regime`
- `trigger`
- `entry`
- `exit`
- `protect`
- `target`

Top-Level-`let`, `if` und Ausdrucksanweisungen sind erlaubt.

## Basisintervall

Jedes Skript muss genau ein Basisintervall deklarieren:

```palmscript
interval 1m
```

Der Compiler lehnt ein Skript ohne Basis-`interval` oder mit mehr als einem
Basis-`interval` ab.

## Quell-Deklarationen

Eine Quell-Deklaration hat diese Form:

```palmscript
source bb = bybit.usdt_perps("BTCUSDT")
```

Regeln:

- der Alias muss ein Identifier sein
- der Alias muss unter allen deklarierten Quellen eindeutig sein
- das Template muss zu einem der unterstuetzten Quell-Templates aufgeloest
  werden
- das Symbolargument muss ein String-Literal sein

## `use`-Deklarationen

Zusatzintervalle werden pro Quelle deklariert:

```palmscript
use bb 1h
```

Regeln:

- der Alias muss eine deklarierte Quelle benennen
- das Intervall darf nicht niedriger als das Basisintervall sein
- doppelte `use <alias> <interval>`-Deklarationen werden abgelehnt
- ein Intervall gleich dem Basisintervall wird akzeptiert, ist aber redundant

## Funktionen

Benutzerdefinierte Funktionen sind Top-Level-Deklarationen mit Ausdruckskoerper:

```palmscript
fn cross_signal(a, b) = a > b and a[1] <= b[1]
```

Regeln:

- Funktionsnamen muessen eindeutig sein
- ein Funktionsname darf nicht mit einem Builtin-Namen kollidieren
- Parameternamen innerhalb einer Funktion muessen eindeutig sein
- rekursive und zyklische Funktionsgraphen werden abgelehnt
- Funktionskoerper duerfen auf ihre Parameter, deklarierte Quellserien und
  unveraenderliche Top-Level-Bindungen `const` / `input` verweisen
- Funktionskoerper duerfen `plot` nicht aufrufen
- Funktionskoerper duerfen keine `let`-Bindings aus umgebenden
  Anweisungsscopes capturen

Funktionen werden nach Argumenttyp und Aktualisierungstakt spezialisiert.

## `let`-Bindings

`let` erzeugt eine Bindung im aktuellen Block-Scope:

```palmscript
let basis = ema(spot.close, 20)
```

Regeln:

- ein doppeltes `let` im selben Scope wird abgelehnt
- innere Scopes duerfen aeussere Bindungen ueberschatten
- der gebundene Wert darf skalar oder Serie sein
- `na` ist erlaubt und wird waehrend der Kompilierung als numeriknaher
  Platzhalter behandelt

PalmScript unterstuetzt ausserdem Tupel-Destrukturierung fuer unmittelbare
tupelwertige Builtin-Ergebnisse:

```palmscript
let (line, signal, hist) = macd(spot.close, 12, 26, 9)
```

Zusaetzliche Regeln:

- Tupel-Destrukturierung ist eine erstklassige `let`-Form
- die rechte Seite muss derzeit ein unmittelbares tupelwertiges Builtin-
  Ergebnis sein
- die Tupel-Arität muss exakt uebereinstimmen
- tupelwertige Ausdruecke muessen vor weiterer Verwendung destrukturiert werden

## `const` Und `input`

PalmScript unterstuetzt unveraenderliche Top-Level-Bindungen fuer
Strategiekonfiguration:

```palmscript
input fast_len = 21
const neutral_rsi = 50
```

Regeln:

- beide Formen sind nur auf Top-Level erlaubt
- doppelte Namen im selben Scope werden abgelehnt
- beide Formen sind in v1 rein skalar: `float`, `bool`, `ma_type`, `tif`,
  `trigger_ref`, `position_side`, `exit_kind` oder `na`
- `input` ist in v1 nur zur Compile-Zeit wirksam
- `input`-Werte muessen skalare Literale oder Enum-Literale sein
- `const`-Werte duerfen auf zuvor deklarierte `const` / `input`-Bindings und
  reine skalare Builtins verweisen
- fensterbasierte Builtins und Serienindexierung akzeptieren unveraenderliche
  numerische Bindungen ueberall dort, wo ein Integer-Literal verlangt wird

## Ausgaben

`export`, `regime`, `trigger`, erstklassige Strategie-Signale und backtestbezogene
Order-Deklarationen sind nur auf Top-Level erlaubt:

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
regime trend_long = state(ema(spot.close, 20) > ema(spot.close, 50), ema(spot.close, 20) < ema(spot.close, 50))
trigger long_entry = spot.close > spot.high[1]
entry1 long = spot.close > spot.high[1]
entry2 long = crossover(spot.close, ema(spot.close, 20))
order entry1 long = limit(spot.close[1], tif.gtc, false)
protect long = stop_market(position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref.last)
protect_after_target1 long = stop_market(position.entry_price, trigger_ref.last)
target1 long = take_profit_market(position.entry_price + 4, trigger_ref.last)
target2 long = take_profit_market(position.entry_price + 8, trigger_ref.last)
size entry1 long = 0.5
size entry2 long = 0.5
size entry3 long = risk_pct(0.01, stop_price)
size target1 long = 0.5
```

Regeln:

- alle Formen sind nur auf Top-Level erlaubt
- doppelte Namen im selben Scope werden abgelehnt
- `regime` erfordert `bool`, `series<bool>` oder `na` und ist fuer persistente Marktzustands-Serien gedacht
- `regime`-Namen werden nach ihrem Deklarationspunkt zu Bindungen und mit gewoehnlichen exportierten Diagnosen erfasst
- `trigger`-Namen werden nach ihrem Deklarationspunkt zu Bindungen
- `entry long` und `entry short` sind Kompatibilitaets-Aliase fuer
  `entry1 long` und `entry1 short`
- `entry1`, `entry2` und `entry3` sind gestufte Backtest-Entry-Signal-
  Deklarationen
- `exit long` und `exit short` bleiben einzelne diskretionaere Vollpositions-
  Exits
- `order entry ...` und `order exit ...` haengen ein Ausfuehrungstemplate an
  eine passende Signalrolle
- `protect`, `protect_after_target1..3` und `target1..3` deklarieren gestufte
  angehaengte Exits, die nur aktiv sind, waehrend die passende Position offen
  ist
- `size entry1..3 long|short` kann einen gestuften Entry-Fill dimensionieren,
  entweder mit einer Legacy-Zahl-Fraction, `capital_fraction(x)` oder
  `risk_pct(pct, stop_price)` fuer risikobasierte Entry-Groesse
- `size target1..3 long|short` kann einen gestuften `target`-Fill als Anteil
  der offenen Position dimensionieren
- pro Signalrolle ist hoechstens eine `order`-Deklaration erlaubt
- pro gestufter Rolle ist hoechstens eine Deklaration erlaubt
- wenn eine Signalrolle keine explizite `order`-Deklaration hat, verwendet der
  Backtester implizit `market()`
- `size entry ...` und `size target ...` erfordern jeweils eine passende
  gestufte `order ...`- oder angehaengte `target ...`-Deklaration fuer dieselbe
  Rolle
- `risk_pct(...)` ist in v1 nur bei gestuften Entry-Size-Deklarationen gueltig
- gestufte angehaengte Exits sind sequenziell: immer nur die naechste
  Target-Stufe und die aktuelle Protect-Stufe sind aktiv
- `position.*` ist nur innerhalb von `protect`- und `target`-Deklarationen
  verfuegbar
- `position_event.*` ist ueberall verfuegbar, wo ein `series<bool>` gueltig
  ist, und dient dazu, Logik an echte Backtest-Fills zu binden
- aktuelle `position_event`-Felder sind:
  `long_entry_fill`, `short_entry_fill`, `long_exit_fill`, `short_exit_fill`,
  `long_protect_fill`, `short_protect_fill`, `long_target_fill`,
  `short_target_fill`, `long_signal_exit_fill`, `short_signal_exit_fill`,
  `long_reversal_exit_fill`, `short_reversal_exit_fill`,
  `long_liquidation_fill` und `short_liquidation_fill`
- gestufte Fill-Felder sind ebenfalls verfuegbar:
  `long_entry1_fill` .. `long_entry3_fill`,
  `short_entry1_fill` .. `short_entry3_fill`,
  `long_target1_fill` .. `long_target3_fill` und
  `short_target1_fill` .. `short_target3_fill`
- `last_exit.*`, `last_long_exit.*` und `last_short_exit.*` sind ueberall
  verfuegbar, wo gewoehnliche Ausdruecke gueltig sind
- aktuelle `last_*_exit`-Felder sind `kind`, `stage`, `side`, `price`, `time`,
  `bar_index`, `realized_pnl`, `realized_return` und `bars_held`
- `last_*_exit.kind` enthaelt `exit_kind.liquidation` zusaetzlich zu den
  vorhandenen Exit-Typen
- Legacy-Skripte im Stil `trigger long_entry = ...` bleiben als
  Kompatibilitaetsbruecke unterstuetzt, solange keine erstklassigen
  Signal-Deklarationen vorhanden sind

## Bedingter Scope

`if` fuehrt zwei Child-Scopes ein:

```palmscript
if spot.close > spot.open {
    let x = 1
} else {
    let x = 0
}
```

Regeln:

- die Bedingung muss zu `bool`, `series<bool>` oder `na` ausgewertet werden
- beide Zweige haben voneinander unabhaengige Scopes
- Bindungen, die in einem Zweig erzeugt werden, sind ausserhalb des `if` nicht
  sichtbar
