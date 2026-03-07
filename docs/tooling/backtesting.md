# Backtesting

PalmScript now exposes a library-first backtester on top of the existing
source-aware runtime.

The backtester does not change PalmScript syntax or VM semantics. It runs a
compiled script, consumes trigger events from the runtime outputs, and simulates
fills, trades, and equity for one configured execution source.

## Rust API

Use `run_backtest_with_sources` from the library crate:

```rust
use palmscript::{
    compile, run_backtest_with_sources, BacktestConfig, Interval, SignalContract, SourceFeed,
    SourceRuntimeConfig, VmLimits,
};

let source = r#"
interval 1m
source spot = binance.spot("BTCUSDT")
trigger long_entry = spot.close > spot.close[1]
trigger long_exit = spot.close < spot.close[1]
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
        signals: SignalContract::default(),
    },
)
.expect("backtest succeeds");

println!("ending equity = {}", result.summary.ending_equity);
```

Checked-in end-to-end example:

```bash
cargo run --example binance_multi_strategy_backtest
```

That example loads [`examples/strategies/multi_strategy_backtest.palm`](https://github.com/journeytosilius/palmscript/blob/main/examples/strategies/multi_strategy_backtest.palm),
fetches the required Binance feeds for the last 365 days ending at the latest
closed 4-hour boundary, and prints the backtest summary plus recent trades.

The result includes:

- raw runtime `outputs`
- per-fill records in `fills`
- closed round trips in `trades`
- per-bar account marks in `equity_curve`
- aggregate metrics in `summary`
- any still-open position in `open_position`

## Default Trigger Names

`SignalContract::default()` maps these trigger names:

- `long_entry`
- `long_exit`
- `short_entry`
- `short_exit`

The backtester treats trigger names as an external contract. The compiler and VM
still treat them as ordinary named trigger outputs.

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

The backtester is library-first in this release.

Not included in V1:

- a `palmscript` CLI backtest command
- stop or limit orders
- partial fills
- leverage beyond the implicit 1x gross-notional model
- funding, borrow fees, or liquidation logic
