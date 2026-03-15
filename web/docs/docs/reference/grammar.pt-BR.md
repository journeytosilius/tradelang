# Gramatica

Esta pagina e a gramatica normativa do PalmScript.

As producoes abaixo definem a superficie aceita pelo parser. Regras que
dependem de resolucao de nomes, ordenacao de intervalos ou tipagem sao
definidas nos capitulos de referencia posteriores.

## Programa

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

## Instrucoes

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
signal_side            ::= "long" | "short"
order_spec             ::= ident
                         | "market" "(" ")"
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

Declaracoes `module` sao apenas de nivel superior. Na v1 elas rotulam somente
os papeis `entry`, `entry2` e `entry3` para diagnosticos.

## Expressoes

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

## Expressoes Primarias

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

## Nao Terminais Lexicais

```text
market_field           ::= "open" | "high" | "low" | "close" | "volume" | "time"
interval               ::= one of the literals listed in [Interval Table](intervals.md)
ident                  ::= identifier token
string_literal         ::= string token
number                 ::= numeric literal token
separator              ::= newline | ";"
```

## Binding E Precedencia

PalmScript faz parsing de operadores binarios com a seguinte precedencia, da
menor para a maior:

1. `or`
2. `and`
3. `==`, `!=`, `<`, `<=`, `>`, `>=`
4. `+`, `-`
5. `*`, `/`
6. unario `-`, unario `!`
7. chamada `(...)`, indexacao `[...]` e qualificacao de source / campo com `.`

Operadores dentro de um mesmo nivel de precedencia associam da esquerda para a
direita, exceto o condicional ternario, que associa da direita para a esquerda.

## Restricoes Semanticas Exigidas

A gramatica por si so nao torna um programa valido. A implementacao tambem
exige:

- um script deve declarar exatamente um `interval` base
- um script deve declarar pelo menos uma `source`
- `interval`, `source`, `use`, `fn`, `const`, `input`, `export`, `regime`, `trigger`,
  `entry`, `exit`, `protect`, `target`, `order` e `size` devem aparecer apenas
  no nivel superior
- identificadores de mercado soltos como `close` sao rejeitados e series de
  mercado precisam ser qualificadas por fonte
- referencias a intervalos superiores de source exigem `use <alias> <interval>`
- todo `if` deve ter `else`
- literais string sao aceitos lexicalmente, mas so sao semanticamente validos
  dentro de declaracoes `source`
- apenas identificadores podem ser chamados
- indexacao de serie deve usar um literal inteiro nao negativo ou um binding
  numerico imutavel de nivel superior
- builtins tuple-valued devem ser associados via desestruturacao antes do uso
- `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`,
  `position_side.<variant>` e `exit_kind.<variant>` sao namespaces enum tipados
- `position.*` e valido apenas dentro de declaracoes `protect` e `target`
- `position_event.*` e um namespace `series<bool>` dirigido por backtest
- `last_exit.*`, `last_long_exit.*` e `last_short_exit.*` sao namespaces do
  ultimo trade fechado dirigidos por backtest
- `entry1..3 long|short`, `target1..3 long|short` e
  `protect_after_target1..3 long|short` sao declaracoes em estagio validas na
  v1
- `entry long` e `target long|short` continuam como aliases de compatibilidade
  para o estagio 1
- `cooldown long|short` e `max_bars_in_trade long|short` exigem uma expressao
  escalar inteira nao negativa resolvida em compilacao
- `size entry1..3 long|short` e `size target1..3 long|short` sao declaracoes
  `size` em estagio validas na v1
- `size module <name> = <expr>` tambem e valido quando `<name>` se resolve para um `module` declarado ligado a um role de entrada em estagio
- tamanhos de entrada em estagio aceitam uma fracao numerica nua legada,
  `capital_fraction(x)` ou `risk_pct(pct, stop_price)`
- `size entry ...` exige uma declaracao correspondente `order entry ...` em
  estagio para o mesmo role
- `size target ...` exige uma declaracao correspondente `target ...` em estagio
  para o mesmo role
- `risk_pct(...)` e valido apenas em declaracoes de tamanho de entrada em
  estagio
- funcoes definidas pelo usuario tem corpo de expressao, sao apenas de nivel
  superior, nao recursivas e nao podem capturar bindings `let` ao redor
- funcoes definidas pelo usuario podem capturar bindings imutaveis de nivel
  superior `const` e `input`
- regras de source, intervalo, escopo e tipo sao aplicadas como descrito nas
  outras paginas de `Reference`

## Nota Sobre `input ... optimize(...)`

A superficie do parser agora aceita um sufixo opcional `optimize(...)` em declaracoes `input`. Esse sufixo pode descrever um intervalo inteiro, um intervalo float ou uma lista `choice`, mas continua sujeito a validacao semantica adicional.

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
