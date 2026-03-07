# Quickstart

## 1. Build The Binaries

```bash
cargo build --bin palmscript --bin palmscript-lsp
```

## 2. Validate A Strategy

```bash
target/debug/palmscript check strategy.palm
```

## 3. Run A Market-Backed Strategy

```bash
target/debug/palmscript run market strategy.palm \
  --from 1704067200000 \
  --to 1704153600000
```
## 4. Run Another Exchange-Backed Strategy

```bash
target/debug/palmscript run market spread_strategy.palm \
  --from 1704067200000 \
  --to 1704153600000
```

See [Market Mode](../tooling/market-mode.md) for supported source templates and fetch behavior.

## 5. Inspect Compiled Output

```bash
target/debug/palmscript dump-bytecode strategy.palm
```

## 6. Use The Editor Tooling

- install or build the PalmScript VS Code extension
- open a `.palm` file
- use diagnostics, formatting, hover, completion, definitions, and document symbols from `palmscript-lsp`

Next: [First Strategy](first-strategy.md)
