# Reference Des Commandes CLI

Cette page est de nouveau publique parce que PalmScript est maintenant open source. Une localisation complete sera publiee dans une mise a jour ulterieure. En attendant, le contenu canonique en anglais est inclus ci-dessous afin que cette version du site expose la meme surface publique CLI et tooling.

## English Canonical Content

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

## `palmscript run backtest`

```bash
palmscript run backtest <script.ps> --from <unix_ms> --to <unix_ms> \
  [--execution-source <alias>]... \
  [--initial-capital <N>] \
  --maker-fee-bps <N> --taker-fee-bps <N> \
  [--fee-schedule <alias:maker:taker>]... \
  [--slippage-bps <N>] \
  [--diagnostics summary|full-trace] \
  [--format json|text]
```

Additional diagnostics flag:

- `--diagnostics summary|full-trace`: diagnostics detail mode; default `summary`
- repeat `--execution-source <alias>` to activate portfolio mode with a shared equity ledger across the selected execution aliases
- execution-oriented runs require explicit `--maker-fee-bps` and `--taker-fee-bps`; repeat `--fee-schedule <alias:maker:taker>` to override one selected alias

## `palmscript run walk-forward`

```bash
palmscript run walk-forward <script.ps> --from <unix_ms> --to <unix_ms> \
  --train-bars <N> --test-bars <N> [--step-bars <N>] \
  [--execution-source <alias>]... \
  [--initial-capital <N>] \
  --maker-fee-bps <N> --taker-fee-bps <N> \
  [--fee-schedule <alias:maker:taker>]... \
  [--slippage-bps <N>] \
  [--diagnostics summary|full-trace] \
  [--format json|text]
```

Additional diagnostics flag:

- `--diagnostics summary|full-trace`: diagnostics detail mode; default `summary`
- repeat `--execution-source <alias>` to activate portfolio mode with a shared equity ledger across the selected execution aliases
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
  [--param int:name=low:high[:step]] \
  [--param float:name=low:high[:step]] \
  [--param choice:name=v1,v2,v3] \
  [--objective robust-return|total-return|ending-equity|return-over-drawdown] \
  [--trials <N>] \
  [--startup-trials <N>] \
  [--seed <N>] \
  [--workers <N>] \
  [--top <N>] \
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
- trading scripts require at least one declared `execution` target in the script
- trading scripts also require matching explicit `order ...` templates for every declared `entry` / `exit` signal role
- `--train-bars <N>`: in-sample bars per walk-forward segment
- `--test-bars <N>`: out-of-sample bars per walk-forward segment
- `--step-bars <N>`: segment advance size; defaults to `test-bars`
- `--holdout-bars <N>`: reserve the final `N` execution bars as a final untouched holdout
- `--no-holdout`: explicitly disable the default untouched holdout reservation
- `--param ...`: search-space declaration; repeat for multiple tuned inputs, with optional integer/float step support
- `--objective ...`: ranking objective; defaults to `robust-return`
- `--trials <N>`: total bounded trial budget
- `--startup-trials <N>`: initial random trial count before the TPE search phase
- `--seed <N>`: deterministic optimizer seed
- `--workers <N>`: bounded parallel worker count
- `--top <N>`: number of top candidates to retain
- `--preset-out <path>`: write the best preset and top candidates to disk
- `--diagnostics summary|full-trace`: diagnostics detail mode; default `summary`
- `--format json|text`: output rendering format; default `json`

Default safety behavior:

- `walk-forward` is the default optimizer runner
- when `walk-forward` is used, the CLI reserves a final untouched holdout automatically
- the default holdout size matches `test-bars`
- if `--param` is omitted, PalmScript first looks for preset parameter space and then infers search space from `input ... optimize(...)` metadata inside the script
- repeated `--execution-source` flags activate portfolio mode, which evaluates the same compiled strategy logic for each selected alias under one shared equity ledger
- execution-oriented runs require explicit `--maker-fee-bps` and `--taker-fee-bps`; repeat `--fee-schedule <alias:maker:taker>` to override one selected alias
- trading scripts require at least one declared `execution` target in the script
- trading scripts also require matching explicit `order ...` templates for every declared `entry` / `exit` signal role
- portfolio scripts can declare `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, and `portfolio_group` to block entries that would exceed shared caps
- the final JSON/text result also carries holdout drift, top-candidate holdout robustness, parameter stability ranges, deterministic overfitting-risk summaries, and improvement hints

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
- `run paper` requires at least one declared `execution` target in the script
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
- the session snapshots the script source and queues it under the local execution state root
- v1 paper mode uses the existing VM and deterministic order simulator with closed-bar strategy evaluation, not real live order placement
- `paper-status` and `paper-export` now include shared live quote snapshots for each execution alias: top-of-book bid/ask, derived mid price, and venue last/mark prices when available

## `palmscript run paper-list`

```bash
palmscript run paper-list [--format json|text]
```

Lists the locally persisted paper session manifests.

## `palmscript run paper-status`

```bash
palmscript run paper-status <session-id> [--format json|text]
```

Reads the latest persisted paper-session snapshot.

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
- active paper sessions share one in-process quote cache per venue/symbol instead of duplicating upstream quote fetches
- the daemon status output now includes the current `subscription_count`

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
