#!/bin/sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)"
CONFIG_PATH="$ROOT_DIR/web/docs/mkdocs.yml"
SITE_DIR="$ROOT_DIR/site"
DOCS_ROOT="$ROOT_DIR/web/docs/docs"
LOCALES="es pt-BR de ja fr"
TMP_BASE="${TMPDIR:-$ROOT_DIR/.tmp}"
NORMALIZER="$ROOT_DIR/infra/scripts/normalize_docs_output.py"

if [ -x "$ROOT_DIR/.venv-docs/bin/mkdocs" ]; then
    MKDOCS_BIN="$ROOT_DIR/.venv-docs/bin/mkdocs"
else
    MKDOCS_BIN="mkdocs"
fi

mkdir -p "$TMP_BASE"

prepare_locale_docs_dir() {
    locale="$1"
    docs_dir="$(mktemp -d "$TMP_BASE/palmscript-docs.XXXXXX")"

    cp -RL "$DOCS_ROOT/." "$docs_dir/"

    for lang in $LOCALES; do
        find "$docs_dir" -type f -name "*.${lang}.md" -delete
    done

    if [ "$locale" != "en" ]; then
        find "$DOCS_ROOT" -type f -name "*.${locale}.md" | while IFS= read -r src; do
            rel="${src#"$DOCS_ROOT"/}"
            dest_rel="${rel%.$locale.md}.md"
            dest_dir="$(dirname "$docs_dir/$dest_rel")"
            mkdir -p "$dest_dir"
            cp "$src" "$docs_dir/$dest_rel"
        done
    fi

    printf '%s\n' "$docs_dir"
}

build_locale() {
    locale="$1"
    site_url="$2"
    output_dir="$3"
    docs_dir="$(prepare_locale_docs_dir "$locale")"

    BUILD_ONLY_LOCALE="$locale" \
    DOCS_SITE_URL="$site_url" \
    DOCS_DIR="$docs_dir" \
    "$MKDOCS_BIN" build --strict -f "$CONFIG_PATH" -d "$output_dir"

    rm -rf "$docs_dir"
}

rm -rf "$SITE_DIR"
mkdir -p "$SITE_DIR"

build_locale "en" "https://palmscript.dev/docs/" "$SITE_DIR/docs"
build_locale "es" "https://palmscript.dev/es/docs/" "$SITE_DIR/es/docs"
build_locale "pt-BR" "https://palmscript.dev/pt-BR/docs/" "$SITE_DIR/pt-BR/docs"
build_locale "de" "https://palmscript.dev/de/docs/" "$SITE_DIR/de/docs"
build_locale "ja" "https://palmscript.dev/ja/docs/" "$SITE_DIR/ja/docs"
build_locale "fr" "https://palmscript.dev/fr/docs/" "$SITE_DIR/fr/docs"

python3 "$NORMALIZER" "$SITE_DIR"
