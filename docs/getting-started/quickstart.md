# Quickstart

## 1. Check a Strategy

```bash
target/debug/tradelang check examples/strategies/sma_cross.trl
```

## 2. Run a Strategy in CSV Mode

```bash
target/debug/tradelang run csv examples/strategies/sma_cross.trl \
  --bars examples/data/minute_bars.csv
```

CSV mode is the only `run` mode today. It accepts one raw market-data file, infers its source interval, and rolls it up into the strategy's declared `interval` and `use` intervals when possible.

## 3. Inspect Bytecode

```bash
target/debug/tradelang dump-bytecode examples/strategies/sma_cross.trl
```

## 4. Open the Project in VS Code

- install the TradeLang extension
- open a `.trl` file
- diagnostics, completions, hovers, definitions, document symbols, and formatting are provided by `tradelang-lsp`

## 5. Build and Serve the Docs

```bash
python -m venv .venv-docs
source .venv-docs/bin/activate
pip install -r requirements-docs.txt
mkdocs serve
```
