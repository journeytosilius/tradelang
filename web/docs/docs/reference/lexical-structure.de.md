# Lexikalische Struktur

PalmScript-Quelltext wird vor dem Parsen tokenisiert. Der Lexer muss die
Quellreihenfolge und Spans erhalten und unbekannte Zeichen sowie ungueltige
Intervall-Literale ablehnen.

## Schluesselwoerter

Die folgenden Tokens sind reservierte Schluesselwoerter:

- `fn`
- `let`
- `interval`
- `source`
- `use`
- `const`
- `input`
- `export`
- `regime`
- `trigger`
- `entry`
- `exit`
- `protect`
- `target`
- `order`
- `if`
- `else`
- `and`
- `or`
- `true`
- `false`
- `na`

Diese Schluesselwoerter duerfen nicht dort verwendet werden, wo ein
Identifier erforderlich ist.

## Bezeichner

Ein Bezeichner:

- muss mit einem ASCII-Buchstaben oder `_` beginnen
- darf mit ASCII-Buchstaben, Ziffern oder `_` fortgesetzt werden

Beispiele:

- `trend`
- `_tmp1`
- `weekly_basis`

## Literale

### Zahlenliterale

Zahlenliterale werden als `f64` geparst.

Akzeptierte Formen:

- `1`
- `14`
- `3.5`

Abgelehnte Formen:

- Exponentialschreibweise wie `1e6`
- fuehrende Punktformen wie `.5`

Negative Zahlen werden ueber das unäre `-` ausgedrueckt, nicht ueber ein
separates signiertes Literal-Token.

### Boolesche Literale

Boolesche Literale sind:

- `true`
- `false`

### Missing-Value-Literal

`na` ist das Literal fuer fehlende Werte.

### String-Literale

String-Literale werden derzeit nur dort akzeptiert, wo die Grammatik sie in
`source`-Deklarationen erlaubt.

Sie:

- sind durch `"` begrenzt
- duerfen einfache Escapes fuer `"`, `\\`, `\\n`, `\\r` und `\\t` enthalten
- duerfen keine ungeescapte neue Zeile enthalten

## Kommentare

Es werden nur einzeilige Kommentare unterstuetzt:

```palmscript
// Trendregime
let fast = ema(spot.close, 5)
```

Ein einzelnes `/` ist der arithmetische Divisionsoperator. `//` startet einen
einzeiligen Kommentar.

## Anweisungs-Trennzeichen

Anweisungen werden getrennt durch:

- Zeilenumbrueche
- Semikolons

Zeilenumbrueche innerhalb von Klammern oder eckigen Klammern beenden keine
Anweisung.

## Intervall-Literale

Intervall-Literale sind gross-/kleinschreibungs-sensitiv. Die akzeptierte Menge
ist in der [Intervalltabelle](intervals.md) definiert.

Zum Beispiel:

- `1w` ist gueltig
- `1M` ist gueltig
- `1W` ist ungueltig
