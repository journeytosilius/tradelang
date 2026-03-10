# PalmScript

<p align="center">
  <img src="editors/vscode/images/palmscript.png" alt="PalmScript logo" width="220">
</p>

PalmScript is a language for financial time-series strategies.

The public documentation is focused on:

- language syntax and semantics
- builtins and indicators
- examples and learning guides
- the basic CLI flow for checking and running scripts

Documentation:

- published site: <https://palmscript.dev/docs/>
- local source: [docs/index.md](docs/index.md)

Start here:

- [Learn](docs/learn/overview.md)
- [Language Reference](docs/reference/overview.md)
- [Indicators Reference](docs/reference/indicators.md)

Repo-local tooling docs:

- [Browser IDE](docs-private/tooling/browser-ide.md)

## Common Commands

```bash
cargo build --bin palmscript
cargo build --bin palmscript-ide-server
target/debug/palmscript check examples/strategies/sma_cross.palm
target/debug/palmscript run market examples/strategies/sma_cross.palm --from 1704067200000 --to 1704153600000
target/debug/palmscript run market examples/strategies/cross_source_spread.palm --from 1704067200000 --to 1704153600000
target/debug/palmscript dump-bytecode examples/strategies/sma_cross.palm
mkdocs build --strict
```

## Browser IDE Container

```bash
docker build -f Dockerfile.ide -t palmscript-ide .
docker run --rm -p 8080:8080 palmscript-ide
```

The browser IDE shell uses the same blue-grey and accent-blue visual language
as the published docs at <https://palmscript.dev/docs/>.

The public demo keeps the chrome intentionally minimal: one editor buffer, a
curated dataset selector, diagnostics, and backtest output panels.
