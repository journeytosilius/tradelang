#!/usr/bin/env python3

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
import argparse
import re
import xml.etree.ElementTree as ET

ET.register_namespace("", "http://www.sitemaps.org/schemas/sitemap/0.9")
ET.register_namespace("xhtml", "http://www.w3.org/1999/xhtml")


@dataclass(frozen=True)
class LocaleConfig:
    locale: str
    path_prefix: str
    site_url: str


LOCALES = (
    LocaleConfig("en", "docs", "https://palmscript.dev/docs/"),
    LocaleConfig("es", "es/docs", "https://palmscript.dev/es/docs/"),
    LocaleConfig("pt-BR", "pt-BR/docs", "https://palmscript.dev/pt-BR/docs/"),
    LocaleConfig("de", "de/docs", "https://palmscript.dev/de/docs/"),
    LocaleConfig("ja", "ja/docs", "https://palmscript.dev/ja/docs/"),
    LocaleConfig("fr", "fr/docs", "https://palmscript.dev/fr/docs/"),
)

CANONICAL_RE = re.compile(r'(<link rel="canonical" href=")([^"]+)(")')


def page_rel_path(html_path: Path, locale_root: Path) -> str | None:
    rel = html_path.relative_to(locale_root).as_posix()
    if rel == "404.html" or rel.endswith("/404.html"):
        return None
    if rel == "index.html":
        return ""
    if rel.endswith("/index.html"):
        return rel[: -len("/index.html")]
    if rel.endswith(".html"):
        return rel[: -len(".html")]
    return rel


def page_url(site_url: str, rel_path: str) -> str:
    if not rel_path:
        return site_url
    return f"{site_url}{rel_path}/"


def locale_roots(site_root: Path) -> dict[str, Path]:
    return {config.locale: site_root / config.path_prefix for config in LOCALES}


def collect_pages(site_root: Path) -> dict[str, set[str]]:
    pages: dict[str, set[str]] = {}
    for config in LOCALES:
        root = site_root / config.path_prefix
        locale_pages: set[str] = set()
        if root.exists():
            for html_path in root.rglob("*.html"):
                rel_path = page_rel_path(html_path, root)
                if rel_path is not None:
                    locale_pages.add(rel_path)
        pages[config.locale] = locale_pages
    return pages


def normalize_canonicals(site_root: Path) -> None:
    for config in LOCALES:
        root = site_root / config.path_prefix
        if not root.exists():
            continue
        for html_path in root.rglob("*.html"):
            rel_path = page_rel_path(html_path, root)
            if rel_path is None:
                continue
            canonical = page_url(config.site_url, rel_path)
            text = html_path.read_text()
            updated = CANONICAL_RE.sub(rf"\1{canonical}\3", text, count=1)
            html_path.write_text(updated)


def write_locale_sitemap(
    site_root: Path,
    config: LocaleConfig,
    pages_by_locale: dict[str, set[str]],
) -> None:
    root = site_root / config.path_prefix
    if not root.exists():
        return

    urlset = ET.Element(
        "urlset",
        {"xmlns": "http://www.sitemaps.org/schemas/sitemap/0.9"},
    )
    today = datetime.now(timezone.utc).date().isoformat()
    page_paths = sorted(pages_by_locale[config.locale])
    for rel_path in page_paths:
        url = ET.SubElement(urlset, "url")
        ET.SubElement(url, "loc").text = page_url(config.site_url, rel_path)
        ET.SubElement(url, "lastmod").text = today
        ET.SubElement(url, "changefreq").text = "daily"
        for alt in LOCALES:
            if rel_path not in pages_by_locale[alt.locale]:
                continue
            ET.SubElement(
                url,
                "{http://www.w3.org/1999/xhtml}link",
                {
                    "rel": "alternate",
                    "hreflang": alt.locale,
                    "href": page_url(alt.site_url, rel_path),
                },
            )

    tree = ET.ElementTree(urlset)
    ET.indent(tree, space="    ")
    tree.write(root / "sitemap.xml", encoding="utf-8", xml_declaration=True)


def write_root_sitemap_index(site_root: Path) -> None:
    today = datetime.now(timezone.utc).date().isoformat()
    index = ET.Element(
        "sitemapindex",
        {"xmlns": "http://www.sitemaps.org/schemas/sitemap/0.9"},
    )
    for config in LOCALES:
        entry = ET.SubElement(index, "sitemap")
        ET.SubElement(entry, "loc").text = f"https://palmscript.dev/{config.path_prefix}/sitemap.xml"
        ET.SubElement(entry, "lastmod").text = today

    tree = ET.ElementTree(index)
    ET.indent(tree, space="    ")
    tree.write(site_root / "sitemap.xml", encoding="utf-8", xml_declaration=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("site_root", type=Path)
    args = parser.parse_args()

    site_root = args.site_root.resolve()
    normalize_canonicals(site_root)
    pages_by_locale = collect_pages(site_root)
    for config in LOCALES:
        write_locale_sitemap(site_root, config, pages_by_locale)
    write_root_sitemap_index(site_root)


if __name__ == "__main__":
    main()
