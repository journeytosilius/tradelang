# Repository Architecture

PalmScript is structured as a library-first repository. Tooling layers reuse the library instead of duplicating semantics.

## Main Areas

- `src/`: compiler, runtime, VM, IDE, and shared types
- `cli/`: CLI binary
- `lsp/`: language server binary
- `editors/vscode/`: VS Code extension
- `examples/`: Rust examples, `.palm` strategies, and fixtures
- `tests/`: integration and CLI coverage
- `docs/`: canonical documentation source

## Architectural Principle

Language and runtime behavior belongs in the library. Wrappers such as the CLI and LSP should translate inputs and outputs, not implement separate semantics.
