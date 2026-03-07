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
- `--format json|text`: output rendering format, default `json`
- `--max-instructions-per-bar <N>`: VM instruction budget per step, default `10000`
- `--max-history-capacity <N>`: maximum retained history per series slot, default `1024`

Requirements:

- the script must declare at least one `source`
- the script must emit at least one configured backtest trigger
- `--from` must be strictly less than `--to`
- `--execution-source` is required when the script declares multiple sources

## `palmscript dump-bytecode`

```bash
palmscript dump-bytecode <script.palm> [--format text|json]
```

Arguments and flags:

- `<script.palm>`: path to the strategy source file
- `--format text|json`: bytecode output format, default `text`
