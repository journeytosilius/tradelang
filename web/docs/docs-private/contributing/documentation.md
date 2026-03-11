# Documentation Maintenance

The MkDocs site under `web/docs/docs/` is the canonical documentation source for PalmScript.
English is the canonical default locale, published at `/docs/`.

## Documentation Structure

The site is organized into these top-level areas:

- `Learn`: onboarding, workflows, and guided usage
- `Reference`: the normative language and CLI definition
- `Tooling`: CLI modes, editor integrations, and operational behavior
- `Internals`: implementation architecture for contributors
- `Contributing`: repository workflow and maintenance guidance

Localized content lives beside the English source with locale suffixes such as
`index.es.md`. The hosted URL scheme is:

- English: `/docs/`
- Spanish: `/es/docs/`
- Portuguese (Brazil): `/pt-BR/docs/`
- German: `/de/docs/`
- Japanese: `/ja/docs/`
- French: `/fr/docs/`
- future locales: `/{lang}/docs/`

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
pip install -r web/docs/requirements-docs.txt
mkdocs serve -f web/docs/mkdocs.yml
mkdocs build --strict -f web/docs/mkdocs.yml
bash infra/scripts/build_docs_site.sh
```

Use `mkdocs build --strict -f web/docs/mkdocs.yml` for config validation and
local preview of the multilingual source tree. Use
`bash infra/scripts/build_docs_site.sh` for the production output layout that
publishes English at `/docs/` and translated locales at `/{lang}/docs/`.

For containerized serving or self-hosting:

```bash
docker build -f infra/docker/Dockerfile.docs -t palmscript-docs .
docker run --rm -p 8080:8080 palmscript-docs
```

The container publishes the static docs site under `http://127.0.0.1:8080/docs/`.
It also serves translated locales under `http://127.0.0.1:8080/{lang}/docs/`.
It does not serve the site homepage at `/`; that host-level routing belongs to
the external front proxy.

## Repository-Local Docs

- keep the root `README.md` short and link into the canonical docs site
- keep `examples/README.md` and `editors/vscode/README.md` as short entrypoint notes, not parallel documentation sets
- keep commands, flags, filenames, and example snippets synchronized with the implementation
- docs changes belong in the same change as the behavior change they describe
- keep `infra/docker/Dockerfile.docs`, `infra/docker/Dockerfile.ide`, `infra/docker/docs-nginx.conf`, and Docker-related instructions in sync when the docs or hosted IDE serving model changes
