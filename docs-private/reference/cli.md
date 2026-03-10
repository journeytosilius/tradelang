# CLI Command Reference

This page is the compact public command reference for the `palmscript` CLI. For workflows and examples, see [CLI](../tooling/cli.md).

## `palmscript check`

```bash
palmscript check <script.palm>
```

Compiles and validates a script without executing it.

Arguments:

- `<script.palm>`: path to the PalmScript source file

## `palmscript run market`

```bash
palmscript run market <script.palm> --from <unix_ms> --to <unix_ms> \
  [--format json|text] \
  [--max-instructions-per-bar <N>] \
  [--max-history-capacity <N>]
```

Arguments and flags:

- `<script.palm>`: path to the PalmScript source file
- `--from <unix_ms>`: inclusive lower time bound in Unix milliseconds UTC
- `--to <unix_ms>`: exclusive upper time bound in Unix milliseconds UTC
- `--format json|text`: output rendering format, default `json`
- `--max-instructions-per-bar <N>`: VM instruction budget per step, default `10000`
- `--max-history-capacity <N>`: maximum retained history per series slot, default `1024`

Requirements:

- the script must declare at least one `source`
- `--from` must be strictly less than `--to`

## `palmscript dump-bytecode`

```bash
palmscript dump-bytecode <script.palm> [--format text|json]
```

Arguments and flags:

- `<script.palm>`: path to the PalmScript source file
- `--format text|json`: bytecode output format, default `text`
