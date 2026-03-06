# Installation

## Build the CLI and Language Server

From the repository root:

```bash
cargo build --bin tradelang --bin tradelang-lsp
```

The binaries will be available at:

- `target/debug/tradelang`
- `target/debug/tradelang-lsp`

Use `--release` for optimized builds:

```bash
cargo build --release --bin tradelang --bin tradelang-lsp
```

## Install Python Dependencies for Documentation

```bash
python -m venv .venv-docs
source .venv-docs/bin/activate
pip install -r requirements-docs.txt
```

Then serve or build the docs:

```bash
mkdocs serve
mkdocs build --strict
```

Or use the repository `Makefile` helpers:

```bash
make docs-serve
make docs-build-strict
```

## Install VS Code Extension Dependencies

```bash
cd editors/vscode
npm install
npm run compile
```

The extension can use bundled `tradelang-lsp` binaries or fall back to a locally built repo binary during development.
