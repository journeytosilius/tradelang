#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONFIG_PATH="$ROOT_DIR/web/docs/mkdocs.yml"
SITE_DIR="$ROOT_DIR/site"

build_locale() {
    local locale="$1"
    local site_url="$2"
    local output_dir="$3"

    BUILD_ONLY_LOCALE="$locale" \
    DOCS_SITE_URL="$site_url" \
    mkdocs build --strict -f "$CONFIG_PATH" -d "$output_dir"
}

rm -rf "$SITE_DIR"
mkdir -p "$SITE_DIR"

build_locale "en" "https://palmscript.dev/docs/" "$SITE_DIR/docs"
build_locale "es" "https://palmscript.dev/es/docs/" "$SITE_DIR/es/docs"
build_locale "pt-BR" "https://palmscript.dev/pt-BR/docs/" "$SITE_DIR/pt-BR/docs"
