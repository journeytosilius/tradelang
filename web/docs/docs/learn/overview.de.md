# PalmScript Lernen

Die offentliche PalmScript-Dokumentation ist um zwei Dinge herum aufgebaut:

- die Sprache zum Schreiben von Strategien
- Beispiele, die zeigen, wie Skripte geschrieben und verwendet werden

## Was Du Mit PalmScript Tust

Typischer Ablauf:

1. ein `.ps`-Skript schreiben
2. ein Basis-`interval` deklarieren
3. eine oder mehrere `source`-Bindungen deklarieren
4. es in der Browser-IDE validieren
5. es in der App uber historische Daten laufen lassen

## Lange Optimierungen

Fur lange CLI-Tuning-Laufe:

- nutze `palmscript run optimize ...`, wenn du das Ergebnis im Vordergrund willst
- nutze `palmscript run optimize ...` fuer direkte Optimierung in der CLI
- speichere brauchbare Kandidaten mit `--preset-out best.json`, damit du sie mit `run backtest` oder `run walk-forward` erneut pruefen kannst
- lasse den standardmaessigen unangetasteten Holdout aktiv, sofern du diesen Schutz nicht bewusst abschaltest
- fuege explizite Constraints wie `--min-sharpe`, `--min-holdout-pass-rate` und `--max-overfitting-risk` hinzu, wenn der Optimizer nur in der feasible region suchen soll
- fuege `--direct-validate-top <N>` hinzu, wenn der Optimizer die besten feasible survivors automatisch ueber das volle Fenster replayen soll

## Was Du Als Nachstes Lesen Solltest

- Erster ausfuhrbarer Ablauf: [Schnellstart](quickstart.md)
- Erste vollstandige Strategiefuhrung: [Erste Strategie](first-strategy.md)
- Sprachuberblick: [Sprachuberblick](language-overview.md)
- Exakte Regeln und Semantik: [Referenz-Uberblick](../reference/overview.md)

## Rollen Der Dokumentation

- `Lernen` erklart, wie man PalmScript effektiv einsetzt.
- `Referenz` definiert, was PalmScript bedeutet.

## Latest Diagnostics Additions

PalmScript now exposes richer machine-readable backtest diagnostics in every public locale build:

- `run backtest`, `run walk-forward`, and `run optimize` accept `--diagnostics summary|full-trace`
- summary mode keeps cohort, drawdown-path, baseline-comparison, source-alignment, holdout-drift, robustness, overfitting-risk, validation-constraint, and hint data, and top-level backtests also add bounded date-perturbation reruns
- full-trace mode adds one typed per-bar decision trace per execution bar
- optimize output now includes top-candidate holdout checks plus validation-constraint, feasible vs infeasible survivor counts, constraint-failure breakdowns, optional direct-validation survivor replays, holdout-pass-rate, parameter stability, baseline-comparison, and overfitting-risk summaries

## Lokale Paper-Ausfuhrung

PalmScript enthaelt jetzt auch einen lokalen Paper-Ausfuehrungs-Daemon:

- `palmscript run paper ...` legt eine persistente Paper-Session an
- `palmscript execution serve` verarbeitet diese Sessions mit echten Exchange-Daten auf geschlossenen Kerzen
- die Session verwendet dieselbe kompilierte VM, dieselbe Ordersimulation und dieselben Portfolio-Regeln wie der Backtester
- die Paper-Snapshots zeigen jetzt auch Top-of-Book Bid/Ask, den daraus abgeleiteten Mid Price und Last-/Mark-Preise, wenn vorhanden
- v1 verwendet nur Spielgeld und sendet niemals echte Orders
