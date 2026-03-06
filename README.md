# PalmScript

<p align="center">
  <img src="editors/vscode/images/palmscript.png" alt="PalmScript logo" width="220">
</p>

PalmScript is a deterministic DSL and bytecode VM for financial time-series strategies.

The repository currently ships:

- the Rust library crate
- the `palmscript` CLI
- the `palmscript-lsp` language server
- the first-party VS Code extension
- the MkDocs documentation site

## Documentation

The canonical documentation source is the MkDocs site under `docs/`.

- local source: [docs/index.md](docs/index.md)
- published site: <https://palmscript.dev/docs/>
- GitHub Pages mirror: <https://journeytosilius.github.io/palmscript/>

Start here:

- [Getting Started](docs/getting-started/overview.md)
- [Language Guide](docs/language/syntax.md)
- [CLI](docs/tooling/cli.md)
- [VS Code Extension](docs/tooling/vscode.md)

## Common Commands

```bash
cargo build --bin palmscript --bin palmscript-lsp
target/debug/palmscript check examples/strategies/sma_cross.palm
target/debug/palmscript run csv examples/strategies/sma_cross.palm --bars examples/data/minute_bars.csv
mkdocs build --strict
docker build -f Dockerfile.docs -t palmscript-docs .
```
