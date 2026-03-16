# Grammatik

Diese Seite ist die normative Grammatik fuer PalmScript.

Die Produktionen unten definieren die akzeptierte Parser-Oberflaeche. Regeln,
die von Namensaufloesung, Intervallreihenfolge oder Typen abhaengen, werden in
spaeteren Referenzkapiteln definiert.

## Programm

```text
program                ::= separator* item* EOF
item                   ::= interval_decl
                         | source_decl
                         | execution_decl
                         | use_decl
                         | function_decl
                         | stmt

interval_decl          ::= "interval" interval
source_decl            ::= "source" ident "=" source_template "(" string_literal ")"
execution_decl         ::= "execution" ident "=" source_template "(" string_literal ")"
source_template        ::= ident "." ident
use_decl               ::= "use" interval
                         | "use" ident interval
function_decl          ::= "fn" ident "(" param_list? ")" "=" expr
param_list             ::= ident ("," ident)*
```

## Anweisungen

```text
stmt                   ::= let_stmt
                         | const_stmt
                         | input_stmt
                         | export_stmt
                         | regime_stmt
                         | trigger_stmt
                         | risk_control_stmt
                         | module_stmt
                         | signal_stmt
                         | attached_exit_stmt
                         | order_template_stmt
                         | order_stmt
                         | transfer_stmt
                         | if_stmt
                         | expr_stmt

let_stmt               ::= "let" ident "=" expr
                         | "let" "(" ident ("," ident)+ ")" "=" expr
const_stmt             ::= "const" ident "=" expr
input_stmt             ::= "input" ident "=" expr
export_stmt            ::= "export" ident "=" expr
regime_stmt            ::= "regime" ident "=" expr
trigger_stmt           ::= "trigger" ident "=" expr
risk_control_stmt      ::= "cooldown" signal_side "=" expr
                         | "max_bars_in_trade" signal_side "=" expr
module_stmt            ::= "module" ident "=" module_role
module_role            ::= "entry" signal_side
                         | ("entry2" | "entry3") signal_side
signal_stmt            ::= "entry" signal_side "=" expr
                         | "exit" signal_side "=" expr
attached_exit_stmt     ::= "protect" signal_side "=" order_spec
                         | "target" signal_side "=" order_spec
order_template_stmt    ::= "order_template" ident "=" order_spec
order_stmt             ::= "order" ("entry" | "exit") signal_side "=" order_spec
transfer_stmt          ::= "transfer" ("quote" | "base") "=" transfer_spec
signal_side            ::= "long" | "short"
order_spec             ::= ident
                         | "market" "(" ")"
                         | "limit" "(" expr "," expr "," expr ")"
                         | "stop_market" "(" expr "," expr ")"
                         | "stop_limit" "(" expr "," expr "," expr "," expr "," expr "," expr ")"
                         | "take_profit_market" "(" expr "," expr ")"
                         | "take_profit_limit" "(" expr "," expr "," expr "," expr "," expr "," expr ")"
transfer_spec          ::= "quote_transfer" "(" named_order_arg ("," named_order_arg)* ")"
                         | "base_transfer" "(" named_order_arg ("," named_order_arg)* ")"
if_stmt                ::= "if" expr block "else" else_tail
else_tail              ::= if_stmt
                         | block
expr_stmt              ::= expr
block                  ::= "{" separator* stmt* "}"
```

`module`-Deklarationen sind nur auf Top-Level erlaubt. In v1 markieren sie nur
`entry`-, `entry2`- und `entry3`-Rollen fuer Diagnostik.

## Ausdruecke

```text
expr                   ::= conditional_expr
conditional_expr       ::= or_expr ("?" expr ":" conditional_expr)?
or_expr                ::= and_expr ("or" and_expr)*
and_expr               ::= cmp_expr ("and" cmp_expr)*
cmp_expr               ::= add_expr (cmp_op add_expr)*
cmp_op                 ::= "==" | "!=" | "<" | "<=" | ">" | ">="
add_expr               ::= mul_expr (("+" | "-") mul_expr)*
mul_expr               ::= unary_expr (("*" | "/") unary_expr)*
unary_expr             ::= ("-" | "!") unary_expr
                         | postfix_expr
postfix_expr           ::= primary_expr postfix*
postfix                ::= call_suffix
                         | index_suffix
                         | source_suffix
call_suffix            ::= "(" arg_list? ")"
index_suffix           ::= "[" expr "]"
source_suffix          ::= "." ident
                         | "." interval "." ident
arg_list               ::= expr ("," expr)*
```

## Primaerausdruecke

```text
primary_expr           ::= number
                         | "true"
                         | "false"
                         | "na"
                         | string_literal
                         | ident
                         | ident "." ident
                         | interval "." market_field
                         | "(" expr ")"
```

## Lexikalische Nichtterminale

```text
market_field           ::= "open" | "high" | "low" | "close" | "volume" | "time"
interval               ::= one of the literals listed in [Interval Table](intervals.md)
ident                  ::= identifier token
string_literal         ::= string token
number                 ::= numeric literal token
separator              ::= newline | ";"
```

## Bindung Und Praezedenz

PalmScript parst binaere Operatoren mit der folgenden Praezedenz, von niedrig
nach hoch:

1. `or`
2. `and`
3. `==`, `!=`, `<`, `<=`, `>`, `>=`
4. `+`, `-`
5. `*`, `/`
6. unäres `-`, unäres `!`
7. Aufruf `(...)`, Indexierung `[...]` und Quellen-/Feld-Qualifizierung mit `.`

Operatoren innerhalb einer Praezedenzstufe sind linksassoziativ, mit Ausnahme
des tertiaeren Konditionals, das rechtsassoziativ ist.

## Erforderliche Semantische Einschraenkungen

Die Grammatik allein macht ein Programm noch nicht gueltig. Die Implementierung
fordert zusaetzlich:

- ein Skript muss genau ein Basis-`interval` deklarieren
- ein Skript muss mindestens eine `source` deklarieren
- `interval`, `source`, `use`, `fn`, `const`, `input`, `export`, `regime`, `trigger`,
  `entry`, `exit`, `protect`, `target`, `order` und `size` duerfen nur auf
  Top-Level erscheinen
- nackte Marktbezeichner wie `close` werden abgelehnt und Marktserien muessen
  quellqualifiziert sein
- Referenzen auf hoehere Quellintervalle erfordern `use <alias> <interval>`
- jedes `if` muss ein `else` haben
- String-Literale werden lexikalisch akzeptiert, sind semantisch aber nur
  innerhalb von `source`-Deklarationen gueltig
- nur Identifikatoren sind aufrufbar
- Serienindexierung muss ein nicht-negatives Integer-Literal oder eine
  unveraenderliche Top-Level-Zahlenbindung verwenden
- tupelwertige Builtins muessen vor der Verwendung per Tupel-Destrukturierung
  gebunden werden
- `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`,
  `position_side.<variant>` und `exit_kind.<variant>` sind typisierte
  Enum-Namespaces
- `position.*` ist nur innerhalb von `protect`- und `target`-Deklarationen
  gueltig
- `position_event.*` ist ein backtestgetriebener `series<bool>`-Namespace
- `last_exit.*`, `last_long_exit.*` und `last_short_exit.*` sind
  backtestgetriebene Namespaces fuer den zuletzt geschlossenen Trade
- `ledger(<execution_alias>).base_free|quote_free|base_total|quote_total|mark_value_quote` ist ein backtestgetriebener Execution-Ledger-Namespace
- `entry1..3 long|short`, `target1..3 long|short` und
  `protect_after_target1..3 long|short` sind gueltige gestufte Deklarationen in
  v1
- `entry long` und `target long|short` bleiben Kompatibilitaets-Aliase fuer
  Stufe 1
- `cooldown long|short` und `max_bars_in_trade long|short` erfordern einen zur
  Compile-Zeit aufgeloesten nicht-negativen ganzzahligen Skalarausdruck
- `size entry1..3 long|short` und `size target1..3 long|short` sind gueltige
  gestufte `size`-Deklarationen in v1
- `size module <name> = <expr>` ist ebenfalls gueltig, wenn `<name>` zu einem deklarierten `module` aufgeloest wird, das an eine gestufte Entry-Rolle gebunden ist
- gestufte Entry-Groessen akzeptieren entweder eine nackte Legacy-Fraktion,
  `capital_fraction(x)` oder `risk_pct(pct, stop_price)`
- `size entry ...` erfordert eine passende gestufte `order entry ...`-
  Deklaration fuer dieselbe Rolle
- `size target ...` erfordert eine passende gestufte `target ...`-Deklaration
  fuer dieselbe Rolle
- `risk_pct(...)` ist nur bei gestuften Entry-Size-Deklarationen gueltig
- benutzerdefinierte Funktionen sind ausdrucksbasiert, nur auf Top-Level,
  nicht-rekursiv und duerfen keine umgebenden `let`-Bindings capturen
- benutzerdefinierte Funktionen duerfen unveraenderliche Top-Level-Bindungen
  `const` und `input` capturen
- Quell-, Intervall-, Scope- und Typregeln werden wie in den anderen
  `Reference`-Seiten beschrieben durchgesetzt

## Hinweis Zu `input ... optimize(...)`

Die Parser-Oberflaeche akzeptiert jetzt ein optionales `optimize(...)`-Suffix an `input`-Deklarationen. Dieses Suffix kann einen Integer-Bereich, einen Float-Bereich oder eine `choice`-Liste beschreiben, unterliegt aber weiterhin zusaetzlicher semantischer Validierung.

## Arbitrage Basket Runtime

- Portfolio-Backtests fuehren jetzt `market_pair(...)` aus, wenn mindestens zwei Spot-`execution`-Aliase ausgewaehlt sind
- in v1 bedeutet `size = ...` die Basis-Asset-Menge
- `limit_pair(...)` und `mixed_pair(...)` kompilieren weiter, werden zur Laufzeit aber abgelehnt, bis Resting-Pair-Orders implementiert sind

## Latest Portfolio Additions

- PalmScript now reserves `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, and `portfolio_group`.
- These declarations are top-level only and compile-time only.
- Portfolio mode activates when backtest-oriented CLI commands receive repeated `--execution-source` flags.
- Portfolio mode shares one equity ledger across the selected aliases and blocks only the new entries that would exceed the configured caps.

## Latest Execution Additions

- The parser now accepts `execution <alias> = exchange.market("SYMBOL")` as a top-level declaration.
- `execution` shares the exchange-backed template surface with `source`, but does not create market-series bindings.
- Matching `source` and `execution` aliases may mirror each other when the template and symbol are the same.
- Order constructors now accept named arguments in addition to the legacy positional form.
- `venue = <execution_alias>` binds an order role to a declared execution target.
- Positional and named order arguments cannot be mixed in the same constructor call.
- Trading scripts now require at least one declared `execution` target.
