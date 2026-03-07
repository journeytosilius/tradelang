# Grammar

This page is the normative grammar for PalmScript as implemented in this repository.

The productions below define the accepted parser surface. Rules that depend on name resolution, interval ordering, or typing are defined in later reference chapters.

## Program

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

## Statements

```text
stmt                   ::= let_stmt
                         | const_stmt
                         | input_stmt
                         | export_stmt
                         | trigger_stmt
                         | signal_stmt
                         | if_stmt
                         | expr_stmt

let_stmt               ::= "let" ident "=" expr
                         | "let" "(" ident ("," ident)+ ")" "=" expr
const_stmt             ::= "const" ident "=" expr
input_stmt             ::= "input" ident "=" expr
export_stmt            ::= "export" ident "=" expr
trigger_stmt           ::= "trigger" ident "=" expr
signal_stmt            ::= "entry" signal_side "=" expr
                         | "exit" signal_side "=" expr
signal_side            ::= "long" | "short"
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

## Primary Expressions

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

## Lexical Nonterminals

```text
market_field           ::= "open" | "high" | "low" | "close" | "volume" | "time"
interval               ::= one of the literals listed in [Interval Table](intervals.md)
ident                  ::= identifier token
string_literal         ::= string token
number                 ::= numeric literal token
separator              ::= newline | ";"
```

## Binding And Precedence

PalmScript parses binary operators with the following precedence, from lowest to highest:

1. `or`
2. `and`
3. `==`, `!=`, `<`, `<=`, `>`, `>=`
4. `+`, `-`
5. `*`, `/`
6. unary `-`, unary `!`
7. call `(...)`, indexing `[...]`, and source/field qualification with `.`

Operators within one precedence level associate left-to-right, except the ternary conditional which associates right-to-left.

## Required Semantic Restrictions

The grammar does not by itself make a program valid. The implementation additionally requires:

- a script must declare exactly one base `interval`
- a script must declare at least one `source`
- `interval`, `source`, `use`, `fn`, `const`, `input`, `export`, `trigger`, `entry`, and `exit` must appear only at the top level
- bare market identifiers such as `close` are rejected and market series must be source-qualified
- higher source interval references require `use <alias> <interval>`
- every `if` must have an `else`
- string literals are accepted lexically but are semantically valid only inside `source` declarations
- only identifiers may be called
- series indexing must use a non-negative integer literal or a top-level immutable numeric binding
- tuple-valued builtins must be bound with tuple destructuring before use
- `ma_type.<variant>` is the first typed enum namespace and is reserved for TA-Lib moving-average selectors
- user-defined functions are expression-bodied, top-level only, non-recursive, and may not capture surrounding `let` bindings
- user-defined functions may capture top-level immutable `const` and `input` bindings
- source, interval, scope, and type rules are enforced as described in the other `Reference` pages
