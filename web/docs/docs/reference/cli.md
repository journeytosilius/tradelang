# CLI Command Reference

This page is the compact public command reference for the `palmscript` CLI. For workflows and examples, see [CLI](../tooling/cli.md).

## `palmscript check`

```bash
palmscript check <script.ps>
```

Compiles and validates a script without executing it.

Arguments:

- `<script.ps>`: path to the PalmScript source file

## `palmscript docs`

```bash
palmscript docs [<topic>] [--list|--all]
```

Reads the embedded public English docs snapshot shipped inside the CLI binary.

Arguments and flags:

- `<topic>`: exact embedded docs topic path, for example `tooling/cli` or `reference/intervals-and-sources`
- `--list`: print every embedded topic with its title and relative docs path
- `--all`: print the full embedded English docs set in one terminal-friendly stream

Notes:

- if neither `<topic>` nor a flag is passed, the command prints usage plus the topic list
- `--list` is the best discovery mode before calling a specific topic
- the embedded docs are generated from `web/docs/docs/` at build time

## `palmscript inspect`

```bash
palmscript inspect exports <artifact.json> [--format json|text]
palmscript inspect export <artifact.json> <name> [--format json|text]
palmscript inspect overlap <artifact.json> <left> <right> [--format json|text]
```

Queries saved PalmScript outputs artifacts without writing ad hoc JSON-processing scripts.

Arguments and flags:

- `<artifact.json>`: path to a saved backtest result, paper export, or raw outputs JSON artifact
- `<name>`: one export name to summarize
- `<left>` / `<right>`: two boolean export names to compare
- `--format json|text`: output rendering format, default `json`

Notes:

- `inspect exports` lists every export with point counts plus bool or numeric summary stats
- `inspect export` prints the same summary for one named export
- `inspect overlap` requires boolean exports and reports counts such as `both_true_count`, `left_only_true_count`, and `na_count`

## `palmscript run market`

```bash
palmscript run market <script.ps> --from <unix_ms> --to <unix_ms> \
  [--format json|text] \
  [--max-instructions-per-bar <N>] \
  [--max-history-capacity <N>]
```

Arguments and flags:

- `<script.ps>`: path to the PalmScript source file
- `--from <unix_ms>`: inclusive lower time bound in Unix milliseconds UTC
- `--to <unix_ms>`: exclusive upper time bound in Unix milliseconds UTC
- `--format json|text`: output rendering format, default `json`
- `--max-instructions-per-bar <N>`: VM instruction budget per step, default `10000`
- `--max-history-capacity <N>`: maximum retained history per series slot, default `1024`

Requirements:

- the script must declare at least one `source`
- `--from` must be strictly less than `--to`
- if the script declares trading signal roles, it must also declare at least one `execution` target and matching explicit `order ...` templates for every declared `entry` / `exit` signal role

## `palmscript run backtest`

```bash
palmscript run backtest <script.ps> --from <unix_ms> --to <unix_ms> \
  [--preset <path>] \
  [--preset-trial-id <N>] \
  [--execution-source <alias>]... \
  [--initial-capital <N>] \
  --maker-fee-bps <N> --taker-fee-bps <N> \
  [--fee-schedule <alias:maker:taker>]... \
  [--set name=value]... \
  [--slippage-bps <N>] \
  [--diagnostics summary|full-trace] \
  [--format json|text]
```

Additional diagnostics flag:

- `--diagnostics summary|full-trace`: diagnostics detail mode; default `summary`
- `--preset <path>`: load saved optimize survivor overrides from a preset artifact
- `--preset-trial-id <N>`: replay that saved top-candidate `trial_id` instead of the preset best candidate
- `--set name=value`: override one numeric `input` on top of the selected preset survivor; repeat per input
- trading scripts require at least one declared `execution` target in the script
- trading scripts also require matching explicit `order ...` templates for every declared `entry` / `exit` signal role
- repeat `--execution-source <alias>` to activate portfolio mode across the selected execution aliases
- `--spot-virtual-rebalance`: optional multi-venue spot-only portfolio mode that transfers quote between the selected venue ledgers automatically before long entries
- execution-oriented runs require explicit `--maker-fee-bps` and `--taker-fee-bps`; repeat `--fee-schedule <alias:maker:taker>` to override one selected alias

## `palmscript run walk-forward`

```bash
palmscript run walk-forward <script.ps> --from <unix_ms> --to <unix_ms> \
  --train-bars <N> --test-bars <N> [--step-bars <N>] \
  [--min-trades <N>] [--min-sharpe <N>] [--max-zero-trade-segments <N>] \
  [--preset <path>] \
  [--preset-trial-id <N>] \
  [--execution-source <alias>]... \
  [--initial-capital <N>] \
  --maker-fee-bps <N> --taker-fee-bps <N> \
  [--fee-schedule <alias:maker:taker>]... \
  [--set name=value]... \
  [--slippage-bps <N>] \
  [--diagnostics summary|full-trace] \
  [--format json|text]
```

Additional diagnostics flag:

- `--diagnostics summary|full-trace`: diagnostics detail mode; default `summary`
- `--min-trades <N>`: require at least `N` out-of-sample trades across stitched walk-forward segments
- `--min-sharpe <N>`: require stitched walk-forward Sharpe to stay at or above `N`
- `--max-zero-trade-segments <N>`: fail validation when more than `N` out-of-sample segments produce zero trades
- `--preset <path>`: load saved optimize survivor overrides from a preset artifact
- `--preset-trial-id <N>`: replay that saved top-candidate `trial_id` instead of the preset best candidate
- `--set name=value`: override one numeric `input` on top of the selected preset survivor; repeat per input
- trading scripts require at least one declared `execution` target in the script
- trading scripts also require matching explicit `order ...` templates for every declared `entry` / `exit` signal role
- repeat `--execution-source <alias>` to activate portfolio mode across the selected execution aliases
- `--spot-virtual-rebalance`: optional multi-venue spot-only portfolio mode that transfers quote between the selected venue ledgers automatically before long entries
- execution-oriented runs require explicit `--maker-fee-bps` and `--taker-fee-bps`; repeat `--fee-schedule <alias:maker:taker>` to override one selected alias

## `palmscript run optimize`

```bash
palmscript run optimize <script.ps> --from <unix_ms> --to <unix_ms> \
  [--runner walk-forward|backtest] \
  [--execution-source <alias>]... \
  [--initial-capital <N>] \
  --maker-fee-bps <N> --taker-fee-bps <N> \
  [--fee-schedule <alias:maker:taker>]... \
  [--slippage-bps <N>] \
  [--train-bars <N>] \
  [--test-bars <N>] \
  [--step-bars <N>] \
  [--holdout-bars <N>] \
  [--no-holdout] \
  [--min-trades <N>] \
  [--min-sharpe <N>] \
  [--min-holdout-trades <N>] \
  [--require-positive-holdout] \
  [--max-zero-trade-segments <N>] \
  [--min-holdout-pass-rate <0..1>] \
  [--min-date-perturbation-positive-ratio <0..1>] \
  [--min-date-perturbation-outperform-ratio <0..1>] \
  [--max-overfitting-risk low|moderate|high] \
  [--param int:name=low:high[:step]] \
  [--param float:name=low:high[:step]] \
  [--param choice:name=v1,v2,v3] \
  [--objective robust-return|total-return|ending-equity|return-over-drawdown] \
  [--trials <N>] \
  [--startup-trials <N>] \
  [--seed <N>] \
  [--workers <N>] \
  [--top <N>] \
  [--direct-validate-top <N>] \
  [--preset-out <path>] \
  [--diagnostics summary|full-trace] \
  [--format json|text]
```

Arguments and flags:

- `<script.ps>`: path to the PalmScript source file
- `--from <unix_ms>`: inclusive lower time bound in Unix milliseconds UTC
- `--to <unix_ms>`: exclusive upper time bound in Unix milliseconds UTC
- `--runner`: optimize evaluation mode; defaults to `walk-forward`
- `--execution-source <alias>`: execution alias selection; repeat it to activate portfolio mode
- `--initial-capital <N>`: account starting equity for each execution-oriented run; default `10000`
- `--maker-fee-bps <N>`: required global maker fee in basis points for execution-oriented runs
- `--taker-fee-bps <N>`: required global taker fee in basis points for execution-oriented runs
- `--fee-schedule <alias:maker:taker>`: optional execution-alias-specific maker/taker fee override; repeat per alias
- `--slippage-bps <N>`: slippage model in basis points; default `2`
- `--train-bars <N>`: in-sample bars per walk-forward segment
- `--test-bars <N>`: out-of-sample bars per walk-forward segment
- `--step-bars <N>`: segment advance size; defaults to `test-bars`
- `--holdout-bars <N>`: reserve the final `N` execution bars as a final untouched holdout
- `--no-holdout`: explicitly disable the default untouched holdout reservation
- `--min-trades <N>`: require at least `N` stitched candidate trades before a candidate can pass validation
- `--min-sharpe <N>`: require stitched candidate Sharpe to stay at or above `N`
- `--min-holdout-trades <N>`: require at least `N` trades in the final untouched holdout summary
- `--require-positive-holdout`: require the final untouched holdout return to stay above `0`
- `--max-zero-trade-segments <N>`: reject walk-forward candidates that produce more than `N` zero-trade OOS segments
- `--min-holdout-pass-rate <0..1>`: require at least that fraction of evaluated top-candidate holdout reruns to pass
- `--min-date-perturbation-positive-ratio <0..1>`: require at least that fraction of validated candidate date-perturbation reruns to stay positive
- `--min-date-perturbation-outperform-ratio <0..1>`: require at least that fraction of validated candidate date-perturbation reruns to beat execution-asset buy-and-hold
- `--max-overfitting-risk low|moderate|high`: reject validated candidates whose deterministic overfitting-risk level exceeds the selected ceiling
- `--param ...`: search-space declaration; repeat for multiple tuned inputs, with optional integer/float step support
- `--objective ...`: ranking objective; defaults to `robust-return`
- `--trials <N>`: total bounded trial budget
- `--startup-trials <N>`: initial random trial count before the TPE search phase
- `--seed <N>`: deterministic optimizer seed
- `--workers <N>`: bounded parallel worker count
- `--top <N>`: number of top candidates to retain
- `--direct-validate-top <N>`: rerun that many top feasible validated survivors as full-window direct backtests and include stitched-vs-direct drift summaries in the optimize result
- `--preset-out <path>`: write the best preset and top candidates to disk
- `--diagnostics summary|full-trace`: diagnostics detail mode; default `summary`
- `--format json|text`: output rendering format; default `json`

Default safety behavior:

- `walk-forward` is the default optimizer runner
- trading scripts require at least one declared `execution` target in the script
- trading scripts also require matching explicit `order ...` templates for every declared `entry` / `exit` signal role
- when `walk-forward` is used, the CLI reserves a final untouched holdout automatically
- the default holdout size matches `test-bars`
- if `--param` is omitted, PalmScript first looks for preset parameter space and then infers search space from `input ... optimize(...)` metadata inside the script
- repeated `--execution-source` flags activate portfolio mode, which seeds one explicit ledger per selected execution alias from `initial_capital`
- `--spot-virtual-rebalance` lets multi-venue spot backtests and optimize runs transfer quote between those venue ledgers automatically before long entries
- execution-oriented runs require explicit `--maker-fee-bps` and `--taker-fee-bps`; repeat `--fee-schedule <alias:maker:taker>` to override one selected alias
- portfolio scripts can declare `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, and `portfolio_group` to block entries that would exceed shared caps
- the final JSON/text result also carries validation-constraint summaries, feasible vs infeasible candidate counts, best-infeasible-candidate fallback data, constraint-failure breakdowns, optional direct-validation survivor replays, holdout drift, top-candidate holdout robustness, holdout pass rate, parameter stability ranges, deterministic overfitting-risk summaries, `starting_ledgers`, `ending_ledgers`, `ledger_events`, and improvement hints

## `palmscript run paper`

```bash
palmscript run paper <script.ps> \
  [--execution-source <alias>]... \
  [--initial-capital <N>] \
  --maker-fee-bps <N> --taker-fee-bps <N> \
  [--fee-schedule <alias:maker:taker>]... \
  [--slippage-bps <N>] \
  [--leverage <N>] \
  [--margin-mode isolated] \
  [--diagnostics summary|full-trace] \
  [--format json|text]
```

Arguments and flags:

- `<script.ps>`: path to the PalmScript source file
- `--execution-source <alias>`: execution alias selection; repeat it to activate shared-equity portfolio paper mode
- `--initial-capital <N>`: paper account starting equity; default `10000`
- `--maker-fee-bps <N>`: required global maker fee in basis points for execution-oriented runs
- `--taker-fee-bps <N>`: required global taker fee in basis points for execution-oriented runs
- `--fee-schedule <alias:maker:taker>`: optional execution-alias-specific maker/taker fee override; repeat per alias
- `--slippage-bps <N>`: slippage model in basis points; default `2`
- `--leverage <N>`: optional isolated leverage for perp execution aliases
- `--margin-mode isolated`: perp margin mode; only `isolated` is currently supported
- `--diagnostics summary|full-trace`: diagnostics detail mode; default `summary`
- `--format json|text`: output rendering format; default `json`

Notes:

- `run paper` submits a persistent local paper session; it does not start the daemon itself
- trading scripts submitted to `run paper` require at least one declared `execution` target and matching explicit `order ...` templates for every declared `entry` / `exit` signal role
- scripts that reference `binance.usdm` auxiliary historical source fields such as `funding_rate`, `mark_price`, `index_price`, `premium_index`, or `basis` are rejected by `run paper` until live polling for those fields exists
- the session snapshots the script source and queues it under the local execution state root
- v1 paper mode uses the existing VM and deterministic order simulator with closed-bar strategy evaluation, not real live order placement
- queued sessions now transition through `queued -> arming_history -> arming_live -> live`
- `paper-status` and `paper-export` now include feed readiness counters plus a `required_feeds` inventory with each feed's arming state, readiness flags, latest closed bar time, and quote snapshots for each execution alias

## `palmscript run paper-list`

```bash
palmscript run paper-list [--format json|text]
```

Lists the locally persisted paper session manifests.

## `palmscript run paper-status`

```bash
palmscript run paper-status <session-id> [--format json|text]
```

Reads the latest persisted paper-session snapshot, including feed readiness counters and current execution-alias quote health when available.

## `palmscript run paper-stop`

```bash
palmscript run paper-stop <session-id> [--format json|text]
```

Marks a queued or running paper session for stop.

## `palmscript run paper-logs`

```bash
palmscript run paper-logs <session-id> [--format json|text]
```

Reads the persistent paper-session event log.

## `palmscript run paper-positions`

```bash
palmscript run paper-positions <session-id> [--format json|text]
```

Prints the latest open positions for the paper session.

## `palmscript run paper-orders`

```bash
palmscript run paper-orders <session-id> [--format json|text]
```

Prints the latest persisted paper orders for the session.

## `palmscript run paper-fills`

```bash
palmscript run paper-fills <session-id> [--format json|text]
```

Prints the latest persisted paper fills for the session.

## `palmscript run paper-export`

```bash
palmscript run paper-export <session-id> [--format json|text]
```

Exports the full persisted paper session bundle, including manifest, snapshot, and latest result when available.

## `palmscript execution serve`

```bash
palmscript execution serve [--poll-interval-ms <N>] [--once]
```

Notes:

- one local execution service can host many paper sessions
- active paper sessions share one in-process armed feed cache per venue/symbol/canonical interval instead of duplicating upstream history bootstrap and quote refreshes
- the daemon status output now includes `subscription_count`, `armed_feed_count`, `connecting_feed_count`, `degraded_feed_count`, and `failed_feed_count`

Arguments and flags:

- `--poll-interval-ms <N>`: closed-bar polling interval for queued/running paper sessions; default `30000`
- `--once`: process the queue once and exit instead of running as a long-lived local daemon

## `palmscript execution status`

```bash
palmscript execution status [--format json|text]
```

Prints the local daemon heartbeat snapshot when available.

## `palmscript execution stop`

```bash
palmscript execution stop
```

Requests that the local execution daemon stop on its next loop iteration.

## `palmscript dump-bytecode`

```bash
palmscript dump-bytecode <script.ps> [--format text|json]
```

Arguments and flags:

- `<script.ps>`: path to the PalmScript source file
- `--format text|json`: bytecode output format, default `text`
