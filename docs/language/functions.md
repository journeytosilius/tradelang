# Functions

TradeLang supports top-level user-defined functions.

## Syntax

```tradelang
fn crossover(a, b) = a > b and a[1] <= b[1]
```

Functions are expression-bodied and declared at the top level.

## Compilation Model

Functions are compiled through specialization and inlining. This preserves determinism and avoids adding a dynamic function call mechanism to the VM hot path.

The compiler rejects:

- duplicate function names
- duplicate parameter names
- recursive or cyclic function graphs
- unsupported captures of local state

## Interval-Aware Specialization

Series arguments are not treated as interchangeable if they advance on different clocks. A function specialized for base-interval `close` is distinct from a specialization fed by a slower series such as `1w.close`.
