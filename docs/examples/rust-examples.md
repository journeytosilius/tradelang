# Rust Examples

Rust examples live under `examples/` and show how to embed the library directly.

Run them from the repository root:

```bash
cargo run --example sma
cargo run --example rsi
cargo run --example step_engine
cargo run --example multi_interval
cargo run --example monthly_trend
cargo run --example pipeline
```

## What They Cover

- `sma`: single-interval SMA output
- `rsi`: RSI calculation
- `step_engine`: explicit per-bar stepping with the engine
- `multi_interval`: higher-interval data usage
- `monthly_trend`: mixed monthly and daily context
- `pipeline`: host-managed composition
