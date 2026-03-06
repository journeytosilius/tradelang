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
./palmscript check examples/strategies/sma_cross.trl
./palmscript run csv examples/strategies/sma_cross.trl --bars examples/data/minute_bars.csv
./palmscript run csv examples/strategies/volume_breakout.trl --bars examples/data/minute_bars.csv --format text
./palmscript run csv examples/strategies/weekly_bias.trl --bars /path/to/daily_bars.csv
```
