# Lexical Structure

PalmScript source is tokenized before parsing. The lexer must preserve source order and spans, and it must reject unknown characters and invalid interval literals.

## Keywords

The following tokens are reserved keywords:

- `fn`
- `let`
- `interval`
- `source`
- `use`
- `const`
- `input`
- `optimize`
- `export`
- `regime`
- `trigger`
- `cooldown`
- `max_bars_in_trade`
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

These keywords must not be used where an identifier is required.

## Identifiers

An identifier:

- must begin with an ASCII letter or `_`
- may continue with ASCII letters, digits, or `_`

Examples:

- `trend`
- `_tmp1`
- `weekly_basis`

## Literals

### Number literals

Numeric literals are parsed as `f64`.

Accepted forms:

- `1`
- `14`
- `3.5`

Rejected forms:

- exponent notation such as `1e6`
- leading-dot forms such as `.5`

Negative numbers are expressed through unary `-`, not a separate signed literal token.

### Boolean literals

Boolean literals are:

- `true`
- `false`

### Missing-value literal

`na` is the missing-value literal.

### String literals

String literals are currently accepted only where the grammar permits them in source declarations.

They:

- are delimited by `"`
- may contain basic escapes for `"`, `\`, `\n`, `\r`, and `\t`
- must not span an unescaped newline

## Comments

Only single-line comments are supported:

```palmscript
// trend regime
let fast = ema(spot.close, 5)
```

A standalone `/` token is the arithmetic division operator. `//` starts a
single-line comment.

## Statement separators

Statements are separated by:

- newlines
- semicolons

Newlines inside parentheses or brackets do not terminate a statement.

## Interval literals

Interval literals are case-sensitive. The accepted set is defined in [Interval Table](intervals.md).

For example:

- `1w` is valid
- `1M` is valid
- `1W` is invalid
