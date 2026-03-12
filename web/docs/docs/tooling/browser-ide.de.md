# Browser-IDE

Diese Seite ist wieder oeffentlich verfuegbar, weil PalmScript jetzt Open Source ist. Eine vollstaendige Lokalisierung folgt in einer spaeteren Aktualisierung. Bis dahin steht unten der kanonische englische Inhalt, damit diese Sprachversion dieselbe oeffentliche CLI- und Tooling-Oberflaeche zeigt.

## English Canonical Content


The hosted PalmScript browser IDE is backed by:

- `palmscript-ide-server` for HTTP, websocket LSP transport, public dataset metadata, and hosted backtests
- the shared `IdeLspSession` library core for both stdio LSP and browser websocket sessions
- a Vite-built React and TypeScript frontend with Monaco Editor, embedded by the server for editing, diagnostics, summary rendering, and hosted backtest execution

## Run locally

```bash
bash infra/scripts/build_ide_web.sh
cargo build --bin palmscript-ide-server
target/debug/palmscript-ide-server
```

Default bind address:

- `127.0.0.1:8080`
- local entrypoint: `/`
- hosted reverse-proxy entrypoint: `/app/`

Override with:

```bash
PALMSCRIPT_IDE_BIND=0.0.0.0:8080 target/debug/palmscript-ide-server
```

## Container image

```bash
docker build -f infra/docker/Dockerfile.ide -t palmscript-ide .
docker run --rm -p 8080:8080 palmscript-ide
```

The image embeds the browser shell and serves the full IDE from the
`palmscript-ide-server` binary.

## Public IDE constraints

The first public IDE release is intentionally narrow:

- one `.ps` buffer
- minimal demo chrome with Monaco Editor, calendar date-range pickers, and a run action only
- light/dark mode switch in the header
- Inter as the shell UI font
- native browser clipboard behavior through Monaco
- compiler diagnostics rendered into Monaco markers
- Monaco hover cards, completion lists, and callable completion snippets backed by the shared PalmScript IDE metadata
- PalmScript logo mark in the header instead of a text heading
- browser tab favicon generated from the current PalmScript logo
- anonymous ephemeral browser sessions
- one hosted BTCUSDT Binance + Gate spot dataset with a two-exchange starter demo and the available history windowed by the selected date range
- live compile diagnostics shown above a formatted backtest summary plus hosted backtest execution
- no walk-forward, optimize, market mode, or arbitrary exchange fetches

Operationally, the hosted dataset fetch path now caps Gate requests to the
venue's 1000-candle public limit per HTTP call. When the IDE server still gets
a venue-side fetch failure, the logged error includes the request URL and a
truncated response-body snippet for both non-200 HTTP responses and malformed
JSON payloads so production debugging stays actionable.

Dark mode uses a VS Code-like shell palette with a Dracula-style Monaco theme.
VS Code and the hosted Monaco editor now share builtin signatures, summaries,
and callable completion snippets through the same `ide.rs` metadata and LSP
completion items. Completion fallback stays active even while the current line
is syntactically incomplete, so builtin suggestions remain available mid-edit.

The websocket endpoint remains available on the backend, but the current React
shell does not yet wire the browser UI into the websocket LSP transport.

## HTTP surface

- `GET /api/healthz`
- `GET /api/examples`
- `GET /api/datasets`
- `POST /api/check`
- `POST /api/backtest`
- `WS /api/lsp`

The hosted deployment also exposes the same surface under `/app/...`. The
public docs nginx front door normalizes `https://palmscript.dev/app` to
`/app/` and proxies `/app/...` to
`https://palmscript-ide-backend-production.up.railway.app`.

## Session and resource limits

- max script size: `128 KiB`
- max one active backtest per session
- backtest timeout: `30s`
- bounded backtest worker pool
- bounded concurrent websocket LSP sessions
