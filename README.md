# PalmScript

<p align="center">
  <img src="editors/vscode/images/palmscript.png" alt="PalmScript logo" width="220">
</p>

PalmScript is a language for financial time-series strategies.

The public documentation is focused on:

- language syntax and semantics
- builtins and indicators
- examples and learning guides
- the basic CLI flow for checking and running scripts

Documentation:

- published site: <https://palmscript.dev/docs/>
- Spanish docs: <https://palmscript.dev/es/docs/>
- Portuguese (Brazil) docs: <https://palmscript.dev/pt-BR/docs/>
- German docs: <https://palmscript.dev/de/docs/>
- Japanese docs: <https://palmscript.dev/ja/docs/>
- French docs: <https://palmscript.dev/fr/docs/>
- hosted IDE: <https://palmscript.dev/>
- local source: [web/docs/docs/index.md](web/docs/docs/index.md)

Start here:

- [Learn](web/docs/docs/learn/overview.md)
- [Language Reference](web/docs/docs/reference/overview.md)
- [Indicators Reference](web/docs/docs/reference/indicators.md)

Repo-local tooling docs:

- [Browser IDE](web/docs/docs/tooling/browser-ide.md)

## Common Commands

```bash
cargo build --bin palmscript
cargo build --bin palmscript-ide-server
npm --prefix web/ide run build
target/debug/palmscript check crates/palmscript/examples/strategies/sma_cross.ps
target/debug/palmscript docs --list
target/debug/palmscript docs --all
target/debug/palmscript inspect exports artifacts/backtest.json --format text
target/debug/palmscript run market crates/palmscript/examples/strategies/sma_cross.ps --from 1704067200000 --to 1704153600000
target/debug/palmscript run market crates/palmscript/examples/strategies/cross_source_spread.ps --from 1704067200000 --to 1704153600000
target/debug/palmscript run market crates/palmscript/examples/strategies/bybit_spot.ps --from 1704067200000 --to 1704153600000
target/debug/palmscript run market crates/palmscript/examples/strategies/gate_spot.ps --from 1704067200000 --to 1704153600000
target/debug/palmscript run backtest crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps --from 1704067200000 --to 1704153600000 --leverage 2
target/debug/palmscript run backtest crates/palmscript/examples/strategies/gate_usdt_perps_backtest.ps --from 1704067200000 --to 1704153600000 --leverage 2
target/debug/palmscript run backtest crates/palmscript/examples/strategies/portfolio_caps_backtest.ps --from 1704067200000 --to 1704153600000 --execution-source left --execution-source right
target/debug/palmscript run optimize crates/palmscript/examples/strategies/adaptive_trend_backtest.ps --from 1646611200000 --to 1772841600000 --train-bars 252 --test-bars 63 --step-bars 63 --trials 50 --preset-out best.json
target/debug/palmscript run paper crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps --execution-source perp
target/debug/palmscript execution serve --once
target/debug/palmscript dump-bytecode crates/palmscript/examples/strategies/sma_cross.ps
mkdocs build --strict -f web/docs/mkdocs.yml
sh infra/scripts/build_docs_site.sh
```

Any script that declares trading signal roles now requires at least one
top-level `execution` declaration and matching explicit `order ...`
declarations for every declared `entry` / `exit` signal role. Every executable
inline order and every `order_template` must declare `venue = <execution_alias>`
explicitly, even when the script declares only one execution target. If the
script declares exactly one `execution` alias, the CLI still selects it
automatically for commands that need an execution source. If the script
declares multiple execution aliases, pass `--execution-source <alias>` to
choose one or repeat that flag to activate portfolio mode.

PalmScript no longer synthesizes implicit `market()` orders for trading scripts
in `check`, `run market`, `run backtest`, `run walk-forward`,
`run walk-forward-sweep`, `run optimize`, or `run paper`.

Trading scripts can also declare reusable top-level `order_template` bindings
and reference them from `order ...` declarations. For example:

```palmscript
execution perp = binance.usdm("BTCUSDT")
order_template market_order = market(venue = perp)
order entry long = market_order
order exit long = market_order
```

Trading scripts can also label entry roles for per-module attribution in
backtest, walk-forward, and optimize diagnostics:

```palmscript
module breakout = entry long
module pullback = entry2 long
```

These labels currently bind to `entry`, `entry2`, or `entry3` roles and flow
through trade diagnostics plus cohort summaries as `entry_module`.
They can also reuse the existing staged entry sizing surface through
`size module <name> = <expr>`, so a module can opt into the same
`capital_fraction(...)`, legacy bare fraction, or `risk_pct(...)` entry sizing
without duplicating the bound staged entry role name.
Because that size expression is captured through the same hidden order-field
path as other order parameters, module sizing can already follow regime logic
at signal time, for example
`size module breakout = strong_trend ? 0.4 : 0.15` or
`size module breakout = risk_pct(strong_trend ? 0.01 : 0.005, stop_price)`.

`run optimize` now defaults to walk-forward tuning with a final untouched holdout window reserved from the tail of the selected execution range. By default that holdout size matches `--test-bars`. Optimizer search space can now live directly in the script through `input ... optimize(int|float|choice, ...)` metadata, with explicit `--param` still taking precedence when you need to override it. PalmScript also supports first-class `regime` declarations backed by the `state(enter, exit)` builtin for persistent market-state logic, plus declarative backtest controls such as `cooldown long = 12` and `max_bars_in_trade short = 48`. The executable indicator surface now includes `supertrend`, `anchored_vwap`, `donchian`, rolling `percentile`, rolling `zscore`, and `ulcer_index`.

Backtests can also run in portfolio mode when you repeat `--execution-source`. PalmScript now seeds one explicit ledger per selected execution alias from `initial_capital`: spot aliases hold quote/base balances per venue, while USD-M aliases hold quote-collateral balances plus isolated-margin positions. Without `--spot-virtual-rebalance`, multi-venue spot entries can only spend the local venue quote balance on that alias. Pass `--spot-virtual-rebalance` on multi-venue spot backtests, walk-forward runs, walk-forward sweeps, or optimize runs when you want PalmScript to transfer quote between those spot venue ledgers automatically before long entries. That virtual-rebalance mode is long/flat only for spot aliases in v1. Top-level declarations such as `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, and `portfolio_group "name" = [alias, ...]` block only the new entries that would exceed the configured shared caps.

Strategy logic can now read those backtest ledgers directly with `ledger(<execution_alias>).base_free`, `quote_free`, `base_total`, `quote_total`, and `mark_value_quote`. Outside backtests those `ledger(...)` fields stay defined but evaluate to `na`, which keeps scripts deterministic across `run market` and compiled analysis flows.

Venue-selection builtins can also compare declared execution aliases on the
current bar: `cheapest(exec_a, exec_b, ...)`, `richest(exec_a, exec_b, ...)`,
and `spread_bps(buy_exec, sell_exec)`. The selector helpers return
`execution_alias` values so they can be compared to aliases or fed into other
helpers, while `spread_bps` returns the current close-to-close spread in basis
points. If any referenced execution alias has no current price on the active
bar, the affected helper returns `na`.

PalmScript now also reserves first-class arbitrage declarations:
`arb_entry = ...`, `arb_exit = ...`, and `arb_order entry|exit = market_pair(...)`
/ `limit_pair(...)` / `mixed_pair(...)`. Portfolio backtests now execute the
`market_pair(...)` subset against the existing per-execution spot ledgers when
at least two spot execution aliases are selected. In v1 the first selected
portfolio execution alias acts as the controller runtime, `size = ...` is the
base-asset quantity, and each basket currently surfaces as paired long/short
leg fills and trades. `limit_pair(...)` and `mixed_pair(...)` still compile but
reject at runtime until resting paired-order semantics land.

Spot portfolio scripts can also declare first-class quote transfers with
`transfer quote = quote_transfer(...)`. The controller runtime evaluates that
declaration each bar, debits the source ledger on the next bar open, and credits
the destination ledger after `delay_bars`. In v1 only quote transfers execute;
`transfer base = base_transfer(...)` remains reserved but unsupported.

Those portfolio runs now report typed basket and transfer summaries too.
Backtests expose top-level `arbitrage` diagnostics with per-pair basket counts,
spreads, and realized PnL, plus `transfer_summary` diagnostics with route
counts, fees, and delay statistics. Walk-forward stitched summaries, holdout
windows, and optimize direct validations carry the same summary shapes.

Fee modeling now requires explicit live-like maker/taker schedules on execution-oriented CLI runs. `--maker-fee-bps` and `--taker-fee-bps` set the default schedule, and `--fee-schedule <alias:maker:taker>` overrides one selected execution alias so portfolio backtests can simulate different exchange fee tiers in the same run.

Backtest-oriented CLI commands now also expose a richer diagnostics surface. `run backtest`, `run walk-forward`, and `run optimize` accept `--diagnostics summary|full-trace`. Summary mode keeps compact machine-readable cohort, drawdown, Sharpe, baseline-comparison, source-alignment, holdout-drift, robustness, overfitting-risk, validation-constraint, and hint data, including fixed 4-hour UTC time-bucket cohort summaries, explicit `starting_ledgers` / `ending_ledgers` / `ledger_events`, and top-level backtests also add bounded date-perturbation reruns. `run backtest` and `run walk-forward` can replay saved optimize survivors directly with `--preset <path> --preset-trial-id <trial_id>`, and `--set name=value` can mutate that saved survivor in-place without editing the preset file. `run walk-forward` and `run optimize` also accept explicit quality gates such as `--min-trades`, `--min-sharpe`, `--max-zero-trade-segments`, `--min-holdout-trades`, `--require-positive-holdout`, `--min-holdout-pass-rate`, `--min-date-perturbation-positive-ratio`, `--min-date-perturbation-outperform-ratio`, and `--max-overfitting-risk`. `run optimize` now chooses winners from the validated feasible survivor set when one exists, supports `--direct-validate-top <N>` to replay top feasible survivors over the full window, and the final JSON/text output reports typed constraint summaries, validated/feasible/infeasible candidate counts, best-infeasible fallback data, constraint-failure breakdowns, direct-validation drift summaries, optimize holdout pass rate, and candidate/direct-validation time-bucket cohort summaries. Full-trace mode adds one typed per-bar decision trace per execution bar so agents can inspect why a signal or order was queued, blocked, expired, or forced out. Saved outputs artifacts can now be queried directly with `palmscript inspect exports`, `palmscript inspect export`, and `palmscript inspect overlap` instead of ad hoc JSON-scraping scripts.

PalmScript also now ships a first-class local paper-execution loop. `run paper` snapshots a script into a persistent local paper session, and `execution serve` maintains one shared armed feed cache per local service so paper sessions can reuse compiler-derived warmup history plus current top-of-book bid/ask, last-price, and mark-price snapshots without duplicating upstream venue fetches. Sessions stay blocked in explicit `arming_history` and `arming_live` states until the required feed inventory is ready, and `paper-status` / `paper-export` now surface the per-feed readiness summary directly. The VM still evaluates only on closed execution bars, but open paper positions are valued from live top-of-book mid prices when available. v1 execution is still paper only and never places real live orders.

The language now also separates market-data bindings from execution routing. `source` remains the market-series input surface, while top-level `execution` declarations define venue targets for orders. Order constructors still accept the legacy positional syntax, but they also support named arguments such as `venue = exec` so multi-source scripts can read from many exchanges while routing fills to one explicit execution alias.

Exchange-backed source endpoints can be overridden with environment variables for mock servers and venue-specific routing:

- `PALMSCRIPT_BINANCE_SPOT_BASE_URL`
- `PALMSCRIPT_BINANCE_USDM_BASE_URL`
- `PALMSCRIPT_BYBIT_BASE_URL`
- `PALMSCRIPT_GATE_BASE_URL`

Historical exchange-backed runs now also expose Binance USD-M auxiliary source fields directly on `binance.usdm("<symbol>")`: `funding_rate`, `mark_price`, `index_price`, `premium_index`, and `basis`. Historical modes fetch those datasets automatically when referenced, while `run paper` still rejects them until live polling is implemented.

## Browser IDE Container

```bash
bash infra/scripts/build_ide_web.sh
docker build -f infra/docker/Dockerfile.ide -t palmscript-ide .
docker run --rm -p 8080:8080 palmscript-ide
```

The browser IDE shell now ships as a Vite-built React and TypeScript frontend
using Monaco Editor, embedded directly by the `palmscript-ide-server` binary.
Refresh the checked-in browser bundle with `bash infra/scripts/build_ide_web.sh`
when you change the frontend under `web/ide/`.

The web shell keeps the same blue-grey and accent-blue visual language as the
published docs at <https://palmscript.dev/docs/>.

The public demo keeps the chrome intentionally minimal: one editor buffer, a
calendar date-range picker over the available BTCUSDT dataset history, Monaco editing,
compile diagnostics, Monaco hover and completion docs for builtins and language
constructs, callable completion snippets in Monaco and VS Code, and backtest
output panels. The toolbar keeps the PalmScript logo inside the header instead
of a text title, plus a light/dark mode switch. Dark mode uses a VS Code-like
shell with a Dracula-inspired Monaco theme, the shell typography uses Inter,
and the browser tab favicon now matches the current PalmScript logo.

The hosted IDE entrypoint is `/`. `https://palmscript.dev/` serves the
browser IDE directly, while the public docs remain under `/docs/`.

The documentation build is locale-aware. English is the canonical default at
`/docs/`, Spanish is served at `/es/docs/`, Portuguese (Brazil) is served at
`/pt-BR/docs/`, German is served at `/de/docs/`, Japanese is served at
`/ja/docs/`, French is served at `/fr/docs/`, and future locales follow the
same `/{lang}/docs/` pattern.
