# Testing Expectations

PalmScript is a financial computation engine, so tests are mandatory for non-trivial changes.

## Required Quality Gate

Before completing a change:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
mkdocs build --strict
```

## Expected Test Coverage

Depending on the change, add or update:

- lexer and parser tests
- semantic/compiler tests
- VM/runtime tests
- CLI integration tests
- pipeline tests
- language server and VS Code tests
- documentation links and command examples when docs-facing behavior changes

Regression fixes should include a regression test whenever practical.
