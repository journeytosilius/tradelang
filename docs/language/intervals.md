# Intervals and Multi-Interval Semantics

PalmScript supports one base execution interval plus explicit higher or equal interval references.

## Declaring Intervals

Every strategy must declare exactly one base interval:

```palmscript
interval 1d
```

Every additional interval used in qualified market-series references must be declared explicitly:

```palmscript
interval 1d
use 1w
use 1M
```

The compiler rejects:

- missing `interval`
- multiple `interval` declarations
- duplicate `use` declarations
- `use` repeating the base interval
- qualified interval references that were not declared with `use`

## Qualified Market Series

The syntax is:

```palmscript
<interval>.<field>
```

Examples:

- `1w.close`
- `4h.volume`
- `1M.high`

Allowed fields:

- `open`
- `high`
- `low`
- `close`
- `volume`
- `time`

## No-Lookahead Guarantee

Higher-interval values only become visible after that higher-interval candle fully closes.

If a script runs on `interval 1m` and references `1w.close`:

- the weekly close stays fixed across the whole week
- it updates only when the weekly candle closes
- partial weekly candles are never exposed

## Equal and Lower Intervals

- referencing the base interval explicitly is allowed
- lower-than-base interval references are rejected

This keeps the runtime deterministic and avoids ambiguous downsampling semantics inside the VM.

## Indexing

Indexing composes on the referenced interval's own clock:

- `1w.close[0]` is the latest fully closed weekly close
- `1w.close[1]` is the previous weekly close
- `ema(1w.close, 5)[1]` is the prior weekly EMA sample
