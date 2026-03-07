# Documentation Maintenance

The MkDocs site under `docs/` is the canonical documentation source for PalmScript.

## Documentation Structure

The site is organized into these top-level areas:

- `Learn`: onboarding, workflows, and guided usage
- `Reference`: the normative language and CLI definition
- `Tooling`: CLI modes, editor integrations, and operational behavior
- `Internals`: implementation architecture for contributors
- `Contributing`: repository workflow and maintenance guidance

## When To Update Docs

Update documentation whenever a change affects:

- language syntax or semantics
- runtime behavior
- CLI commands or flags
- market mode behavior
- editor tooling behavior
- released artifacts or workflows
- examples or checked-in fixtures used in docs

## Reference-First Rules

- language behavior changes must update `Reference` first
- guide pages may teach or summarize, but they must not override `Reference`
- command or flag changes must update both the CLI guide and the CLI command reference when both are affected
- keep docs examples inline and self-contained; repository demo strategies may evolve independently outside the docs tree
- extend an existing page before creating a near-duplicate page

## Language-Doc Audit Checklist

When a change touches language behavior, audit the docs against these implementation truth sources before you finish:

- `src/token.rs` for reserved keywords and token-level surface
- `src/ast.rs` for source-level nodes and binding forms
- `src/builtins.rs` for reserved names, signatures, and builtin categories
- `tests/parser.rs` for accepted syntax and parser-facing restrictions
- `tests/diagnostics_compile.rs` for public compile-time diagnostic contracts
- `tests/vm.rs` for runtime truth tables and VM-visible semantics

The goal is to keep the docs aligned with what the parser, compiler, and VM actually enforce today.

## Information Architecture Mapping

The current documentation layout replaces the older structure with this mapping:

| Previous area | Current destination |
| --- | --- |
| `getting-started/*` | `learn/*` |
| `language/*` | split between `learn/*` and `reference/*` |
| `runtime/market-mode.md` | `tooling/market-mode.md` |
| `runtime/*` internals pages | `internals/*` |
| `examples/*` | `learn/cookbook/*` or `internals/rust-examples.md` |

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

## Repository-Local Docs

- keep the root `README.md` short and link into the canonical docs site
- keep `examples/README.md` and `editors/vscode/README.md` as short entrypoint notes, not parallel documentation sets
- keep commands, flags, filenames, and example snippets synchronized with the implementation
- docs changes belong in the same change as the behavior change they describe
- keep `Dockerfile.docs`, `docker/docs-nginx.conf`, and Docker docs instructions in sync when the docs build or serving model changes
