# Overview

TradeLang exists in three layers:

- the Rust library, which owns lexing, parsing, semantic analysis, bytecode generation, VM execution, input preparation, pipeline execution, and IDE analysis
- the `tradelang` CLI, which runs scripts directly
- the editor stack, built from `tradelang-lsp` plus the VS Code extension

## Repository Outputs

- `tradelang`: CLI for `check`, `run csv`, and `dump-bytecode`
- `tradelang-lsp`: stdio language server used by editors
- `editors/vscode/`: the first-party VS Code extension

## How To Use The Project

- Write `.trl` strategies with an `interval <...>` directive and optional `use <...>` declarations.
- Validate them with `tradelang check`.
- Execute them with `tradelang run csv ...`.
- Inspect compiled output with `tradelang dump-bytecode`.
- Author them interactively with the VS Code extension.

## Required Background

You do not need Rust to use TradeLang from the CLI or VS Code. You only need Rust when building the binaries from source or embedding the library directly.
