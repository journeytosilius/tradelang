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
  [--fee-bps <N>] \
  [--slippage-bps <N>] \
  [--diagnostics summary|full-trace] \
  [--format json|text]
```

Additional diagnostics flag:

- `--diagnostics summary|full-trace`: diagnostics detail mode; default `summary`
- repeat `--execution-source <alias>` to activate portfolio mode with a shared equity ledger across the selected execution aliases

## `palmscript run walk-forward`

```bash
palmscript run walk-forward <script.ps> --from <unix_ms> --to <unix_ms> \
  --train-bars <N> --test-bars <N> [--step-bars <N>] \
  [--execution-source <alias>]... \
  [--diagnostics summary|full-trace] \
  [--format json|text]
```

Additional diagnostics flag:

- `--diagnostics summary|full-trace`: diagnostics detail mode; default `summary`
- repeat `--execution-source <alias>` to activate portfolio mode with a shared equity ledger across the selected execution aliases

## `palmscript run optimize`

```bash
palmscript run optimize <script.ps> --from <unix_ms> --to <unix_ms> \
  [--runner walk-forward|backtest] \
  [--execution-source <alias>]... \
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
- portfolio scripts can declare `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, and `portfolio_group` to block entries that would exceed shared caps
- the final JSON/text result also carries holdout drift, top-candidate holdout robustness, parameter stability ranges, and deterministic improvement hints

## `palmscript dump-bytecode`

```bash
palmscript dump-bytecode <script.ps> [--format text|json]
```

Arguments and flags:

- `<script.ps>`: path to the PalmScript source file
- `--format text|json`: bytecode output format, default `text`
