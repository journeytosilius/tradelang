# 文法

このページは PalmScript の規範的文法です。

以下の生成規則は、受理される parser surface を定義します。名前解決、インターバル順序、型付けに依存する規則は後続の reference 章で定義されます。

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

## 束縛と優先順位

PalmScript は二項演算子を、低いものから高いものへ次の優先順位で解析します。

1. `or`
2. `and`
3. `==`, `!=`, `<`, `<=`, `>`, `>=`
4. `+`, `-`
5. `*`, `/`
6. 単項 `-`, 単項 `!`
7. 呼び出し `(...)`、インデックス `[...]`、`.` によるソース / フィールド修飾

同じ優先順位内の演算子は左結合ですが、三項条件演算子だけは右結合です。

## 必須の意味論的制限

文法だけではプログラムの妥当性は決まりません。実装は追加で次を要求します。

- スクリプトはちょうど一つのベース `interval` を宣言しなければならない
- スクリプトは少なくとも一つの `source` を宣言しなければならない
- `interval`、`source`、`use`、`fn`、`const`、`input`、`export`、`regime`、`trigger`、`entry`、`exit`、`protect`、`target`、`order`、`size` はトップレベルにのみ現れなければならない
- `close` のような裸の市場識別子は拒否され、市場シリーズはソース修飾されていなければならない
- 上位ソースインターバル参照には `use <alias> <interval>` が必要
- すべての `if` は `else` を持たなければならない
- 文字列リテラルは字句的には受理されるが、意味的に有効なのは `source` 宣言内だけ
- 呼び出せるのは識別子だけ
- シリーズインデックスには非負整数リテラルまたはトップレベル不変数値束縛が必要
- タプル値 builtin は、使用前にタプル分解で束縛しなければならない
- `ma_type.<variant>`、`tif.<variant>`、`trigger_ref.<variant>`、`position_side.<variant>`、`exit_kind.<variant>` は型付き enum 名前空間
- `position.*` は `protect` と `target` 宣言内でのみ有効
- `position_event.*` はバックテスト駆動の `series<bool>` 名前空間
- `last_exit.*`、`last_long_exit.*`、`last_short_exit.*` はバックテスト駆動の最新クローズトレード名前空間
- `entry1..3 long|short`, `target1..3 long|short`, `protect_after_target1..3 long|short` は v1 の有効な staged 宣言
- `entry long` と `target long|short` は stage 1 の互換エイリアスのまま
- `cooldown long|short` と `max_bars_in_trade long|short` はコンパイル時に解決される非負整数スカラー式を必要とする
- `size entry1..3 long|short` と `size target1..3 long|short` は v1 の有効な staged `size` 宣言
- staged entry size は、旧来の裸の数値比率、`capital_fraction(x)`、または `risk_pct(pct, stop_price)` を受け付ける
- `size entry ...` には、同じロールの対応する staged `order entry ...` 宣言が必要
- `size target ...` には、同じロールの対応する staged `target ...` 宣言が必要
- `risk_pct(...)` は staged entry size 宣言でのみ有効
- ユーザー定義関数は式本体・トップレベル専用・非再帰であり、周囲の `let` 束縛をキャプチャしてはならない
- ユーザー定義関数はトップレベルの不変 `const` と `input` 束縛をキャプチャできる
- ソース、インターバル、スコープ、型のルールは、他の `Reference` ページに記述されたとおりに強制される

## `input ... optimize(...)` について

パーサー表面は、`input` 宣言の末尾に任意の `optimize(...)` 接尾辞を受け入れるようになりました。この接尾辞は整数範囲、float 範囲、または `choice` の列を表現できますが、追加の意味検証は引き続き必要です。
