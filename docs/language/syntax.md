# Syntax and Lexical Structure

PalmScript source files are sequences of top-level items. The parser recognizes:

- `interval <interval>`
- `use <interval>`
- `fn name(params...) = expr`
- `let name = expr`
- `export name = expr`
- `trigger name = expr`
- `if / else if / else`
- expression statements such as `plot(close)`

## Statement Separation

Statements are separated by newlines or semicolons. Newlines inside parentheses or brackets do not terminate a statement.

```palmscript
let x = close;
plot(
    sma(close, 5)
)
```

## Comments

Only single-line comments are supported:

```palmscript
// trend regime
let fast = ema(close, 5)
```

## Identifiers

Identifiers start with a letter or `_`, then continue with letters, digits, or `_`.

Examples:

- `trend`
- `_tmp1`
- `weekly_basis`

## Reserved Keywords

- `fn`
- `let`
- `interval`
- `use`
- `export`
- `trigger`
- `if`
- `else`
- `and`
- `or`
- `true`
- `false`
- `na`

## Numbers

Numeric literals are parsed as `f64`.

Supported:

- `1`
- `14`
- `3.5`

Not supported:

- exponent notation such as `1e6`
- leading-dot literals such as `.5`

Negative values are written with unary `-`.

## Valid Interval Literals

PalmScript currently supports all Binance kline intervals, case-sensitive:

- `1s`
- `1m`
- `3m`
- `5m`
- `15m`
- `30m`
- `1h`
- `2h`
- `4h`
- `6h`
- `8h`
- `12h`
- `1d`
- `3d`
- `1w`
- `1M`

Important:

- weekly is `1w`
- monthly is `1M`
- `1W` is invalid
