# Ausgaben

Diese Seite definiert die fuer Benutzer sichtbaren Ausgabeformen in PalmScript.

## Ausgabeformen

PalmScript stellt drei Konstrukte zur Ausgabeerzeugung bereit:

- `plot(value)`
- `export name = expr`
- `regime name = expr`
- `trigger name = expr`
- `entry long = expr`, `entry1 long = expr`, `entry2 long = expr`,
  `entry3 long = expr`
- `entry short = expr`, `entry1 short = expr`, `entry2 short = expr`,
  `entry3 short = expr`
- `exit long = expr`, `exit short = expr`
- `protect long = order_spec`, `protect short = order_spec`
- `protect_after_target1 long = order_spec`,
  `protect_after_target2 long = order_spec`,
  `protect_after_target3 long = order_spec`
- `protect_after_target1 short = order_spec`,
  `protect_after_target2 short = order_spec`,
  `protect_after_target3 short = order_spec`
- `target long = order_spec`, `target1 long = order_spec`,
  `target2 long = order_spec`, `target3 long = order_spec`
- `target short = order_spec`, `target1 short = order_spec`,
  `target2 short = order_spec`, `target3 short = order_spec`
- `size entry long = expr`, `size entry1 long = expr`,
  `size entry2 long = expr`, `size entry3 long = expr`
- `size entry short = expr`, `size entry1 short = expr`,
  `size entry2 short = expr`, `size entry3 short = expr`
- `size target long = expr`, `size target1 long = expr`,
  `size target2 long = expr`, `size target3 long = expr`
- `size target short = expr`, `size target1 short = expr`,
  `size target2 short = expr`, `size target3 short = expr`

`plot` ist ein Builtin-Aufruf. `export`, `regime` und `trigger` sind Deklarationen.

## `plot`

`plot` emittiert einen Plot-Punkt fuer den aktuellen Schritt.

Regeln:

- das Argument muss numerisch, `series<float>` oder `na` sein
- der aktuelle Schritt liefert pro ausgefuehrtem `plot`-Aufruf genau einen
  Plot-Punkt
- `plot` erzeugt keine wiederverwendbare Sprachbindung
- `plot` ist innerhalb von benutzerdefinierten Funktionskoerpern nicht erlaubt

## `export`

`export` veroeffentlicht eine benannte Ausgabeserie:

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
```

Regeln:

- nur auf Top-Level
- der Name muss im aktuellen Scope eindeutig sein
- der Ausdruck darf zu numerisch, bool, numerischer Serie, boolescher Serie
  oder `na` auswerten
- `void` wird abgelehnt

Typ-Normalisierung:

- numerische, numerische Serien- und `na`-Exports werden zu `series<float>`
- boolesche und boolesche Serien-Exports werden zu `series<bool>`

## `regime`

`regime` veroeffentlicht eine benannte persistente boolesche Marktzustandsserie:

```palmscript
regime trend_long = state(
    ema(spot.close, 20) > ema(spot.close, 50),
    ema(spot.close, 20) < ema(spot.close, 50)
)
```

Regeln:

- nur auf Top-Level
- der Ausdruck muss zu `bool`, `series<bool>` oder `na` auswerten
- der Ausgabetyp ist immer `series<bool>`
- `regime`-Namen werden nach dem Deklarationspunkt zu wiederverwendbaren Bindungen
- `regime` ist fuer die Kombination mit `state(...)`, `activated(...)` und `deactivated(...)` gedacht
- Laufzeitdiagnosen erfassen es zusammen mit gewoehnlichen exportierten Serien

## `trigger`

`trigger` veroeffentlicht eine benannte boolesche Ausgabeserie:

```palmscript
trigger breakout = spot.close > spot.high[1]
```

Regeln:

- nur auf Top-Level
- der Ausdruck muss zu `bool`, `series<bool>` oder `na` auswerten
- der Ausgabetyp ist immer `series<bool>`

Laufzeit-Ereignisregel:

- ein Trigger-Ereignis wird fuer einen Schritt nur emittiert, wenn das aktuelle
  Trigger-Sample `true` ist
- `false` und `na` emittieren keine Trigger-Ereignisse

## Erstklassige Strategie-Signale

PalmScript stellt erstklassige Strategie-Signaldeklarationen fuer
strategieorientierte Ausfuehrung bereit:

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)
entry short = spot.close < spot.low[1]
exit short = spot.close > ema(spot.close, 20)
```

Regeln:

- die vier Deklarationen sind nur auf Top-Level erlaubt
- jeder Ausdruck muss zu `bool`, `series<bool>` oder `na` auswerten
- sie werden zu Trigger-Ausgaben mit expliziten Signalrollen-Metadaten
  kompiliert
- die Laufzeit-Ereignisemission folgt denselben `true`/`false`/`na`-Regeln wie
  gewoehnliche Trigger
- `entry long` und `entry short` sind Kompatibilitaets-Aliase fuer
  `entry1 long` und `entry1 short`
- `entry2` und `entry3` sind sequenzielle, gleichseitige Add-on-Signale, die
  erst gueltig werden, nachdem die vorherige Stufe im aktuellen Positionszyklus
  gefuellt wurde

## Order-Deklarationen

PalmScript stellt ausserdem Top-Level-`order`-Deklarationen bereit, die
parametrisieren, wie eine Signalrolle ausgefuehrt wird:

```palmscript
execution exec = binance.spot("BTCUSDT")
order_template maker_entry = limit(price = spot.close[1], tif = tif.gtc, post_only = false, venue = exec)
order_template stop_exit = stop_market(trigger_price = lowest(spot.low, 5)[1], trigger_ref = trigger_ref.last, venue = exec)
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)

order entry long = maker_entry
order exit long = stop_exit
```

Regeln:

- Order-Deklarationen sind nur auf Top-Level erlaubt
- `order_template` ist ebenfalls nur auf Top-Level erlaubt und definiert wiederverwendbare Spezifikationen
- pro Signalrolle darf es hoechstens eine `order`-Deklaration geben
- ausfuehrungsorientierte CLI-Modi verlangen eine explizite `order ...`-Deklaration fuer jede `entry`- / `exit`-Signalrolle
- `order ... = <template_name>` verwendet ein zuvor deklariertes `order_template`
- Templates duerfen ein anderes Template referenzieren, zyklische Referenzen werden aber abgelehnt
- numerische Order-Felder wie `price`, `trigger_price` und `expire_time_ms`
  werden von der Runtime als versteckte interne Serien ausgewertet
- `tif.<variant>` und `trigger_ref.<variant>` sind typisierte Enum-Literale,
  die zur Compile-Zeit geprueft werden
- venue-spezifische Kompatibilitaetspruefungen laufen beim Start des Backtests
  anhand der Ausfuehrungs-`source`

## Angehaengte Exits

PalmScript stellt ausserdem erstklassige angehaengte Exits bereit, sodass das
diskretionaere `exit`-Signal frei bleibt:

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)
protect long = stop_market(position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref.last)
target long = take_profit_market(
    highest_since(position_event.long_entry_fill, spot.high) + 4,
    trigger_ref.last
)
size target long = 0.5
```

Regeln:

- angehaengte Exits sind nur auf Top-Level erlaubt
- `protect` ist die Basis-Schutzstufe fuer eine Seite
- `protect_after_target1`, `protect_after_target2` und
  `protect_after_target3` koennen die aktive Protect-Order nach jedem
  gestuften Target-Fill nachziehen
- `target`, `target1`, `target2` und `target3` sind sequenzielle gestufte
  Gewinnmitnahmen; `target` ist ein Kompatibilitaets-Alias fuer `target1`
- `size entry1..3` und `size target1..3` sind optional pro Stufe und gelten nur
  fuer den jeweiligen gestuften Entry oder Target
- gestufte Entry-Groessen unterstuetzen:
  - eine nackte Legacy-Fraction wie `0.5`
  - `capital_fraction(x)`
  - `risk_pct(pct, stop_price)`
- `capital_fraction(...)` muss zu einer endlichen Fraction in `(0, 1]`
  auswerten
- eine Entry-Size-Fraction unter `1` laesst Kapital fuer spaetere gleichseitige
  Scale-ins auf spaeteren Entry-Stufen frei
- `risk_pct(...)` ist in v1 nur fuer Entries gueltig und skaliert anhand des
  tatsaechlichen Fill-Preises und der Stop-Distanz zum Fill-Zeitpunkt
- wenn ein `risk_pct(...)` mehr Kapital verlangt, als aktuelles Bargeld oder
  freie Collateral erlauben, begrenzt der Backtester den Fill und markiert
  `capital_limited = true`
- sie werden erst aktiv, nachdem ein passender Entry-Fill existiert
- sie werden einmal pro Ausfuehrungsbalken neu ausgewertet, solange die
  Position offen ist
- gleichzeitig aktiv sind nur die aktuelle gestufte Protect-Order und das
  naechste gestufte Target
- wenn `target1` fuellt, wechselt die Engine von `protect` zu
  `protect_after_target1`, falls deklariert, sonst erbt sie die zuletzt
  verfuegbare Protect-Stufe
- gestufte Target-Size-Fractions muessen zu einer endlichen Fraction in
  `(0, 1]` auswerten
- eine `size targetN ...`-Deklaration macht die entsprechende Target-Stufe zu
  einer partiellen Gewinnmitnahme, wenn die Fraction kleiner als `1` ist
- gestufte Targets sind innerhalb eines Positionszyklus einmalig und werden
  sequenziell aktiv
- wenn beide auf derselben Ausfuehrungsbar fillbar werden, gewinnt `protect`
  deterministisch
- `position.*` ist nur innerhalb von `protect`- und `target`-Deklarationen
  verfuegbar
- `position_event.*` ist ein backtestgetriebener Serien-Namespace, der reale
  Fill-Ereignisse wie `position_event.long_entry_fill` exponiert
- `position_event.*` exponiert ausserdem exitspezifische Fill-Ereignisse wie
  `position_event.long_target_fill`, `position_event.long_protect_fill` und
  `position_event.long_liquidation_fill`
- gestufte Fill-Ereignisse sind ebenfalls verfuegbar, darunter
  `position_event.long_entry1_fill`, `position_event.long_entry2_fill`,
  `position_event.long_entry3_fill`, `position_event.long_target1_fill`,
  `position_event.long_target2_fill` und `position_event.long_target3_fill`
  sowie entsprechende Felder fuer die Short-Seite
- `last_exit.*`, `last_long_exit.*` und `last_short_exit.*` exponieren den
  juengsten Closed-Trade-Snapshot global oder pro Seite
- `last_*_exit.kind` wird mit typisierten Enum-Literalen wie
  `exit_kind.target` und `exit_kind.liquidation` verglichen
- `last_*_exit.stage` exponiert die gestufte Target-/Protect-Stufennummer, wenn
  zutreffend
- ausserhalb von Backtests ist `position_event.*` definiert, evaluiert aber auf
  jedem Schritt zu `false`
- ausserhalb von Backtests ist `last_*_exit.*` definiert, evaluiert aber zu
  `na`

## Reserved Trading Trigger Names

- `trigger long_entry = ...`, `trigger long_exit = ...`, `trigger short_entry = ...`, and `trigger short_exit = ...` are no longer executable aliases
- use first-class `entry` / `exit` declarations plus matching `order ...` templates instead
- ordinary `trigger` declarations with other names remain valid

## Laufzeit-Ausgabesammlungen

Ueber einen kompletten Lauf sammelt die Runtime:

- `plots`
- `exports`
- `triggers`
- `order_fields`
- `trigger_events`
- `alerts`

`alerts` existieren derzeit in den Runtime-Ausgabestrukturen, werden aber nicht
durch ein erstklassiges PalmScript-Sprachkonstrukt erzeugt.

## Ausgabezeit Und Bar-Index

Jedes Ausgabe-Sample ist markiert mit:

- dem aktuellen `bar_index`
- der aktuellen Schritt-`time`

In quellenbewussten Laeufen ist die Schrittzeit die Oeffnungszeit des aktuellen
Basis-Taktschritts.

## Latest Diagnostics Additions

PalmScript now exposes richer machine-readable backtest diagnostics in every public locale build:

- `run backtest`, `run walk-forward`, and `run optimize` accept `--diagnostics summary|full-trace`
- summary mode keeps cohort, drawdown-path, source-alignment, holdout-drift, robustness, and hint data
- full-trace mode adds one typed per-bar decision trace per execution bar
- optimize output now includes top-candidate holdout checks plus parameter stability summaries

## Latest Execution Additions

- `execution` declarations now separate execution routing from market-data `source` bindings.
- Order constructors accept named arguments in addition to the legacy positional form.
- `venue = <execution_alias>` binds an `order`, `protect`, or `target` role to a declared execution alias.
- Named order arguments cannot be mixed with positional arguments in the same constructor call.
- Trading scripts now require at least one declared `execution` target.
