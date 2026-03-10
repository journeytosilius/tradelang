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

## Common Commands

```bash
cargo build --bin palmscript
target/debug/palmscript check examples/strategies/sma_cross.palm
target/debug/palmscript run market examples/strategies/sma_cross.palm --from 1704067200000 --to 1704153600000
target/debug/palmscript run market examples/strategies/cross_source_spread.palm --from 1704067200000 --to 1704153600000
target/debug/palmscript dump-bytecode examples/strategies/sma_cross.palm
mkdocs build --strict
```
