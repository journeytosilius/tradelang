function getPalmScriptLocaleHomeHref() {
  const { origin, pathname } = window.location;
  const localeMatch = pathname.match(/^\/([^/]+)\/docs(?:\/|$)/);
  if (localeMatch) {
    return `${origin}/${localeMatch[1]}/`;
  }

  return `${origin}/`;
}

function wirePalmScriptHeaderHomeLink() {
  const topic = document.querySelector(".md-header__title .md-header__topic");
  if (!topic || topic.querySelector(".ps-home-link")) {
    return;
  }

  const link = document.createElement("a");
  link.href = getPalmScriptLocaleHomeHref();
  link.className = "ps-home-link";
  link.setAttribute("aria-label", "PalmScript home");
  link.textContent = "PalmScript";

  topic.replaceChildren(link);
}

function isExternalHref(href) {
  if (!href || href.startsWith("#")) {
    return false;
  }

  if (href.startsWith("mailto:") || href.startsWith("tel:")) {
    return true;
  }

  try {
    const url = new URL(href, window.location.href);
    return url.origin !== window.location.origin;
  } catch {
    return false;
  }
}

function wirePalmScriptDocsLinks() {
  const links = document.querySelectorAll("a[href]");
  for (const link of links) {
    const href = link.getAttribute("href");
    if (!href || href.startsWith("#")) {
      continue;
    }

    if (isExternalHref(href)) {
      link.setAttribute("target", "_blank");
      link.setAttribute("rel", "noopener noreferrer");
      continue;
    }

    link.removeAttribute("target");
    link.removeAttribute("rel");
  }
}

function wirePalmScriptDocsUi() {
  wirePalmScriptHeaderHomeLink();
  wirePalmScriptDocsLinks();
}

if (typeof document$ !== "undefined") {
  document$.subscribe(wirePalmScriptDocsUi);
} else {
  document.addEventListener("DOMContentLoaded", wirePalmScriptDocsUi);
}
