# Gramatica

Esta pagina es la gramatica normativa de PalmScript.

Las producciones de abajo definen la superficie aceptada por el parser. Las
reglas que dependen de resolucion de nombres, orden de intervalos o tipos se
definen en capitulos posteriores de referencia.

## Programa

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

## Sentencias

```text
stmt                   ::= let_stmt
                         | const_stmt
                         | input_stmt
                         | export_stmt
                         | regime_stmt
                         | trigger_stmt
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

## Expresiones

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

## Expresiones Primarias

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

## No Terminales Lexicos

```text
market_field           ::= "open" | "high" | "low" | "close" | "volume" | "time"
interval               ::= one of the literals listed in [Interval Table](intervals.md)
ident                  ::= identifier token
string_literal         ::= string token
number                 ::= numeric literal token
separator              ::= newline | ";"
```

## Bindings Y Precedencia

PalmScript parsea operadores binarios con la siguiente precedencia, de menor a
mayor:

1. `or`
2. `and`
3. `==`, `!=`, `<`, `<=`, `>`, `>=`
4. `+`, `-`
5. `*`, `/`
6. `-` unario, `!` unario
7. llamada `(...)`, indexacion `[...]` y calificacion de fuente/campo con `.`

Los operadores dentro de un mismo nivel de precedencia se asocian de izquierda
a derecha, excepto el condicional ternario, que se asocia de derecha a
izquierda.

## Restricciones Semanticas Requeridas

La gramatica por si sola no vuelve valido a un programa. La implementacion
ademas exige:

- un script debe declarar exactamente un `interval` base
- un script debe declarar al menos una `source`
- `interval`, `source`, `use`, `fn`, `const`, `input`, `export`, `regime`, `trigger`,
  `entry`, `exit`, `protect`, `target`, `order` y `size` deben aparecer solo
  en el nivel superior
- identificadores de mercado desnudos como `close` se rechazan y las series de
  mercado deben estar calificadas por fuente
- las referencias a intervalos superiores requieren `use <alias> <interval>`
- todo `if` debe tener un `else`
- los literales string se aceptan lexicamente pero son semanticamente validos
  solo dentro de declaraciones `source`
- solo los identificadores pueden invocarse
- la indexacion de series debe usar un literal entero no negativo o un binding
  numerico inmutable de nivel superior
- los builtins que devuelven tuplas deben enlazarse con destructuracion de
  tupla antes de usarse
- `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`,
  `position_side.<variant>` y `exit_kind.<variant>` son namespaces de enums
  tipados
- `position.*` es valido solo dentro de declaraciones `protect` y `target`
- `position_event.*` es un namespace `series<bool>` impulsado por backtests
- `last_exit.*`, `last_long_exit.*` y `last_short_exit.*` son namespaces del
  ultimo trade cerrado impulsados por backtests
- `entry1..3 long|short`, `target1..3 long|short` y
  `protect_after_target1..3 long|short` son declaraciones escalonadas validas
  en v1
- `entry long` y `target long|short` siguen siendo aliases de compatibilidad
  para la etapa 1
- `size entry1..3 long|short` y `size target1..3 long|short` son declaraciones
  `size` escalonadas validas en v1
- los tamanos de entrada escalonada aceptan una fraccion numerica legacy,
  `capital_fraction(x)` o `risk_pct(pct, stop_price)`
- `size entry ...` requiere una declaracion `order entry ...` escalonada para
  el mismo rol
- `size target ...` requiere una declaracion `target ...` escalonada para el
  mismo rol
- `risk_pct(...)` solo es valido en declaraciones de tamano de entrada
  escalonada
- las funciones definidas por el usuario tienen cuerpo de expresion, existen
  solo en el nivel superior, no son recursivas y no pueden capturar bindings
  `let` circundantes
- las funciones definidas por el usuario si pueden capturar bindings inmutables
  `const` e `input` de nivel superior
- las reglas de fuente, intervalo, scope y tipos se aplican tal como se
  describen en las otras paginas de `Reference`

## Nota Sobre `input ... optimize(...)`

La superficie del parser ahora acepta un sufijo opcional `optimize(...)` en declaraciones `input`. Ese sufijo puede describir un rango entero, un rango float o una lista `choice`, pero sigue sujeto a validacion semantica adicional.
