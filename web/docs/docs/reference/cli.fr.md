# Reference Des Commandes CLI

Cette page est de nouveau publique parce que PalmScript est maintenant open source. Une localisation complete sera publiee dans une mise a jour ulterieure. En attendant, le contenu canonique en anglais est inclus ci-dessous afin que cette version du site expose la meme surface publique CLI et tooling.

## English Canonical Content


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

## `palmscript run optimize`

```bash
palmscript run optimize <script.ps> --from <unix_ms> --to <unix_ms> \
  [--runner walk-forward|backtest] \
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
  [--format json|text]
```

Arguments and flags:

- `<script.ps>`: path to the PalmScript source file
- `--from <unix_ms>`: inclusive lower time bound in Unix milliseconds UTC
- `--to <unix_ms>`: exclusive upper time bound in Unix milliseconds UTC
- `--runner`: optimize evaluation mode; defaults to `walk-forward`
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
- `--format json|text`: output rendering format; default `json`

Default safety behavior:

- `walk-forward` is the default optimizer runner
- when `walk-forward` is used, the CLI reserves a final untouched holdout automatically
- the default holdout size matches `test-bars`
- if `--param` is omitted, PalmScript first looks for preset parameter space and then infers search space from `input ... optimize(...)` metadata inside the script

## `palmscript runs serve`

```bash
palmscript runs serve
```

Runs the local optimize daemon loop over queued durable runs.

Arguments and flags:

- no public flags in v1

Notes:

- the hidden `--once` and `--poll-ms` flags are reserved for internal orchestration and tests

## `palmscript runs submit optimize`

```bash
palmscript runs submit optimize <script.ps> ...same flags as `palmscript run optimize`...
```

Queues a durable local optimize job and writes a source snapshot plus run artifacts into the PalmScript state directory.

Output:

- `run_id=<id>`
- `status=queued`
- `artifact_dir=<path>`

Default safety behavior:

- when the optimizer runs in the default walk-forward mode, the CLI reserves a final untouched holdout window automatically
- the default holdout size is `test-bars`
- use `--holdout-bars <N>` to reserve a different final holdout size
- use `--no-holdout` only when you intentionally want to disable that protection
- when `--param` is omitted, durable optimize submission infers its search space from the preset or compiled `input ... optimize(...)` metadata exactly like `palmscript run optimize`

## `palmscript runs status`

```bash
palmscript runs status <run-id>
```

Prints compact run status including progress, best score, and artifact directory.

## `palmscript runs show`

```bash
palmscript runs show <run-id>
```

Prints the persisted manifest for one durable run, including best candidate and failure information when present.

## `palmscript runs tail`

```bash
palmscript runs tail <run-id>
```

Streams persisted run events until the durable job reaches `completed`, `failed`, or `canceled`.

## `palmscript runs list`

```bash
palmscript runs list
```

Lists durable optimize runs in reverse creation order.

## `palmscript runs cancel`

```bash
palmscript runs cancel <run-id>
```

Cancels a queued durable run immediately or marks a running durable run for cooperative cancellation between optimizer batches.

## `palmscript runs resume`

```bash
palmscript runs resume <run-id>
```

Requeues a canceled or failed durable run from its persisted source snapshot and candidate state.

## `palmscript runs best`

```bash
palmscript runs best <run-id> [--preset-out <path>]
```

Prints the best known overrides for a durable run, or exports them as an optimize preset when `--preset-out` is provided.

## `palmscript dump-bytecode`

```bash
palmscript dump-bytecode <script.ps> [--format text|json]
```

Arguments and flags:

- `<script.ps>`: path to the PalmScript source file
- `--format text|json`: bytecode output format, default `text`
