# Backtesting

PalmScript exposes a deterministic backtester on top of the existing
source-aware runtime.

The backtester does not change PalmScript syntax or VM semantics. It runs a
compiled script, consumes runtime trigger events plus compiled signal-role
metadata, and simulates fills, trades, and equity for one configured execution
source.

## CLI

Run a checked-in strategy end to end:

```bash
palmscript run backtest examples/strategies/multi_strategy_backtest.palm \
  --from 1741348800000 \
  --to 1772884800000 \
  --fee-bps 10 \
  --slippage-bps 2
```

When the script declares one source, the CLI uses it as the execution source automatically. For multiple sources, pass `--execution-source <alias>`.

Additional checked-in strategy example:

```bash
palmscript run backtest examples/strategies/adaptive_trend_backtest.palm \
  --from 1741305600000 \
  --to 1772841600000
```

On Binance spot `BTCUSDT`, that window corresponds to `2025-03-07T00:00:00Z`
through `2026-03-07T00:00:00Z`. With default backtest settings
(`initial_capital=10000`, `fee_bps=5`, `slippage_bps=2`), the checked-in
strategy produced:

- `ending_equity = 12057.12`
- `total_return = 20.57%`
- `trade_count = 41`
- `max_drawdown = 1959.38`

Those numbers are an example snapshot, not a promise of future performance.

## Rust API

Use `run_backtest_with_sources` from the library crate:

```rust
use palmscript::{
    compile, run_backtest_with_sources, BacktestConfig, Interval, SourceFeed, SourceRuntimeConfig,
    VmLimits,
};

let source = r#"
interval 1m
source spot = binance.spot("BTCUSDT")
entry long = spot.close > spot.close[1]
exit long = spot.close < spot.close[1]
plot(spot.close)
"#;

let compiled = compile(source).expect("script compiles");
let runtime = SourceRuntimeConfig {
    base_interval: Interval::Min1,
    feeds: vec![SourceFeed {
        source_id: 0,
        interval: Interval::Min1,
        bars: vec![],
    }],
};
let result = run_backtest_with_sources(
    &compiled,
    runtime,
    VmLimits::default(),
    BacktestConfig {
        execution_source_alias: "spot".to_string(),
        initial_capital: 10_000.0,
        fee_bps: 5.0,
        slippage_bps: 2.0,
    },
)
.expect("backtest succeeds");

println!("ending equity = {}", result.summary.ending_equity);
```

The result includes:

- raw runtime `outputs`
- per-fill records in `fills`
- closed round trips in `trades`
- per-bar account marks in `equity_curve`
- aggregate metrics in `summary`
- any still-open position in `open_position`

## Signal Resolution

Preferred v1 surface:

- `entry long = ...`
- `exit long = ...`
- `entry short = ...`
- `exit short = ...`

Legacy compatibility bridge:

- if no first-class signal declarations are present, the backtester falls back to trigger names `long_entry`, `long_exit`, `short_entry`, and `short_exit`
- if no entry signals are present after resolution, backtest startup fails validation
- ordinary `trigger` declarations remain available for non-backtest consumers

## Execution Model

V1 uses intentionally simple deterministic execution rules:

- fills occur only on the next execution-source base bar with `bar.time >
  signal_time`
- buy-side fills use `open * (1 + slippage_bps / 10_000)`
- sell-side fills use `open * (1 - slippage_bps / 10_000)`
- fees are charged per fill using `fee_bps`
- only one net position is supported: `flat`, `long`, or `short`
- same-side re-entry is ignored
- opposite entry reverses on the same eligible open by closing first and then
  opening the new side
- open positions are marked to market on the execution-source close and are not
  force-closed at the end of the run

## Current Scope

Not included in V1:

- stop or limit orders
- partial fills
- leverage beyond the implicit 1x gross-notional model
- funding, borrow fees, or liquidation logic
