# PalmScript

<p align="center">
  <img src="editors/vscode/images/palmscript.png" alt="PalmScript logo" width="220">
</p>

## What Is PalmScript

PalmScript is a deterministic language and VM for financial time-series
analysis, signal research, and strategy execution logic.

It is built for:

- writing indicator and signal scripts against market data
- running deterministic market replays and backtests
- optimizing inputs over historical windows
- driving local paper-trading sessions from the same compiled strategy

The runtime is batteries included:

- declare exchange-backed `source` feeds directly in the script and let the runtime fetch the data for you
- run the same strategy in market replay, backtest, walk-forward, optimization, and local paper-trading modes
- reuse exchange-backed historical downloads from a persistent local cache instead of refetching overlapping windows on every run, while automatically retrying live-edge gaps that were not fully returned yet
- use all available logical CPUs for optimize runs by default when `--workers` is omitted, with `PALMSCRIPT_OPTIMIZE_DEFAULT_WORKERS` and `PALMSCRIPT_HISTORICAL_FETCH_WORKERS` available to tune optimization workers and bounded historical fetch parallelism
- inspect typed diagnostics, trades, fills, orders, equity curves, and session state without building extra plumbing around the VM

It is also designed to reduce false confidence:

- walk-forward evaluation is built in, so strategies can be judged on rolling train/test windows instead of one lucky in-sample pass
- optimization supports holdouts, direct validation replays, saved presets, and CLI-driven parameter mutation
- diagnostics include overfitting and fragility signals such as holdout pass rates, date perturbation behavior, and parameter stability summaries

The language keeps market-data inputs and execution venues explicit:

- `source` declares market data feeds
- `execution` declares order-routing venues
- `order ...` declares how entries and exits are placed
- `hour_utc(<alias>.time)`, `weekday_utc(<alias>.time)`, and `session_utc(<alias>.time, start_hour, end_hour)` provide deterministic UTC time/session gating without hand-rolled timestamp math
- `exit` declarations can now read `position.*` for time-based or state-aware closes such as `exit long = position.bars_held >= 48`
- `trail_stop_long`, `trail_stop_short`, `break_even_long`, and `break_even_short` keep common trailing-stop and break-even price math readable inside `protect` / `target` declarations
- `current_execution()`, `select_asc`, `select_desc`, `in_top_n`, and `in_bottom_n` let portfolio scripts rank the currently evaluated execution alias and route single-leg orders dynamically with `venue = <execution_alias_expr>`

Documentation and tooling:

- Docs: <https://palmscript.dev/docs/>
- Hosted IDE: <https://palmscript.dev/>
- Docs home: <https://palmscript.dev/docs/>
- CLI reference: <https://palmscript.dev/docs/tooling/cli/>

## Language Examples

Cross-source research script:

```palmscript
interval 15m
source bn = binance.spot("BTCUSDT")
source bb = bybit.spot("BTCUSDT")

use bb 1h

let spread_bps = ((bn.close - bb.close) / bb.close) * 10000
let spread_mean = ema(spread_bps, 48)
let spread_z = zscore(spread_bps - spread_mean, 96)
let higher_trend = ema(bb.1h.close, 24)

trigger spread_extreme = spread_z > 2.0 or spread_z < -2.0

export spread_bps = spread_bps
export spread_z = spread_z
plot(spread_bps)
plot(bb.1h.close - higher_trend)
```

Execution-routed strategy with optimization metadata, higher-timeframe context,
and attached exits:

```palmscript
interval 1h
source spot = binance.spot("BTCUSDT")
execution exec = binance.spot("BTCUSDT")

use spot 4h

input fast_len = 21 optimize(int, 8, 34, 1)
input slow_len = 55 optimize(int, 21, 89, 2)
input stop_atr = 1.8 optimize(float, 1.0, 3.0, 0.2)
input target_atr = 2.6 optimize(float, 1.4, 4.0, 0.2)

let fast = ema(spot.close, fast_len)
let slow = ema(spot.close, slow_len)
let trend = ema(spot.4h.close, 34)
let atr_base = atr(spot.high, spot.low, spot.close, 14)

entry long = crossover(fast, slow) and spot.4h.close > trend
exit long = crossunder(fast, slow)
entry short = false
exit short = false

order entry long = market(venue = exec)
order exit long = market(venue = exec)
order entry short = market(venue = exec)
order exit short = market(venue = exec)

protect long = stop_market(
    trigger_price = position.entry_price - stop_atr * atr_base,
    trigger_ref = trigger_ref.last,
    venue = exec
)
target long = take_profit_market(
    trigger_price = position.entry_price + target_atr * atr_base,
    trigger_ref = trigger_ref.last,
    venue = exec
)

export trend_filter = spot.4h.close > trend
plot(fast - slow)
```

Start with these checked-in strategies:

- [cross_source_spread.ps](/mnt/4tbscratch/projects/tradelang/crates/palmscript/examples/strategies/cross_source_spread.ps): cross-source market research with explicit source aliases and spread math
- [indicator_showcase.ps](/mnt/4tbscratch/projects/tradelang/crates/palmscript/examples/strategies/indicator_showcase.ps): dense indicator tour covering `supertrend`, `anchored_vwap`, `donchian`, `percentile`, `zscore`, and `ulcer_index`
- [venue_orders_backtest.ps](/mnt/4tbscratch/projects/tradelang/crates/palmscript/examples/strategies/venue_orders_backtest.ps): backtest with explicit `execution`, named-argument orders, and attached exit flow
- [portfolio_caps_backtest.ps](/mnt/4tbscratch/projects/tradelang/crates/palmscript/examples/strategies/portfolio_caps_backtest.ps): multi-alias portfolio backtest with caps, shared-equity routing, and regime-aware exposure shaping
- [strategy.ps](/mnt/4tbscratch/projects/tradelang/crates/palmscript/examples/strategies/strategy.ps): advanced perp/spot multi-source strategy with optimizer metadata, staged entries, and mark-triggered exits
- [triiger_happy.ps](/mnt/4tbscratch/projects/tradelang/crates/palmscript/examples/strategies/triiger_happy.ps): intentionally aggressive paper-trading smoke test for fill and lifecycle churn

## CLI

Build the CLI:

```bash
cargo build --bin palmscript
```

Common commands:

```bash
# Validate a richer multi-source script
target/debug/palmscript check crates/palmscript/examples/strategies/cross_source_spread.ps

# Replay exchange-backed market data without order simulation
target/debug/palmscript run market \
  crates/palmscript/examples/strategies/cross_source_spread.ps \
  --from 1704067200000 --to 1704153600000

# Run a portfolio backtest across two execution aliases
target/debug/palmscript run backtest \
  crates/palmscript/examples/strategies/portfolio_caps_backtest.ps \
  --from 1704067200000 --to 1704153600000 \
  --execution-source left --execution-source right \
  --maker-fee-bps 2 --taker-fee-bps 5 \
  --max-volume-fill-pct 0.10

# Validate portfolio scripts that use current_execution(), rank selectors, and dynamic venue routing
target/debug/palmscript check \
  crates/palmscript/examples/strategies/portfolio_caps_backtest.ps

# Optimize a staged strategy with inline input metadata
target/debug/palmscript run optimize \
  crates/palmscript/examples/strategies/adaptive_trend_backtest.ps \
  --from 1646611200000 --to 1772841600000 \
  --train-bars 252 --test-bars 63 --step-bars 63 \
  --trials 50 --preset-out best.json

# Queue a high-churn paper session, then drive the daemon
target/debug/palmscript run paper \
  crates/palmscript/examples/strategies/triiger_happy.ps \
  --maker-fee-bps 2 --taker-fee-bps 5
target/debug/palmscript execution serve

# Read embedded docs
target/debug/palmscript docs --list
target/debug/palmscript docs --all
```

For the full command surface, use the embedded help or the CLI reference:

```bash
target/debug/palmscript --help
target/debug/palmscript run --help
target/debug/palmscript execution --help
```

Containerized paper-trading assets live under
[infra/docker/Dockerfile.paper](/mnt/4tbscratch/projects/tradelang/infra/docker/Dockerfile.paper),
[infra/docker/paper-entrypoint.sh](/mnt/4tbscratch/projects/tradelang/infra/docker/paper-entrypoint.sh),
and
[infra/docker/paper-sessions.toml](/mnt/4tbscratch/projects/tradelang/infra/docker/paper-sessions.toml).
The intended layout is:

- bundled example strategies available at `/usr/share/palmscript/strategies`
- optional custom strategies mounted at `/strategies`
- persistent execution state mounted at `/var/lib/palmscript/execution`
- paper-session config mounted at `/etc/palmscript/paper-sessions.toml`

The paper container now also serves a live monitoring UI at `/paper` on port
`8080`. It lists all persisted paper sessions, shows a single top strategy
accordion for selecting a strategy and its runs, and keeps the selected run in
one unified detail panel with real-time equity, PnL, open positions, trades,
orders, drawdown, feed health, and session logs.
Failed sessions now keep their failure message and log stream visible even
when the session never produced a first snapshot. The bundled
`paper-sessions.toml` now starts `strategy.ps`, `triiger_happy.ps`, and
`experimental/xrp_usdm_mean_reversion.ps` so the canonical multi-source
strategy, the trigger-happy smoke test, and the XRP USD-M mean-reversion paper
session run side by side. The paper daemon keeps the last closed candle armed
when an exchange temporarily returns no fresh bar for the current live append
window, then resumes once the next closed candle appears. Perp sessions now
also wait for aligned mark-price candles instead of failing immediately when
the venue lags the execution feed by one runtime window. If one persisted paper
session becomes invalid or stale, the daemon now marks only that session failed
and keeps the remaining queued/live sessions running.

The CLI, IDE server, and LSP now also emit structured JSON logs on `stderr`.
Set `PALMSCRIPT_LOG_LEVEL=debug` or `trace` when you need more detail, and set
`BETTERSTACK_SOURCE_TOKEN` plus optional `BETTERSTACK_LOGS_URL`,
`BETTERSTACK_TIMEOUT_MS`, and `PALMSCRIPT_LOG_NAME` when you want to mirror the
same events to Better Stack. Paper session logs remain available through
`paper-logs` and now include clearer transition and runtime-window update
messages for debugging live paper runs.

Build and run:

```bash
docker build -f infra/docker/Dockerfile.paper -t palmscript-paper .
docker run --rm \
  -v "$(pwd)/.paper-state:/var/lib/palmscript/execution" \
  -v "$(pwd)/infra/docker/paper-sessions.toml:/etc/palmscript/paper-sessions.toml:ro" \
  -p 8080:8080 \
  palmscript-paper
```
