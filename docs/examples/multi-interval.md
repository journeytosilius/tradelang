# Multi-Interval Examples

PalmScript supports interval-qualified series such as `1w.close` and `1M.close`, but only after the script declares those intervals with `use`.

## Strategy Example

```palmscript
interval 1d
use 1w

let weekly_basis = ema(1w.close, 8)

if close > weekly_basis {
    plot(1)
} else {
    plot(0)
}
```

## CLI Example

```bash
palmscript run csv examples/strategies/weekly_bias.trl \
  --bars /path/to/daily_bars.csv
```

The single CSV file is treated as the raw source. CSV mode rolls it up to the base interval and each declared `use` interval when the file contains enough complete bars.

## Key Semantics

- no partial higher-interval candle is exposed
- lower-than-base interval references are rejected
- indexed qualified series operate on their own interval clock
