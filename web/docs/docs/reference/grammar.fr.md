# Grammaire

Cette page est la grammaire normative de PalmScript.

Les productions ci-dessous definissent la surface acceptee par le parseur. Les
regles qui dependent de la resolution des noms, de l'ordre des intervalles ou
du typage sont definies dans les chapitres de reference ulterieurs.

## Programme

```text
program                ::= separator* item* EOF
item                   ::= interval_decl
                         | source_decl
                         | use_decl
                         | function_decl
                         | stmt

interval_decl          ::= "interval" interval
source_decl            ::= "source" ident "=" source_template "(" string_literal ")"
source_template        ::= ident "." ident
use_decl               ::= "use" interval
                         | "use" ident interval
function_decl          ::= "fn" ident "(" param_list? ")" "=" expr
param_list             ::= ident ("," ident)*
```

## Instructions

```text
stmt                   ::= let_stmt
                         | const_stmt
                         | input_stmt
                         | export_stmt
                         | regime_stmt
                         | trigger_stmt
                         | risk_control_stmt
                         | signal_stmt
                         | attached_exit_stmt
                         | order_stmt
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
signal_stmt            ::= "entry" signal_side "=" expr
                         | "exit" signal_side "=" expr
attached_exit_stmt     ::= "protect" signal_side "=" order_spec
                         | "target" signal_side "=" order_spec
order_stmt             ::= "order" ("entry" | "exit") signal_side "=" order_spec
signal_side            ::= "long" | "short"
order_spec             ::= "market" "(" ")"
                         | "limit" "(" expr "," expr "," expr ")"
                         | "stop_market" "(" expr "," expr ")"
                         | "stop_limit" "(" expr "," expr "," expr "," expr "," expr "," expr ")"
                         | "take_profit_market" "(" expr "," expr ")"
                         | "take_profit_limit" "(" expr "," expr "," expr "," expr "," expr "," expr ")"
if_stmt                ::= "if" expr block "else" else_tail
else_tail              ::= if_stmt
                         | block
expr_stmt              ::= expr
block                  ::= "{" separator* stmt* "}"
```

## Expressions

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

## Expressions Primaires

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

## Non-Terminales Lexicales

```text
market_field           ::= "open" | "high" | "low" | "close" | "volume" | "time"
interval               ::= one of the literals listed in [Interval Table](intervals.md)
ident                  ::= identifier token
string_literal         ::= string token
number                 ::= numeric literal token
separator              ::= newline | ";"
```

## Liaison Et Precedence

PalmScript parse les operateurs binaires avec la precedence suivante, du plus
faible au plus fort :

1. `or`
2. `and`
3. `==`, `!=`, `<`, `<=`, `>`, `>=`
4. `+`, `-`
5. `*`, `/`
6. unaire `-`, unaire `!`
7. appel `(...)`, indexation `[...]` et qualification par source / champ avec
   `.`

Les operateurs d'un meme niveau de precedence s'associent de gauche a droite,
sauf l'operateur conditionnel ternaire qui s'associe de droite a gauche.

## Restrictions Semantiques Requises

La grammaire ne suffit pas, a elle seule, a rendre un programme valide.
L'implementation exige aussi :

- un script doit declarer exactement un `interval` de base
- un script doit declarer au moins une `source`
- `interval`, `source`, `use`, `fn`, `const`, `input`, `export`, `regime`, `trigger`,
  `entry`, `exit`, `protect`, `target`, `order` et `size` ne doivent apparaitre
  qu'au top-level
- les identifiants de marche nus comme `close` sont rejetes et les series de
  marche doivent etre qualifiees par source
- les references a un intervalle source superieur exigent
  `use <alias> <interval>`
- tout `if` doit avoir un `else`
- les litteraux de chaine sont acceptes lexicalement mais ne sont semantiquement
  valides qu'a l'interieur des declarations `source`
- seuls les identifiants peuvent etre appeles
- l'indexation de series doit utiliser un litteral entier non negatif ou une
  liaison numerique immuable top-level
- les builtins a valeur tuple doivent etre lies par destructuration avant usage
- `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`,
  `position_side.<variant>` et `exit_kind.<variant>` sont des espaces de noms
  enum types
- `position.*` n'est valide qu'a l'interieur des declarations `protect` et
  `target`
- `position_event.*` est un espace de noms `series<bool>` pilote par le
  backtest
- `last_exit.*`, `last_long_exit.*` et `last_short_exit.*` sont des espaces de
  noms du dernier trade cloture pilotes par le backtest
- `entry1..3 long|short`, `target1..3 long|short` et
  `protect_after_target1..3 long|short` sont des declarations echelonnees
  valides dans la v1
- `entry long` et `target long|short` restent des alias de compatibilite pour
  la stage 1
- `cooldown long|short` et `max_bars_in_trade long|short` exigent une
  expression scalaire entiere non negative resolue a la compilation
- `size entry1..3 long|short` et `size target1..3 long|short` sont des
  declarations `size` echelonnees valides dans la v1
- les tailles d'entree echelonnees acceptent soit une fraction numerique nue
  historique, soit `capital_fraction(x)`, soit `risk_pct(pct, stop_price)`
- `size entry ...` exige une declaration `order entry ...` echelonnee
  correspondante pour le meme role
- `size target ...` exige une declaration `target ...` echelonnee
  correspondante pour le meme role
- `risk_pct(...)` n'est valide que sur les declarations de taille d'entree
  echelonnee
- les fonctions definies par l'utilisateur ont un corps d'expression, sont
  top-level uniquement, non recursives, et ne doivent pas capturer des liaisons
  `let` environnantes
- les fonctions definies par l'utilisateur peuvent capturer des liaisons
  immuables top-level `const` et `input`
- les regles de source, d'intervalle, de portee et de type sont imposees comme
  decrit dans les autres pages de `Reference`

## Note Sur `input ... optimize(...)`

La surface du parseur accepte maintenant un suffixe optionnel `optimize(...)` sur les declarations `input`. Ce suffixe peut decrire une plage entiere, une plage float ou une liste `choice`, mais il reste soumis a une validation semantique supplementaire.

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
- Execution-oriented CLI modes now require at least one declared `execution` target.
