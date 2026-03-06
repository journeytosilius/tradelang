# Grammar Sketch

This is a practical sketch of the implemented grammar, not a formal parser specification.

```text
program       := item*
item          := interval_decl | use_decl | fn_decl | stmt
interval_decl := "interval" interval
use_decl      := "use" interval
fn_decl       := "fn" ident "(" params? ")" "=" expr
params        := ident ("," ident)*
stmt          := let_stmt | export_stmt | trigger_stmt | if_stmt | expr_stmt
let_stmt      := "let" ident "=" expr
export_stmt   := "export" ident "=" expr
trigger_stmt  := "trigger" ident "=" expr
if_stmt       := "if" expr block ("else" "if" expr block)* "else" block
expr_stmt     := expr
block         := "{" stmt* "}"

expr          := or_expr
or_expr       := and_expr ("or" and_expr)*
and_expr      := cmp_expr ("and" cmp_expr)*
cmp_expr      := prefix (postfix | infix expr)*
prefix        := number
              | "true"
              | "false"
              | "na"
              | ident
              | interval "." market_field
              | "-" expr
              | "!" expr
              | "(" expr ")"

postfix       := "(" args? ")"
              | "[" integer_literal "]"
args          := expr ("," expr)*
market_field  := "open" | "high" | "low" | "close" | "volume" | "time"
```
