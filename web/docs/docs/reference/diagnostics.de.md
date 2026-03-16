# Diagnosen

PalmScript stellt Diagnosen und Fehler aus drei oeffentlichen Schichten bereit.

## 1. Compile-Diagnosen

Compile-Diagnosen sind quellenbezogene Fehler mit Spans.

Diagnoseklassen:

- lexikalische Fehler
- Parser-Fehler
- Typ- und Namensaufloesungsfehler
- strukturelle Compile-Fehler

Beispiele:

- fehlendes oder doppeltes `interval`
- nicht unterstuetztes `source`-Template
- unbekannter Quellenalias
- nicht deklarierte `use`-Intervallreferenz
- Referenz auf ein Intervall unterhalb des Basisintervalls
- doppelte Bindungen
- ungueltige Funktionsrekursion
- ungueltige Builtin-Arity oder ungueltiger Argumenttyp

Diese Diagnosen erscheinen ueber:

- das Diagnosen-Panel im Browser-IDE-Editor
- Backtest-Anfragen aus der gehosteten App

## 2. Markt-Ladefehler

Nach erfolgreicher Kompilierung kann die Runtime-Vorbereitung beim Aufbau der
benoetigten historischen Feeds fehlschlagen.

Beispiele:

- das angeforderte Zeitfenster ist ungueltig
- das Skript hat keine `source`-Deklarationen
- eine Exchange-Anfrage schlaegt fehl
- eine Venue-Antwort ist fehlerhaft
- ein benoetigter Feed liefert im angeforderten Fenster keine Daten
- ein Symbol kann in der ausgewaehlten Venue nicht aufgeloest werden

Fetch-Fehler enthalten jetzt so viel Anfragekontext, wie PalmScript in der betroffenen Schicht kennt, zum Beispiel das angeforderte Zeitfenster und die Bootstrap-Phase des Paper-Feeds, die die Anfrage ausgeloest hat.

## 3. Laufzeitfehler

Laufzeitfehler treten auf, nachdem die Feed-Vorbereitung begonnen hat oder
waehrend der Ausfuehrung.

Beispiele:

- Feed-Ausrichtungsfehler
- fehlende oder doppelte Runtime-Feeds
- Erschoepfung des Instruktionsbudgets
- Stack-Unterlauf
- Typkonflikt waehrend der Ausfuehrung
- ungueltiger lokaler oder Serien-Slot
- History-Capacity-Ueberlauf
- Output-Typkonflikt bei der Sammlung der Ausgaben

Paper-Session-Manifeste und Snapshots enthalten ausserdem Fehlermeldungen pro Feed, sodass `paper-status` und `paper-export` zeigen koennen, welcher Feed in welcher Phase mit welcher Upstream-Fehlermeldung gescheitert ist.

## Schicht-Verantwortung

Welche Schicht fuer einen Fehler zustaendig ist, gehoert zum Vertrag:

- syntaktische und semantische Gueltigkeit gehoeren zur Kompilierung
- Gueltigkeit von Exchange, Netzwerk und Antwort gehoert zum Markt-Layer
- Feed-Konsistenz und Ausfuehrungs-Gueltigkeit gehoeren zur Runtime

PalmScript scheitert explizit, statt die Semantik stillschweigend abzuschwaechen.
