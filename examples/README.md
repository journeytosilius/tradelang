# Examples

Run the examples from the repository root with Cargo:

```bash
cargo run --example sma
cargo run --example rsi
cargo run --example step_engine
cargo run --example multi_interval
cargo run --example monthly_trend
cargo run --example pipeline
```

Each example compiles a TradeLang script, runs it against a small OHLCV fixture,
and prints the resulting outputs.

Current examples:

- `sma`: single-interval SMA over the base `close` series
- `rsi`: single-interval RSI over the base `close` series
- `step_engine`: per-bar stepping with the single-interval `Engine` API
- `multi_interval`: daily execution using a weekly EMA signal from `1w.close`
- `monthly_trend`: weekly execution combining `1M.close` and `1d.volume`
- `pipeline`: host-managed composition where one strategy exports signals and a
  downstream strategy consumes them as external inputs
