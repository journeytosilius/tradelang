# Repository Architecture

PalmScript is structured as a library-first repository. Tooling layers reuse the library instead of duplicating semantics.

## Main Areas

- `crates/palmscript/`: compiler, runtime, VM, IDE analysis, exchange adapters, and shared types
- `apps/cli/`: CLI binary crate
- `apps/lsp/`: language server binary crate
- `apps/ide-server/`: hosted browser IDE HTTP server
- `web/ide/`: React, TypeScript, Vite, and Monaco frontend bundle
- `web/docs/`: MkDocs source plus repo-private documentation
- `editors/vscode/`: VS Code extension
- `infra/`: Dockerfiles, nginx config, and web build scripts

## Architectural Principle

Language and runtime behavior belongs in the library. Wrappers such as the CLI and LSP should translate inputs and outputs, not implement separate semantics.

## Exchange Adapter Boundary

Exchange-backed source ingestion also stays inside the library.

Rules for this layer:

- each supported source template is represented by typed Rust enums and structs
- venue request payloads and response payloads should use typed `serde` models rather than ad hoc positional JSON handling
- venue-specific shapes are normalized into the canonical PalmScript bar schema before they reach the runtime

Current ownership:

- `crates/palmscript/src/exchange/mod.rs` is the thin dispatch facade
- `crates/palmscript/src/exchange/binance/{spot,usdm}.rs` own Binance adapter logic and Binance risk metadata types
- `crates/palmscript/src/exchange/bybit/{spot,usdt_perps}.rs` own Bybit adapter logic and Bybit risk metadata types
- `crates/palmscript/src/exchange/gate/{spot,usdt_perps}.rs` own Gate adapter logic and Gate risk metadata types
- `crates/palmscript/src/exchange/common.rs` is limited to wire-agnostic helpers such as parsing, pagination windows, monotonic bar insertion, and Gate URL fallback handling

The backtest venue layer follows the same rule:

- `crates/palmscript/src/backtest/venue/{binance,bybit,gate}.rs` own exchange-facing order validation entrypoints
- `crates/palmscript/src/backtest/venue/common.rs` only contains explicitly shared rule helpers
- exchange modules must not route through another exchange's file just because behavior is currently identical
