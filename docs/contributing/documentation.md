# Documentation Maintenance

The MkDocs site under `docs/` is the canonical documentation source for PalmScript.

## When To Update Docs

Update documentation whenever a change affects:

- language syntax or semantics
- runtime behavior
- CLI commands or flags
- CSV mode or roll-up behavior
- editor tooling behavior
- released artifacts or workflows
- examples or checked-in fixtures used in docs

## How To Work On Docs

```bash
python -m venv .venv-docs
source .venv-docs/bin/activate
pip install -r requirements-docs.txt
mkdocs serve
mkdocs build --strict
```

For containerized serving or self-hosting:

```bash
docker build -f Dockerfile.docs -t palmscript-docs .
docker run --rm -p 8080:8080 palmscript-docs
```

The container publishes the static docs site under `http://127.0.0.1:8080/docs/`.

## Documentation Rules

- extend existing relevant pages before creating near-duplicate pages
- keep the root README short and link into the canonical site
- keep commands, flags, filenames, and example snippets synchronized with the implementation
- docs changes belong in the same change as the behavior change they describe
- keep `Dockerfile.docs`, `docker/docs-nginx.conf`, and Docker docs instructions in sync when the docs build or serving model changes
