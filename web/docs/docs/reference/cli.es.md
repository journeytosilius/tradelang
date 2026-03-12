# Referencia De Comandos CLI

Esta pagina vuelve a estar disponible publicamente porque PalmScript ahora es de codigo abierto. La localizacion completa se publicara en una actualizacion posterior. Mientras tanto, el contenido canonico en ingles se incluye abajo para que esta version del sitio exponga la misma superficie publica de CLI y herramientas.

## English Canonical Content


This page is the compact public command reference for the `palmscript` CLI. For workflows and examples, see [CLI](../tooling/cli.md).

## `palmscript check`

```bash
palmscript check <script.ps>
```

Compiles and validates a script without executing it.

Arguments:

- `<script.ps>`: path to the PalmScript source file

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
