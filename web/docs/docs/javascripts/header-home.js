function getPalmScriptLocaleHomeHref() {
  const { origin, pathname } = window.location;
  const localeMatch = pathname.match(/^\/([^/]+)\/docs(?:\/|$)/);
  if (localeMatch) {
    return `${origin}/${localeMatch[1]}/`;
  }

  return `${origin}/`;
}

function buildPalmScriptHomeLink(label) {
  const link = document.createElement("a");
  link.href = getPalmScriptLocaleHomeHref();
  link.className = "ps-home-link";
  link.setAttribute("aria-label", "PalmScript home");
  link.textContent = label;
  return link;
}

function wirePalmScriptHeaderHomeLink() {
  const topic = document.querySelector(".md-header__title .md-header__topic");
  if (!topic || topic.querySelector(".ps-home-link")) {
    return;
  }

  topic.replaceChildren(buildPalmScriptHomeLink("PalmScript"));
}

function wirePalmScriptHeaderTopicHomeLink() {
  const topic = document.querySelector(
    '.md-header__title [data-md-component="header-topic"] .md-ellipsis',
  );
  if (!topic || topic.querySelector(".ps-home-link")) {
    return;
  }

  const label = topic.textContent?.trim();
  if (!label) {
    return;
  }

  topic.replaceChildren(buildPalmScriptHomeLink(label));
}

function wirePalmScriptHeaderLogoLink() {
  const logo = document.querySelector("a.md-header__button.md-logo");
  if (!logo) {
    return;
  }

  logo.href = getPalmScriptLocaleHomeHref();
  logo.removeAttribute("target");
  logo.removeAttribute("rel");
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
  wirePalmScriptHeaderLogoLink();
  wirePalmScriptHeaderHomeLink();
  wirePalmScriptHeaderTopicHomeLink();
  wirePalmScriptDocsLinks();
}

if (typeof document$ !== "undefined") {
  document$.subscribe(wirePalmScriptDocsUi);
} else {
  document.addEventListener("DOMContentLoaded", wirePalmScriptDocsUi);
}
