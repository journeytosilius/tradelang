# Examples

The canonical examples documentation now lives in the MkDocs site:

- [CLI Strategies](../docs/examples/cli-strategies.md)
- [Rust Examples](../docs/examples/rust-examples.md)
- [Multi-Interval Examples](../docs/examples/multi-interval.md)
- [Composition Examples](../docs/examples/composition.md)

This file remains a short inventory for repository browsing.

## Rust Examples

Run from the repository root:

```bash
cargo run --example sma
cargo run --example rsi
cargo run --example step_engine
cargo run --example multi_interval
cargo run --example monthly_trend
cargo run --example pipeline
```

## CLI Strategies

Checked-in `.trl` strategies live under `examples/strategies/`.

Common commands:

```bash
./tradelang check examples/strategies/sma_cross.trl
./tradelang run csv examples/strategies/sma_cross.trl --bars examples/data/minute_bars.csv
./tradelang run csv examples/strategies/volume_breakout.trl --bars examples/data/minute_bars.csv --format text
./tradelang run csv examples/strategies/weekly_bias.trl --bars examples/data/daily_bars.csv
```
