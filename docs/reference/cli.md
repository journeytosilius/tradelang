# CLI Command Reference

## `tradelang check`

```bash
tradelang check <script.trl> [--env <compile-env.json>]
```

Compiles and validates a strategy without running it.

## `tradelang run csv`

```bash
tradelang run csv <script.trl> --bars <bars.csv> \
  [--format json|text] \
  [--max-instructions-per-bar N] \
  [--max-history-capacity N]
```

Runs a strategy in CSV mode. The input file is treated as the raw source feed and rolled up to declared intervals if possible.

## `tradelang dump-bytecode`

```bash
tradelang dump-bytecode <script.trl> \
  [--env <compile-env.json>] \
  [--format text|json]
```

Compiles a strategy and prints the program in text or JSON form.
