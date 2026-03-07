# Backtesting

PalmScript exposes a deterministic backtester on top of the existing
source-aware runtime.

The backtester runs a compiled script, consumes runtime trigger events plus
compiled signal-role and order metadata, and simulates fills, orders, trades,
and equity for one configured execution source.

## CLI

Run a backtest end to end:

```bash
palmscript run backtest strategy.palm \
  --from 1741348800000 \
  --to 1772884800000 \
  --fee-bps 10 \
  --slippage-bps 2
```

When the script declares one source, the CLI uses it as the execution source automatically. For multiple sources, pass `--execution-source <alias>`.

Backtest results depend on the script, venue, time window, fees, and slippage.
Treat any performance report as strategy-specific rather than a property of the
backtester itself.

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
- order lifecycle records in `orders`
- per-fill records in `fills`
- closed round trips in `trades`
- event-centered diagnostics in `diagnostics`
- per-bar account marks in `equity_curve`
- aggregate metrics in `summary`
- any still-open position in `open_position`

The `diagnostics` payload is designed for machine analysis and LLM-driven
iteration. It currently includes:

- per-order diagnostics with signal, placement, and fill snapshots of named `export` features
- per-trade diagnostics with entry and exit snapshots, MAE, MFE, holding time, and exit classification
- aggregate summaries such as order fill rate, average bars to fill, average bars held, average MAE/MFE, and counts of signal, stop-loss, take-profit, and reversal exits

To make regime and setup context available to diagnostics, export those fields
explicitly in the strategy:

```palm
export trend_long_state = trend_long
export adaptive_bias = spot.close - kama_4h
export breakout_long_state = breakout_long
```

Those exported values are then snapshotted automatically around backtest events.

## Signal Resolution

Preferred v1 surface:

- `entry long = ...`
- `exit long = ...`
- `entry short = ...`
- `exit short = ...`

Optional execution templates:

- `order entry long = market()`
- `order exit long = stop_market(lowest(spot.low, 5)[1], trigger_ref.last)`
- `order entry short = limit(spot.close[1], tif.gtc, false)`
- `order exit short = take_profit_limit(trigger, price, tif.gtc, false, trigger_ref.mark, expire_ms)`

Legacy compatibility bridge:

- if no first-class signal declarations are present, the backtester falls back to trigger names `long_entry`, `long_exit`, `short_entry`, and `short_exit`
- if no entry signals are present after resolution, backtest startup fails validation
- ordinary `trigger` declarations remain available for non-backtest consumers

## Execution Model

The backtester stays intentionally simple and deterministic:

- the execution venue profile is inferred from the execution `source` template, for example `binance.spot`, `binance.usdm`, `hyperliquid.spot`, or `hyperliquid.perps`
- signals produced on bar `t` become active starting on the first execution-source base bar with `bar.time > signal_time`
- only one net position is supported: `flat`, `long`, or `short`
- the portfolio model remains all-in with no explicit quantity expressions
- same-side re-entry is ignored
- opposite entry reverses on the same eligible open by closing first and then
  opening the new side
- open positions are marked to market on the execution-source close and are not
  force-closed at the end of the run

Supported order constructors:

- `market()`
- `limit(price, tif, post_only)`
- `stop_market(trigger_price, trigger_ref)`
- `stop_limit(trigger_price, limit_price, tif, post_only, trigger_ref, expire_time_ms)`
- `take_profit_market(trigger_price, trigger_ref)`
- `take_profit_limit(trigger_price, limit_price, tif, post_only, trigger_ref, expire_time_ms)`

Enum namespaces:

- `tif.gtc`, `tif.ioc`, `tif.fok`, `tif.gtd`
- `trigger_ref.last`, `trigger_ref.mark`, `trigger_ref.index`

Deterministic fill rules:

- `market()`: fills on the next eligible execution-bar open; buy-side fills use `open * (1 + slippage_bps / 10_000)`, sell-side fills use `open * (1 - slippage_bps / 10_000)`, and fees are charged per fill using `fee_bps`
- `limit(...)`: fills on the first eligible bar whose range crosses the limit; the fill price is the better of `open` and `limit`
- `stop_market(...)`: triggers on the first eligible bar whose range crosses the stop; the fill price is the worse of `open` and `trigger_price`
- `take_profit_market(...)`: triggers on the first eligible bar whose range crosses the trigger; the fill price is the better of `open` and `trigger_price`
- `stop_limit(...)` and `take_profit_limit(...)`: trigger on crossing; if the opening price already satisfies the resulting limit, they fill immediately, otherwise they become resting limit orders starting from the next bar
- `tif.ioc` and `tif.fok`: evaluate only on the first eligible bar and cancel if they do not fully fill
- `tif.gtd`: expires deterministically before evaluating any execution bar at or beyond `expire_time_ms`

Venue profile notes:

- Binance Spot supports the order constructors above, but only `trigger_ref.last` on trigger orders and no `tif.gtd`
- Binance USD-M supports `trigger_ref.last` and `trigger_ref.mark`
- Hyperliquid Spot and Hyperliquid Perps currently support `tif.gtc` and `tif.ioc`, and trigger orders use `trigger_ref.mark`
- venue-incompatible orders are rejected before simulation starts

## Current Scope

Not included in V1:

- partial fills
- order book or queue-position modeling
- leverage beyond the implicit 1x gross-notional model
- funding, borrow fees, or liquidation logic
