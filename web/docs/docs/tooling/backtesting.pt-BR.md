# Backtesting

Esta pagina voltou a ficar disponivel publicamente porque o PalmScript agora e open source. A localizacao completa sera publicada em uma atualizacao posterior. Enquanto isso, o conteudo canonico em ingles aparece abaixo para que esta versao do site exponha a mesma superficie publica de CLI e ferramentas.

## English Canonical Content

# Backtesting

PalmScript exposes a deterministic backtester on top of the existing
source-aware runtime.

The backtester runs a compiled script, consumes runtime trigger events plus
compiled signal-role and order metadata, and simulates fills, orders, trades,
and equity for one configured execution source.

PalmScript also exposes a walk-forward layer on top of the same deterministic
backtester. Walk-forward reuses the existing fill semantics and venue rules,
but evaluates the strategy over rolling train/test windows and stitches only
the out-of-sample slices together.

PalmScript also exposes a bounded walk-forward sweep layer that ranks explicit
numeric `input` grids by stitched out-of-sample performance.

PalmScript also exposes a first-class optimizer layer that performs seeded,
bounded hyper-parameter search over selected numeric `input`s.

PalmScript also supports declarative backtest controls for common safety
policies:

- `cooldown long|short = <bars>` blocks same-side re-entry for a fixed number of execution bars after a full exit
- `max_bars_in_trade long|short = <bars>` forces a same-side market exit at the next execution open once the position has been held for the declared number of execution bars
- `max_positions = <N>`, `max_long_positions = <N>`, and `max_short_positions = <N>` cap simultaneous alias positions when portfolio mode is active
- `max_gross_exposure_pct = <N>` and `max_net_exposure_pct = <N>` cap shared-equity exposure when portfolio mode is active
- `portfolio_group "name" = [alias1, alias2, ...]` declares a named alias bucket for diagnostics and future group-scoped controls

Scripts de trading tambem podem rotular papeis de entrada para atribuicao por
modulo:

- `module breakout = entry long`
- `module pullback = entry2 long`

Esses rotulos aparecem nos diagnosticos como `entry_module` e
`by_entry_module`.

## CLI

Run a backtest end to end:

```bash
palmscript run backtest strategy.ps \
  --from 1741348800000 \
  --to 1772884800000 \
  --maker-fee-bps 2 \
  --taker-fee-bps 5 \
  --slippage-bps 2
```

Diagnostics detail can now be configured explicitly:

```bash
palmscript run backtest strategy.ps \
  --from 1741348800000 \
  --to 1772884800000 \
  --maker-fee-bps 2 \
  --taker-fee-bps 5 \
  --diagnostics full-trace
```

Modes:

- `--diagnostics summary` keeps the default compact diagnostics payload
- `--diagnostics full-trace` adds one typed per-bar decision trace record for each execution bar

Trading scripts require at least one declared `execution` alias. When the script declares exactly one `execution` alias, the CLI uses it as the execution target automatically. Otherwise pass `--execution-source <alias>`. Repeat `--execution-source` to activate portfolio mode across multiple execution aliases. `execution` declarations stay separate from `source` declarations, so cross-source strategies can still route orders onto one venue.

Every executable inline order and every `order_template` must declare `venue = <execution_alias>` explicitly, even when the script declares only one execution target.

Fee modeling now requires explicit global maker/taker inputs for execution-oriented runs. Pass `--maker-fee-bps` and `--taker-fee-bps` on every backtest, walk-forward, walk-forward-sweep, optimize, or paper invocation, and repeat `--fee-schedule <alias:maker:taker>` when one selected execution alias should use a different fee tier.

Execution-routed order example:

```palmscript
interval 1h
source left = binance.spot("BTCUSDT")
source right = bybit.spot("BTCUSDT")
execution exec = bybit.spot("BTCUSDT")

entry long = left.close > right.close
order entry long = limit(price = right.close, tif = tif.gtc, post_only = false, venue = exec)
```

Portfolio mode example:

```bash
palmscript run backtest portfolio_caps_backtest.ps \
  --from 1741348800000 \
  --to 1772884800000 \
  --execution-source left \
  --execution-source right \
  --maker-fee-bps 2 \
  --taker-fee-bps 5 \
  --slippage-bps 2
```

Portfolio mode now seeds one explicit ledger per selected execution alias from `initial_capital`. Spot aliases keep quote/base balances per venue, while USD-M aliases keep quote-collateral balances plus isolated-margin positions. Without `--spot-virtual-rebalance`, multi-venue spot entries can only spend the local quote balance already sitting on that alias. Pass `--spot-virtual-rebalance` when every selected execution alias is spot and you want PalmScript to transfer quote between those spot venue ledgers automatically before long entries. That virtual-rebalance mode is spot-only and long/flat-only in v1. Entry-cap declarations such as `max_positions` and `max_gross_exposure_pct` only block new entries; they do not shrink orders or force exits after the portfolio is already open.

O modo portfolio agora tambem dirige o runtime v1 de cestas de arbitragem. Quando um script declara `arb_entry`, `arb_exit` e `arb_order entry|exit = market_pair(...)`, o PalmScript executa uma perna de compra e uma de venda entre os aliases spot selecionados na proxima abertura de barra. Na v1, o primeiro alias de portfolio selecionado atua como runtime controlador, `size = ...` e interpretado como quantidade do ativo base, e `limit_pair(...)` / `mixed_pair(...)` ainda falham em runtime.

Esse mesmo runtime controlador agora tambem avalia `transfer quote = quote_transfer(...)`. Na v1, a quote de origem e debitada na proxima abertura de barra e o destino recebe o credito apos `delay_bars`. `transfer base = base_transfer(...)` permanece reservado, mas ainda falha em runtime.

Os payloads de resultado orientados a backtest agora tambem resumem essa mecanica de portfolio explicitamente. `run backtest` expõe secoes tipadas `arbitrage` e `transfer_summary`, `run walk-forward` carrega os mesmos dados nos stitched e holdout windows, e `run optimize --direct-validate-top` inclui os mesmos resumos em cada replay direto sobrevivente.

Backtest results depend on the script, venue, time window, fees, and slippage.
Treat any performance report as strategy-specific rather than a property of the
backtester itself.

Perp execution sources also accept isolated-margin controls:

```bash
palmscript run backtest strategy.ps \
  --from 1741348800000 \
  --to 1772884800000 \
  --maker-fee-bps 2 \
  --taker-fee-bps 5 \
  --execution-source perp \
  --leverage 3 \
  --margin-mode isolated
```

Run a rolling walk-forward evaluation:

```bash
palmscript run walk-forward strategy.ps \
  --from 1741348800000 \
  --to 1772884800000 \
  --maker-fee-bps 2 \
  --taker-fee-bps 5 \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --min-trades 30 \
  --min-sharpe 1.0 \
  --max-zero-trade-segments 1
```

V1 notes:

- walk-forward uses the same deterministic order/fill engine as ordinary backtests
- each segment uses the leading `train-bars` as in-sample context and reports the trailing `test-bars` as out-of-sample
- `step-bars` controls how far each segment advances
- `--min-trades`, `--min-sharpe`, and `--max-zero-trade-segments` let you reject stitched results that are too sparse, too weak on risk-adjusted return, or too inactive
- v1 does not optimize parameters automatically; it evaluates the fixed script and inputs you passed in

Run a bounded walk-forward sweep:

```bash
palmscript run walk-forward-sweep strategy.ps \
  --from 1741348800000 \
  --to 1772884800000 \
  --maker-fee-bps 2 \
  --taker-fee-bps 5 \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --set fast_len=13,21,34 \
  --set target_atr_mult=2.0,2.5,3.0 \
  --objective total-return \
  --top 5
```

V1 sweep notes:

- sweeps only override numeric `input` declarations
- each candidate recompiles the same script with one explicit override combination
- each candidate reuses the same fetched runtime data and deterministic walk-forward engine
- the explicit candidate grid is bounded to `10000` combinations
- stitched OOS ranking supports `total-return`, `ending-equity`, and `return-over-drawdown`

Run seeded optimization:

```bash
palmscript run optimize strategy.ps \
  --from 1741348800000 \
  --to 1772884800000 \
  --maker-fee-bps 2 \
  --taker-fee-bps 5 \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --min-trades 30 \
  --min-sharpe 1.0 \
  --min-holdout-trades 10 \
  --require-positive-holdout \
  --max-zero-trade-segments 1 \
  --min-holdout-pass-rate 0.5 \
  --min-date-perturbation-positive-ratio 0.67 \
  --min-date-perturbation-outperform-ratio 0.67 \
  --max-overfitting-risk moderate \
  --objective robust-return \
  --trials 50 \
  --top 5 \
  --direct-validate-top 3 \
  --preset-out /tmp/adaptive-best.json
```

V1 optimizer notes:

- optimizer tuning is restricted to declared numeric `input`s
- parameter-space precedence is explicit `--param`, then preset parameter space, then inferred `input ... optimize(...)` metadata from the script
- integer and float CLI ranges accept optional step syntax: `int:name=low:high[:step]` and `float:name=low:high[:step]`
- scripts can declare search metadata directly with `optimize(int, ...)`, `optimize(float, ...)`, or `optimize(choice, ...)` on numeric `input`s
- walk-forward is the default runner; `--runner backtest` is optional
- by default, walk-forward optimize reserves a final untouched holdout window equal to `test-bars`; use `--holdout-bars <N>` to change it or `--no-holdout` to disable it explicitly
- `--min-trades`, `--min-sharpe`, `--min-holdout-trades`, `--require-positive-holdout`, `--max-zero-trade-segments`, `--min-holdout-pass-rate`, `--min-date-perturbation-positive-ratio`, `--min-date-perturbation-outperform-ratio`, and `--max-overfitting-risk` let you reject fragile candidates before they can rank as survivors
- the search is seeded and deterministic for the same script, seed, and search space
- `--workers` only controls bounded parallel evaluation
- `--preset-out` writes a reusable preset containing the best overrides and top candidates
- `run backtest` and `run walk-forward` now accept `--preset-trial-id <N>` so one saved top candidate can be replayed directly, and `--set name=value` can mutate that survivor without editing the preset file
- `--direct-validate-top <N>` reruns that many top feasible validated survivors as full-window backtests so stitched and direct metrics can be reviewed together
- `walk-forward-sweep` remains the explicit grid-search baseline tool
- the final result now reports a separate holdout summary so the winning candidate is checked on unseen tail data before you trust the tuned output
- the optimizer now revalidates the ranked survivor set with holdout, date-perturbation, and overfitting diagnostics before the winner is chosen
- if at least one validated candidate is feasible, only feasible candidates can win; if none are feasible, PalmScript returns the best infeasible fallback plus its violations
- the final optimize result now also reports validation-constraint summaries, validated/feasible/infeasible candidate counts, constraint-failure breakdowns, optional direct-validation survivor replays with stitched-vs-direct drift, holdout drift, top-candidate holdout robustness, holdout pass rate, parameter stability ranges, baseline comparisons, Sharpe summaries, explicit overfitting-risk summaries, and machine-readable improvement hints

Run optimize in the foreground when you want a direct result:

```bash
palmscript run optimize strategy.ps \
  --from 1741348800000 \
  --to 1772884800000 \
  --maker-fee-bps 2 \
  --taker-fee-bps 5 \
  --train-bars 252 \
  --test-bars 63 \
  --step-bars 63 \
  --objective robust-return \
  --trials 50 \
  --top 5 \
  --preset-out /tmp/adaptive-best.json
```

Foreground optimize notes:

- `run optimize` reuses the same optimizer config model shown above
- when `--param` is omitted, optimize uses the same preset-or-input-metadata inference path as any other CLI optimize run
- every trial still respects bounded workers, deterministic seeding, and the final untouched holdout when holdout protection is enabled
- `--preset-out` exports the best known preset from the completed search so you can rerun it immediately in backtest or walk-forward mode
- use `--preset-trial-id <N>` when you want an exact saved survivor instead of the preset best candidate, and add `--set name=value` to test small mutations on top of it
- the final result includes the holdout summary when holdout protection is enabled

## Default Safety Profile

PalmScript cannot mathematically prevent overfitting, but the CLI now applies a safer default optimize workflow:

- `run optimize` defaults to walk-forward evaluation
- walk-forward optimize now reserves a final untouched holdout window by default
- only the pre-holdout history participates in candidate ranking
- after ranking, the best candidate is rerun with full pre-holdout context and scored separately on the untouched tail

This does not replace paper trading or live forward validation, but it does make the default tuning workflow less likely to confuse in-sample fitting with genuinely unseen performance.

## Local Paper Execution

PalmScript now also exposes a first-class local paper mode on top of the same VM and deterministic order simulator:

```bash
palmscript run paper strategy.ps --execution-source exec --maker-fee-bps 2 --taker-fee-bps 5
palmscript execution serve
```

Execution v1 is intentionally conservative:

- paper only
- local daemon only
- closed-bar VM evaluation
- shared armed warmup history plus top-of-book / last / mark quote snapshots across active paper sessions
- persistent local session state
- no real API keys or live order placement

The paper daemon warms the VM with enough pre-session history to satisfy the compiled history requirements, but suppresses fills, orders, and diagnostics before the paper session activation time. That keeps indicator state realistic without leaking fake pre-session trades into the persistent paper ledger.

Paper snapshots now also expose feed readiness state plus live quote state per execution alias:

- best bid / best ask
- derived mid price
- last price where the venue exposes it
- mark price for perp venues where the venue exposes it

That quote layer does not change PalmScript’s bar-close VM semantics, but it does let the paper session report live mid/mark valuation for open positions and surface degraded feed health when quote snapshots go stale.

Paper mode reuses the same:

- compiled PalmScript program
- runtime interval-close semantics
- backtest order/fill rules
- venue validation
- portfolio caps and shared-equity behavior
- cooldown and `max_bars_in_trade` controls

## Diagnostics Output

Backtest, walk-forward, and optimize results now expose richer machine-readable diagnostics on top of the existing order/trade summaries:

- cohort summaries by side, exit classification, weekday/hour UTC, fixed 4-hour UTC time buckets, holding-time bucket, active regime state, and active exported bool state
- drawdown duration and stagnation diagnostics
- baseline comparisons against flat cash and execution-asset buy-and-hold, plus bounded date-perturbation reruns on top-level backtests
- source alignment diagnostics that show degraded bars and synthetic supplemental updates
- deterministic overfitting-risk summaries and improvement hints such as `too_few_trades`, `holdout_collapse`, and `signal_quality_weak`
- typed validation-constraint summaries for walk-forward and optimize runs, including best-candidate, holdout, and top-level aggregate constraint status
- optional per-bar decision traces when `--diagnostics full-trace` is enabled

## Declarative Risk Controls

PalmScript can express two common backtest guardrails directly in the script:

```palmscript
cooldown long = 6
max_bars_in_trade long = 48
```

Rules:

- both declarations are top-level only
- both declarations currently require a compile-time non-negative whole-number scalar expression
- `cooldown` is side-specific and applies after a full close on that side
- `max_bars_in_trade` is side-specific and exits at the next execution-bar open when the limit is reached
- the forced exit is reported as a signal-style exit so `last_exit.*` and `position_event.*` stay deterministic

PalmScript can also express shared-equity portfolio caps directly in the script:

```palmscript
portfolio_group "majors" = [left, right]
max_positions = 2
max_long_positions = 2
max_short_positions = 0
max_gross_exposure_pct = 0.8
max_net_exposure_pct = 0.8
```

Additional rules:

- portfolio controls are top-level only
- count controls require a compile-time non-negative whole-number scalar expression
- exposure controls require a compile-time non-negative finite numeric scalar expression
- `portfolio_group` aliases must refer to declared `source` aliases
- portfolio controls only matter when multiple `--execution-source` aliases activate portfolio mode
- blocked portfolio entries are surfaced in summary diagnostics, full-trace decision reasons, and JSON output

## Rust API

Use `run_backtest_with_sources` from the library crate:

```rust
use palmscript::{
    compile, run_backtest_with_sources, BacktestConfig, DiagnosticsDetailMode, Interval,
    SourceFeed, SourceRuntimeConfig, VmLimits,
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
        portfolio_execution_aliases: Vec::new(),
        spot_virtual_rebalance: false,
        initial_capital: 10_000.0,
        maker_fee_bps: 2.0,
        taker_fee_bps: 5.0,
        execution_fee_schedules: std::collections::BTreeMap::new(),
        slippage_bps: 2.0,
        max_volume_fill_pct: Some(0.10),
        diagnostics_detail: DiagnosticsDetailMode::SummaryOnly,
        perp: None,
        perp_context: None,
        portfolio_perp_contexts: std::collections::BTreeMap::new(),
    },
)
.expect("backtest succeeds");

println!("ending equity = {}", result.summary.ending_equity);
```

Use `run_walk_forward_with_sources` for rolling train/test evaluation:

```rust
use palmscript::{
    compile, run_walk_forward_with_sources, BacktestConfig, Interval, SourceFeed,
    SourceRuntimeConfig, VmLimits, WalkForwardConfig,
};

let compiled = compile(source).expect("script compiles");
let runtime = SourceRuntimeConfig {
    base_interval: Interval::Min1,
    feeds: vec![SourceFeed {
        source_id: 0,
        interval: Interval::Min1,
        bars: vec![],
    }],
};
let result = run_walk_forward_with_sources(
    &compiled,
    runtime,
    VmLimits::default(),
    WalkForwardConfig {
        backtest: BacktestConfig {
            execution_source_alias: "spot".to_string(),
            portfolio_execution_aliases: Vec::new(),
        spot_virtual_rebalance: false,
            initial_capital: 10_000.0,
            maker_fee_bps: 2.0,
            taker_fee_bps: 5.0,
            execution_fee_schedules: std::collections::BTreeMap::new(),
            slippage_bps: 2.0,
        max_volume_fill_pct: Some(0.10),
            diagnostics_detail: palmscript::DiagnosticsDetailMode::SummaryOnly,
            perp: None,
            perp_context: None,
            portfolio_perp_contexts: std::collections::BTreeMap::new(),
        },
        train_bars: 252,
        test_bars: 63,
        step_bars: 63,
    },
)
.expect("walk-forward succeeds");

println!(
    "stitched out-of-sample return = {:.2}%",
    result.stitched_summary.total_return * 100.0
);
```

Use `run_walk_forward_sweep_with_source` to rank explicit numeric `input` grids:

```rust
use palmscript::{
    run_walk_forward_sweep_with_source, BacktestConfig, InputSweepDefinition, Interval, SourceFeed,
    SourceRuntimeConfig, VmLimits, WalkForwardConfig, WalkForwardSweepConfig,
    WalkForwardSweepObjective,
};

let runtime = SourceRuntimeConfig {
    base_interval: Interval::Min1,
    feeds: vec![SourceFeed {
        source_id: 0,
        interval: Interval::Min1,
        bars: vec![],
    }],
};
let result = run_walk_forward_sweep_with_source(
    source,
    runtime,
    VmLimits::default(),
    WalkForwardSweepConfig {
        walk_forward: WalkForwardConfig {
            backtest: BacktestConfig {
                execution_source_alias: "spot".to_string(),
                portfolio_execution_aliases: Vec::new(),
        spot_virtual_rebalance: false,
                initial_capital: 10_000.0,
                maker_fee_bps: 2.0,
                taker_fee_bps: 5.0,
                execution_fee_schedules: std::collections::BTreeMap::new(),
                slippage_bps: 2.0,
        max_volume_fill_pct: Some(0.10),
                diagnostics_detail: palmscript::DiagnosticsDetailMode::SummaryOnly,
                perp: None,
                perp_context: None,
                portfolio_perp_contexts: std::collections::BTreeMap::new(),
            },
            train_bars: 252,
            test_bars: 63,
            step_bars: 63,
        },
        inputs: vec![
            InputSweepDefinition {
                name: "fast_len".to_string(),
                values: vec![13.0, 21.0, 34.0],
            },
            InputSweepDefinition {
                name: "target_atr_mult".to_string(),
                values: vec![2.0, 2.5, 3.0],
            },
        ],
        objective: WalkForwardSweepObjective::TotalReturn,
        top_n: 5,
        base_input_overrides: std::collections::BTreeMap::new(),
    },
)
.expect("walk-forward sweep succeeds");

println!("best ending equity = {}", result.best_candidate.stitched_summary.ending_equity);
```

Use `run_optimize_with_source` for seeded bounded optimization:

```rust
use std::collections::BTreeMap;

use palmscript::{
    run_optimize_with_source, BacktestConfig, Interval, OptimizeConfig, OptimizeObjective,
    OptimizeParamSpace, OptimizeRunner, SourceFeed, SourceRuntimeConfig, VmLimits,
    WalkForwardConfig,
};

let runtime = SourceRuntimeConfig {
    base_interval: Interval::Min1,
    feeds: vec![SourceFeed {
        source_id: 0,
        interval: Interval::Min1,
        bars: vec![],
    }],
};
let result = run_optimize_with_source(
    source,
    runtime,
    VmLimits::default(),
    OptimizeConfig {
        runner: OptimizeRunner::WalkForward,
        backtest: BacktestConfig {
            execution_source_alias: "spot".to_string(),
            portfolio_execution_aliases: Vec::new(),
        spot_virtual_rebalance: false,
            initial_capital: 10_000.0,
            maker_fee_bps: 2.0,
            taker_fee_bps: 5.0,
            execution_fee_schedules: std::collections::BTreeMap::new(),
            slippage_bps: 2.0,
        max_volume_fill_pct: Some(0.10),
            diagnostics_detail: palmscript::DiagnosticsDetailMode::SummaryOnly,
            perp: None,
            perp_context: None,
            portfolio_perp_contexts: std::collections::BTreeMap::new(),
        },
        walk_forward: Some(WalkForwardConfig {
            backtest: BacktestConfig {
                execution_source_alias: "spot".to_string(),
                portfolio_execution_aliases: Vec::new(),
        spot_virtual_rebalance: false,
                initial_capital: 10_000.0,
                maker_fee_bps: 2.0,
                taker_fee_bps: 5.0,
                execution_fee_schedules: std::collections::BTreeMap::new(),
                slippage_bps: 2.0,
        max_volume_fill_pct: Some(0.10),
                diagnostics_detail: palmscript::DiagnosticsDetailMode::SummaryOnly,
                perp: None,
                perp_context: None,
                portfolio_perp_contexts: std::collections::BTreeMap::new(),
            },
            train_bars: 252,
            test_bars: 63,
            step_bars: 63,
        }),
        params: vec![OptimizeParamSpace::IntegerRange {
            name: "fast_len".to_string(),
            low: 8,
            high: 34,
        }],
        objective: OptimizeObjective::RobustReturn,
        trials: 50,
        startup_trials: 16,
        seed: 7,
        workers: 4,
        top_n: 5,
        base_input_overrides: BTreeMap::new(),
    },
)
.expect("optimize succeeds");

println!("best score = {}", result.best_candidate.objective_score);
```

The result includes:

- raw runtime `outputs`
- order lifecycle records in `orders`
- per-fill records in `fills`
- closed round trips in `trades`
- event-centered diagnostics in `diagnostics`
- per-bar account marks in `equity_curve`
- aggregate metrics in `summary`
- any still-open positions in `open_positions` plus the legacy single-position `open_position` convenience field
- optional perp metadata in `perp`

Walk-forward results instead include:

- per-segment `in_sample` summaries
- per-segment `out_of_sample` summaries
- per-segment `out_of_sample_diagnostics` with compact trade/order/export context for the test slice
- a stitched out-of-sample equity curve
- a stitched out-of-sample summary across all segments

Each segment-level out-of-sample diagnostics payload currently includes:

- an out-of-sample diagnostic summary with fill rate, average hold metrics, and protect/target/signal exit counts
- an out-of-sample capture summary with flat/long/short bar mix and execution-asset return context
- out-of-sample export summaries built from the test slice only

This makes it possible to compare weak walk-forward slices by regime/setup state
without rerunning each slice manually.

The `diagnostics` payload is designed for machine analysis and LLM-driven
iteration. It currently includes:

- per-order diagnostics with signal, placement, and fill snapshots of named `export` features
- per-order position snapshots at placement and fill time for attached exits and other order paths
- per-trade diagnostics with entry and exit snapshots, MAE, MFE, holding time, and exit classification
- aggregate summaries such as order fill rate, average bars to fill, average bars held, average MAE/MFE, and counts of signal, protect, target, reversal, and liquidation exits
- capture summaries that compare strategy return with execution-asset return, time spent flat or in market, and opportunity cost while flat
- export summaries for every named `export` or `regime`, including numeric distribution stats, bool activation counts, and an explicit regime marker for named regime declarations
- dedicated active-regime cohorts built from named `regime` declarations alongside the broader active exported bool cohorts
- bounded opportunity events for bool-export activations and backtest-consumed signal decisions, each with forward-return context over `1`, `6`, and `24` execution bars

To make regime and setup context available to diagnostics, export those fields
explicitly in the strategy:

```palm
export trend_long_state = trend_long
export adaptive_bias = spot.close - kama_4h
export breakout_long_state = breakout_long
```

Those exported values are then snapshotted automatically around backtest events.
All exported series also feed the higher-level diagnostics summaries automatically.

Backtest text output keeps those additions compact:

- `Diagnostics Summary` now includes execution-asset return, flat/long/short bar percentages, and opportunity-cost return
- `Top Export States` shows a short summary of the first few export aggregates
- `Recent Opportunity Events` shows the latest bounded export-activation and signal-decision events

The full diagnostics payload remains JSON-first and is returned in the normal `BacktestResult`.

## Signal Resolution

Preferred v1 surface:

- `entry long = ...`
- `exit long = ...`
- `entry short = ...`
- `exit short = ...`
- `protect long = ...`
- `protect short = ...`
- `target long = ...`
- `target short = ...`

Explicit execution templates for trading scripts:

- `order entry long = market(venue = exec)`
- `order exit long = stop_market(trigger_price = lowest(spot.low, 5)[1], trigger_ref = trigger_ref.last, venue = exec)`
- `order entry short = limit(price = spot.close[1], tif = tif.gtc, post_only = false, venue = exec)`
- `order exit short = take_profit_limit(trigger_price = trigger, limit_price = price, tif = tif.gtc, post_only = false, trigger_ref = trigger_ref.mark, expire_time_ms = expire_ms, venue = exec)`
- `size entry1 long = 0.5`
- `size module breakout = 0.5`
- `size entry1 long = risk_pct(0.01, stop_price)`
- `size entry2 long = 0.5`
- `size entry1 short = 0.5`
- `size target1 long = 0.5`
- `size target2 long = 0.5`
- `size target1 short = 0.5`

Attached position-aware exits:

- `protect long = stop_market(trigger_price = position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref = trigger_ref.last, venue = exec)`
- `target long = take_profit_market(trigger_price = position.entry_price + 4, trigger_ref = trigger_ref.last, venue = exec)`
- `size target1 long = 0.5`
- `position.*` is valid only inside `protect` and `target`

PalmScript no longer synthesizes implicit orders for trading
scripts. Declare `order entry ...` and `order exit ...` explicitly for every
signal role you want `check`, `run market`, `run backtest`, `run walk-forward`,
`run walk-forward-sweep`, `run optimize`, or `run paper` to accept.

Actual-fill anchor helpers:

- `position_event.long_entry_fill`, `position_event.short_entry_fill`, `position_event.long_exit_fill`, and `position_event.short_exit_fill` expose aggregate real backtest fill events as `series<bool>`
- exit-kind-specific events are also available:
  `position_event.long_protect_fill`, `short_protect_fill`, `long_target_fill`, `short_target_fill`,
  `long_signal_exit_fill`, `short_signal_exit_fill`, `long_reversal_exit_fill`,
  `short_reversal_exit_fill`, `long_liquidation_fill`, and `short_liquidation_fill`
- anchored helpers such as `highest_since`, `lowest_since`, `highestbars_since`, `lowestbars_since`, and `valuewhen_since` can use those events directly
- example: `protect long = stop_market(trigger_price = highest_since(position_event.long_entry_fill, spot.high) - 3 * atr(spot.high, spot.low, spot.close, 14), trigger_ref = trigger_ref.last, venue = exec)`
- outside backtests, `position_event.*` stays deterministic by evaluating to `false` on every step

Latest closed-trade state:

- `last_exit.*` exposes the most recent closed trade regardless of side
- `last_long_exit.*` and `last_short_exit.*` keep side-specific latest-closed-trade snapshots
- available fields are `kind`, `side`, `price`, `time`, `bar_index`, `realized_pnl`, `realized_return`, and `bars_held`
- `last_*_exit.kind` compares against `exit_kind.protect`, `exit_kind.target`, `exit_kind.signal`, `exit_kind.reversal`, and `exit_kind.liquidation`
- outside backtests, `last_*_exit.*` evaluates to `na`

Execution ledgers:

- `ledger(exec).base_free`, `quote_free`, `base_total`, `quote_total`, and `mark_value_quote` expose the current backtest ledger snapshot for a declared execution alias
- spot aliases report venue base/quote balances, while USD-M aliases expose quote-collateral totals and return `na` for base fields
- in portfolio mode you can read any selected execution alias ledger from the same script, which makes cross-venue inventory logic deterministic during backtests
- outside backtests, `ledger(...)` evaluates to `na`

Reserved trading trigger names:

- `trigger long_entry`, `trigger long_exit`, `trigger short_entry`, and `trigger short_exit` are no longer executable aliases
- if no entry signals are present after resolution, backtest startup fails validation
- ordinary `trigger` declarations with other names remain available for non-backtest consumers

## Execution Model

The backtester stays intentionally simple and deterministic:

- the execution venue profile is inferred from the execution `source` template, for example `binance.spot`, `binance.usdm`, `bybit.spot`, `bybit.usdt_perps`, `gate.spot`, or `gate.usdt_perps`
- signals produced on bar `t` become active starting on the first execution-source base bar with `bar.time > signal_time`
- only one net position is supported: `flat`, `long`, or `short`
- the portfolio model remains net-position based with no explicit quantity expressions
- same-side re-entry is ignored by default except for explicit staged entries
- `size entry1..3 long|short = <expr>` opt a staged entry role into explicit entry sizing
- `size module <name> = <expr>` applies that same entry sizing to the staged entry role bound by `module <name> = entry...`
- `size entry1..3 long|short = capital_fraction(x)` uses a finite fraction in `(0, 1]` of current cash or free collateral at fill time
- `size entry1..3 long|short = risk_pct(pct, stop_price)` sizes from actual fill price and stop distance so the requested loss at `stop_price` is `pct` of current equity, then clamps to capital or margin limits
- o dimensionamento por modulo ja pode depender do regime, por exemplo `size module breakout = strong ? 0.4 : 0.15`
- entry size expressions are evaluated as hidden numeric series like other order fields
- ordens enfileiradas mantem o valor de tamanho capturado quando a solicitacao de ordem foi produzida; elas nao se redimensionam automaticamente a partir de barras futuras
- valid `capital_fraction(...)` values are finite values in `(0, 1]`
- valid `risk_pct(...)` values are finite values `> 0`
- opposite entry reverses on the same eligible open by closing first and then
  opening the new side
- attached exits arm only after an actual entry fill exists and are reevaluated once per execution bar while that position stays open
- only the currently active staged protect and next staged target are armed for a side at any time
- `protect_after_target1..3` inherit from the most recent declared protect stage when an exact staged protect is absent
- `size entry1..3 long|short = <expr>` optionally reduce a staged entry fill to a fraction of current cash or compute a risk-based quantity instead of opening all-in
- `size module <name> = <expr>` is a shorthand for the same staged entry sizing when you want sizing to follow the module label instead of the raw role name
- `size target1..3 long|short = <expr>` optionally reduce a staged target fill to a fraction of the current position instead of closing it fully
- entry and target size expressions are evaluated as hidden numeric series like other order fields
- v1 only supports explicit sizing for staged `entry` roles and staged attached `target` roles; other order roles still close or open the full position
- valid target size fractions are finite values in `(0, 1]`
- `risk_pct(...)` is entry-only in v1 and records the requested risk percentage, stop price, effective risk-per-unit, and whether the final size was capital-limited
- staged entry fills emit aggregate `position_event.long_entry_fill` / `short_entry_fill` plus staged fields such as `position_event.long_entry2_fill`
- staged target fills emit aggregate `position_event.long_target_fill` / `short_target_fill` plus staged fields such as `position_event.long_target2_fill`
- a partial staged target is one-shot for that stage: once `target1` has filled, `target2` becomes the active target and the remaining runner stays managed by the current staged protect / discretionary `exit`
- if `protect` and `target` both become fillable on one execution bar, `protect` fills and `target` is cancelled
- spot venues continue to use the original cash/notional model
- perp venues now support isolated margin, per-venue risk tiers, leverage, and deterministic liquidation checks
- liquidation checks run after fills and before the strategy step on each execution bar
- v1 does not liquidate a position from the full mark-price range of its entry bar; liquidation checks begin on the first later execution bar after the fill
- Binance USD-M uses mark-price kline bars as the liquidation mark basis
- Bybit USDT perps use mark-price kline bars as the liquidation mark basis
- Gate USDT perps use mark-price candlesticks as the liquidation mark basis
- open positions are not force-closed at the end of the run unless liquidation was triggered earlier

Supported order constructors:

- `market(venue = exec)`
- `limit(price, tif, post_only, venue)`
- `stop_market(trigger_price, trigger_ref, venue)`
- `stop_limit(trigger_price, limit_price, tif, post_only, trigger_ref, expire_time_ms, venue)`
- `take_profit_market(trigger_price, trigger_ref, venue)`
- `take_profit_limit(trigger_price, limit_price, tif, post_only, trigger_ref, expire_time_ms, venue)`

Enum namespaces:

- `tif.gtc`, `tif.ioc`, `tif.fok`, `tif.gtd`
- `trigger_ref.last`, `trigger_ref.mark`, `trigger_ref.index`

Deterministic fill rules:

- `market(venue = exec)`: fills on the next eligible execution-bar open; buy-side fills use `open * (1 + slippage_bps / 10_000)`, sell-side fills use `open * (1 - slippage_bps / 10_000)`, and fees use the taker rate from the resolved fee schedule
- when `max_volume_fill_pct` is set, any would-be fill whose quantity exceeds `bar.volume * max_volume_fill_pct` is cancelled with `VolumeParticipationExceeded` instead of being partially filled
- `limit(...)`: fills on the first eligible bar whose range crosses the limit; the fill price is the better of `open` and `limit`, and resting fills use the maker rate
- `stop_market(...)`: triggers on the first eligible bar whose range crosses the stop; the fill price is the worse of `open` and `trigger_price`, and fills use the taker rate
- `take_profit_market(...)`: triggers on the first eligible bar whose range crosses the trigger; the fill price is the better of `open` and `trigger_price`, and fills use the taker rate
- `stop_limit(...)` and `take_profit_limit(...)`: trigger on crossing; if the opening price already satisfies the resulting limit, they fill immediately and use the taker rate, otherwise they become resting limit orders starting from the next bar and use the maker rate
- `tif.ioc` and `tif.fok`: evaluate only on the first eligible bar and cancel if they do not fully fill
- `tif.gtd`: expires deterministically before evaluating any execution bar at or beyond `expire_time_ms`

Venue profile notes:

- Binance Spot supports the order constructors above, but only `trigger_ref.last` on trigger orders and no `tif.gtd`
- Binance USD-M supports `trigger_ref.last` and `trigger_ref.mark`
- venue-incompatible orders are rejected before simulation starts

Perp startup requirements:

- Binance USD-M prefers live signed leverage brackets when these env vars are available:
  - `PALMSCRIPT_BINANCE_USDM_API_KEY`
  - `PALMSCRIPT_BINANCE_USDM_API_SECRET`
- without those credentials, Binance USD-M falls back to an approximate single-tier risk snapshot built from public `exchangeInfo` symbol margin fields
- the fetched or approximated risk snapshot is embedded in `BacktestResult.perp`
- walk-forward reuses one fetched perp context across all stitched segments in the same run
- in isolated-margin perp mode, PalmScript caps a closed position's residual value at zero and cancels later entry attempts with `InsufficientCollateral` once free collateral is exhausted

## Current Scope

Not included in V1:

- partial fills
- order book or queue-position modeling
- cross-margin accounting
- funding payments
- borrow fees
- venue liquidation penalty fees
