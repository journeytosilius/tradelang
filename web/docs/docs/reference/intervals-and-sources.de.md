# Intervalle Und Quellen

Diese Seite definiert die normativen Intervall- und Quellenregeln von
PalmScript.

## Unterstuetzte Intervalle

PalmScript akzeptiert die Intervall-Literale aus der
[Intervalltabelle](intervals.md). Intervalle sind gross-/kleinschreibungs-
sensitiv.

## Basisintervall

Jedes Skript deklariert genau ein Basisintervall:

```palmscript
interval 1m
```

Das Basisintervall definiert den Ausfuehrungstakt.

## Benannte Quellen

Ausfuehrbare Skripte deklarieren eine oder mehrere benannte, exchangegestuetzte
Quellen:

```palmscript
interval 1m
source bb = bybit.usdt_perps("BTCUSDT")
source bn = binance.spot("BTCUSDT")
use bb 1h

plot(bn.close - bb.1h.close)
```

Regeln:

- mindestens eine `source`-Deklaration ist erforderlich
- Marktserien muessen quellqualifiziert sein
- jede deklarierte Quelle liefert einen Basis-Feed auf dem Basisintervall des
  Skripts
- `use <alias> <interval>` deklariert ein zusaetzliches Intervall fuer diese
  Quelle
- `<alias>.<field>` referenziert diese Quelle auf dem Basisintervall
- `<alias>.<interval>.<field>` referenziert diese Quelle auf dem benannten
  Intervall
- Referenzen auf Intervalle unterhalb des Basisintervalls werden abgelehnt

## Unterstuetzte Quell-Templates

PalmScript unterstuetzt derzeit diese erstklassigen Templates:

- `binance.spot("<symbol>")`
- `binance.usdm("<symbol>")`
- `bybit.spot("<symbol>")`
- `bybit.usdt_perps("<symbol>")`
- `gate.spot("<symbol>")`
- `gate.usdt_perps("<symbol>")`

Die Intervall-Unterstuetzung ist template-spezifisch:

- `binance.spot` akzeptiert alle unterstuetzten PalmScript-Intervalle
- `binance.usdm` akzeptiert alle unterstuetzten PalmScript-Intervalle
- `bybit.spot` akzeptiert `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w` und `1M`
- `bybit.usdt_perps` akzeptiert `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w` und `1M`
- `gate.spot` akzeptiert `1s`, `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h`, `1d` und `1M`
- `gate.usdt_perps` akzeptiert `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h` und `1d`

Auch operative Fetch-Beschraenkungen sind template-spezifisch:

- Bybit verwendet venue-native Symbole wie `BTCUSDT`
- Gate verwendet venue-native Symbole wie `BTC_USDT`
- Bybit-REST-Klines kommen absteigend sortiert zurueck und PalmScript ordnet sie vor der Laufzeitpruefung neu
- Bybit-Spot- und Perp-Kline-Zeitstempel koennen als JSON-Integer oder integer-artige Strings ankommen; PalmScript akzeptiert beide Formen direkt
- Gate-Candlestick-APIs verwenden Unix-Sekunden und PalmScript normalisiert sie auf Unix-Millisekunden UTC
- Gate-Spot- und Futures-Paginierung erfolgt in Zeitfenstern, weil die oeffentliche API `limit` nicht mit `from` / `to` kombiniert
- Gate-Spot- und Futures-Anfragen sind auf 1000 Kerzen pro HTTP-Aufruf begrenzt, damit venue-seitige Bereichslimits keine vermeidbaren `400 Bad Request`-Fehler ausloesen
- Binance-, Bybit- und Gate-Feeds werden intern paginiert
- wenn ein Venue-Abruf fehlschlaegt, zeigt PalmScript die Request-URL und einen gekuerzten Ausschnitt des Response-Bodys an, sofern vorhanden, sowohl bei HTTP-Fehlern ungleich 200 als auch bei fehlerhaften JSON-Payloads
- Basis-URLs lassen sich mit `PALMSCRIPT_BINANCE_SPOT_BASE_URL`,
  `PALMSCRIPT_BINANCE_USDM_BASE_URL`, `PALMSCRIPT_BYBIT_BASE_URL`,
  `PALMSCRIPT_GATE_BASE_URL` ueberschreiben; fuer Gate funktionieren sowohl die
  Host-Wurzel, zum Beispiel `https://api.gateio.ws`, als auch die vollstaendige
  `/api/v4`-Basis-URL

## Quellen-Feldmenge

Alle Quell-Templates werden in dieselben kanonischen Marktfelder normalisiert:

- `time`
- `open`
- `high`
- `low`
- `close`
- `volume`

Regeln:

- `time` ist die Kerzen-Oeffnungszeit in Unix-Millisekunden UTC
- Preis- und Volumenfelder sind numerisch
- `binance.usdm("<symbol>")` stellt ausserdem historische Hilfsfelder bereit:
  `funding_rate`, `mark_price`, `index_price`, `premium_index` und `basis`
- diese Hilfsfelder sind nur fuer `binance.usdm`-Aliases gueltig
- historische Modi laden diese Datensaetze automatisch, sobald ein Skript sie referenziert
- `run paper` lehnt Skripte mit diesen Hilfsfeldern ab, bis Live-Polling dafuer existiert

## Gleiche, Hoehere Und Niedrigere Intervalle

PalmScript unterscheidet drei Faelle fuer ein referenziertes Intervall relativ
zum Basisintervall:

- gleiches Intervall: gueltig
- hoeheres Intervall: gueltig, wenn mit `use <alias> <interval>` deklariert
- niedrigeres Intervall: abgelehnt

## Laufzeit-Semantik

Im Marktmodus:

- PalmScript laedt die benoetigten `(source, interval)`-Feeds direkt von den
  Venues
- die Basis-Zeitleiste ist die Vereinigung aller Basisintervall-
  Kerzenoeffnungszeiten der deklarierten Quellen
- wenn eine Quelle auf einem Zeitschritt keine Basis-Kerze hat, liefert diese
  Quelle auf diesem Schritt `na`
- langsamere Quellintervalle behalten ihren letzten voll geschlossenen Wert,
  bis ihre naechste Schlussgrenze erreicht ist

## Kein-Lookahead-Garantie

PalmScript darf keine Hoeherintervall-Kerze sichtbar machen, bevor diese Kerze
vollstaendig geschlossen ist.

Das gilt auch fuer quellqualifizierte Zusatzintervalle wie `bb.1h.close`.

## Laufzeit-Ausrichtungsregeln

Vorbereitete Feeds muessen an ihren deklarierten Intervallen ausgerichtet sein.

Die Runtime lehnt Feeds ab, die:

- nicht auf der Intervallgrenze ausgerichtet sind
- unsortiert sind
- bei derselben Intervall-Oeffnungszeit Duplikate enthalten
