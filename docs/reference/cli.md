# CLI Command Reference

This page is the compact command and flag reference for the `palmscript` CLI. For workflows and examples, see [CLI](../tooling/cli.md).

## `palmscript check`

```bash
palmscript check <script.palm>
```

Compiles and validates a strategy without executing it.

Arguments:

- `<script.palm>`: path to the strategy source file

## `palmscript run market`

```bash
palmscript run market <script.palm> --from <unix_ms> --to <unix_ms> \
  [--format json|text] \
  [--max-instructions-per-bar <N>] \
  [--max-history-capacity <N>]
```

Arguments and flags:

- `<script.palm>`: path to the strategy source file
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
palmscript run backtest <script.palm> --from <unix_ms> --to <unix_ms> \
  [--execution-source <alias>] \
  [--initial-capital <amount>] \
  [--fee-bps <bps>] \
  [--slippage-bps <bps>] \
  [--leverage <N>] \
  [--margin-mode isolated] \
  [--format json|text] \
  [--max-instructions-per-bar <N>] \
  [--max-history-capacity <N>]
```

Arguments and flags:

- `<script.palm>`: path to the strategy source file
- `--from <unix_ms>`: inclusive lower time bound in Unix milliseconds UTC
- `--to <unix_ms>`: exclusive upper time bound in Unix milliseconds UTC
- `--execution-source <alias>`: source alias used for fills when the script declares multiple sources
- `--initial-capital <amount>`: starting equity, default `10000`
- `--fee-bps <bps>`: fee charged per fill in basis points, default `5`
- `--slippage-bps <bps>`: slippage applied to each fill in basis points, default `2`
- `--leverage <N>`: isolated leverage for perp execution sources, default `1`
- `--margin-mode isolated`: isolated perp margin mode; this is the only accepted v1 value
- `--format json|text`: output rendering format, default `json`
- `--max-instructions-per-bar <N>`: VM instruction budget per step, default `10000`
- `--max-history-capacity <N>`: maximum retained history per series slot, default `1024`

Requirements:

- the script must declare at least one `source`
- the script must emit at least one configured backtest trigger
- the script may optionally declare attached exits through `protect` / `target`
- the script may optionally size staged `order entry ...` fills with `size entry1..3 long|short ...`
- the script may optionally size staged attached `target` exits with `size target1..3 long|short ...`
- `--from` must be strictly less than `--to`
- `--execution-source` is required when the script declares multiple sources
- spot execution sources reject `--leverage` and `--margin-mode`
- Binance USD-M perp runs use live signed leverage brackets when `PALMSCRIPT_BINANCE_USDM_API_KEY` and `PALMSCRIPT_BINANCE_USDM_API_SECRET` are available; otherwise they fall back to an approximate public `exchangeInfo` risk snapshot

Backtest output:

- JSON includes runtime `outputs`, order lifecycle records in `orders`, fills, trades, backtest diagnostics in `diagnostics`, equity, summary, any open position, and optional perp metadata in `perp`
- text output renders summary metrics plus diagnostics, order, and trade sections, with compact export and opportunity summaries when available

## `palmscript run walk-forward`

```bash
palmscript run walk-forward <script.palm> --from <unix_ms> --to <unix_ms> \
  [--execution-source <alias>] \
  [--initial-capital <amount>] \
  [--fee-bps <bps>] \
  [--slippage-bps <bps>] \
  [--leverage <N>] \
  [--margin-mode isolated] \
  --train-bars <N> \
  --test-bars <N> \
  [--step-bars <N>] \
  [--format json|text] \
  [--max-instructions-per-bar <N>] \
  [--max-history-capacity <N>]
```

Arguments and flags:

- `<script.palm>`: path to the strategy source file
- `--from <unix_ms>`: inclusive lower time bound in Unix milliseconds UTC
- `--to <unix_ms>`: exclusive upper time bound in Unix milliseconds UTC
- `--execution-source <alias>`: source alias used for fills when the script declares multiple sources
- `--initial-capital <amount>`: starting equity for each stitched out-of-sample run, default `10000`
- `--fee-bps <bps>`: fee charged per fill in basis points, default `5`
- `--slippage-bps <bps>`: slippage applied to each fill in basis points, default `2`
- `--leverage <N>`: isolated leverage for perp execution sources, default `1`
- `--margin-mode isolated`: isolated perp margin mode; this is the only accepted v1 value
- `--train-bars <N>`: in-sample context window size in execution bars
- `--test-bars <N>`: out-of-sample window size in execution bars
- `--step-bars <N>`: segment advance in execution bars, default `test-bars`
- `--format json|text`: output rendering format, default `json`
- `--max-instructions-per-bar <N>`: VM instruction budget per step, default `10000`
- `--max-history-capacity <N>`: maximum retained history per series slot, default `1024`

Requirements:

- the script must declare at least one `source`
- the script must emit at least one configured backtest trigger
- `--train-bars`, `--test-bars`, and `--step-bars` must be positive
- `--from` must be strictly less than `--to`
- `--execution-source` is required when the script declares multiple sources
- spot execution sources reject `--leverage` and `--margin-mode`
- Binance USD-M perp runs use live signed leverage brackets when `PALMSCRIPT_BINANCE_USDM_API_KEY` and `PALMSCRIPT_BINANCE_USDM_API_SECRET` are available; otherwise they fall back to an approximate public `exchangeInfo` risk snapshot

Walk-forward output:

- JSON includes per-segment `in_sample` and `out_of_sample` summaries, per-segment `out_of_sample_diagnostics`, a stitched out-of-sample summary, and a stitched out-of-sample equity curve
- text output renders a stitched summary, the configured walk-forward window sizes, recent segment rows, and a short weakest-segment section
- v1 does not auto-optimize parameters; it evaluates the fixed script/inputs over rolling train/test slices

## `palmscript dump-bytecode`

```bash
palmscript dump-bytecode <script.palm> [--format text|json]
```

Arguments and flags:

- `<script.palm>`: path to the strategy source file
- `--format text|json`: bytecode output format, default `text`
