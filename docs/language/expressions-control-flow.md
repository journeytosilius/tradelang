# Expressions and Control Flow

PalmScript supports arithmetic, comparisons, logical operators, unary operators, indexing, function calls, and conditional blocks.

## Operators

Implemented operators include:

- arithmetic: `+`, `-`, `*`, `/`
- comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`
- logical: `and`, `or`
- unary: `-`, `!`

## Indexing

Series indexing requires a literal offset:

```palmscript
close[1]
ema(close, 14)[2]
1w.volume[3]
```

Dynamic indexing is not supported.

## Conditionals

PalmScript supports `if`, `else if`, and `else`:

```palmscript
if close > sma(close, 20) {
    plot(1)
} else if close > sma(close, 50) {
    plot(0.5)
} else {
    plot(0)
}
```

Conditionals are statements, not expressions. They are used to control emitted outputs and temporary bindings rather than to produce inline ternary-style values.
