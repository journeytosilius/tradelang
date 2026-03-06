# Installation

## Build the CLI and Language Server

From the repository root:

```bash
cargo build --bin palmscript --bin palmscript-lsp
```

The binaries will be available at:

- `target/debug/palmscript`
- `target/debug/palmscript-lsp`

Use `--release` for optimized builds:

```bash
cargo build --release --bin palmscript --bin palmscript-lsp
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

## Serve Documentation with Docker and nginx

The repository also provides a dedicated docs container image. It builds the MkDocs site and serves the static output through nginx.

Build the image:

```bash
docker build -f Dockerfile.docs -t palmscript-docs .
```

Run it locally:

```bash
docker run --rm -p 8080:8080 palmscript-docs
```

Then open:

```text
http://127.0.0.1:8080/docs/
```

Equivalent `Makefile` helpers:

```bash
make docs-docker-build
make docs-docker-run
```

## Install VS Code Extension Dependencies

```bash
cd editors/vscode
npm install
npm run compile
```

The extension can use bundled `palmscript-lsp` binaries or fall back to a locally built repo binary during development.
