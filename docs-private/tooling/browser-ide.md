# Browser IDE

The hosted PalmScript browser IDE is backed by:

- `palmscript-ide-server` for HTTP, websocket LSP transport, curated dataset metadata, and curated backtests
- the shared `IdeLspSession` library core for both stdio LSP and browser websocket sessions
- a single-file browser workspace with Monaco-based editing, syntax highlighting, semantic tokens, diagnostics, hover, completion, formatting, and curated backtest execution

## Run locally

```bash
cargo build --bin palmscript-ide-server
target/debug/palmscript-ide-server
```

Default bind address:

- `127.0.0.1:8080`

Override with:

```bash
PALMSCRIPT_IDE_BIND=0.0.0.0:8080 target/debug/palmscript-ide-server
```

## Container image

```bash
docker build -f Dockerfile.ide -t palmscript-ide .
docker run --rm -p 8080:8080 palmscript-ide
```

The image embeds the browser shell and serves the full IDE from the
`palmscript-ide-server` binary.

## Public IDE constraints

The first public IDE release is intentionally narrow:

- one `.palm` buffer
- minimal demo chrome with a dataset selector and run action only
- anonymous ephemeral browser sessions
- curated BTCUSDT Binance spot datasets only
- live LSP diagnostics plus curated backtest execution
- no walk-forward, optimize, market mode, or arbitrary exchange fetches

## HTTP surface

- `GET /api/healthz`
- `GET /api/examples`
- `GET /api/datasets`
- `POST /api/check`
- `POST /api/backtest`
- `WS /api/lsp`

## Session and resource limits

- max script size: `128 KiB`
- max one active backtest per session
- backtest timeout: `30s`
- bounded backtest worker pool
- bounded concurrent websocket LSP sessions
