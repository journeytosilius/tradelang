# TradeLang

TradeLang is a deterministic DSL and bytecode VM for financial time-series strategies.

The repository currently ships:

- the Rust library crate
- the `tradelang` CLI
- the `tradelang-lsp` language server
- the first-party VS Code extension
- the MkDocs documentation site

## Documentation

The canonical documentation source is the MkDocs site under `docs/`.

- local source: [docs/index.md](docs/index.md)
- published site: <https://journeytosilius.github.io/tradelang/>

Start here:

- [Getting Started](docs/getting-started/overview.md)
- [Language Guide](docs/language/syntax.md)
- [CLI](docs/tooling/cli.md)
- [VS Code Extension](docs/tooling/vscode.md)

## Common Commands

```bash
cargo build --bin tradelang --bin tradelang-lsp
target/debug/tradelang check examples/strategies/sma_cross.trl
target/debug/tradelang run csv examples/strategies/sma_cross.trl --bars examples/data/minute_bars.csv
mkdocs build --strict
```
