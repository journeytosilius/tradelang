function wirePalmScriptHeaderHomeLink() {
  const topic = document.querySelector(".md-header__title .md-header__topic");
  if (!topic || topic.querySelector(".ps-home-link")) {
    return;
  }

  const link = document.createElement("a");
  link.href = "https://palmscript.dev/";
  link.className = "ps-home-link";
  link.setAttribute("aria-label", "PalmScript home");
  link.setAttribute("target", "_blank");
  link.setAttribute("rel", "noopener noreferrer");
  link.textContent = "PalmScript";

  topic.replaceChildren(link);
}

function wirePalmScriptDocsLinks() {
  const links = document.querySelectorAll("a[href]");
  for (const link of links) {
    const href = link.getAttribute("href");
    if (!href || href.startsWith("#")) {
      continue;
    }

    link.setAttribute("target", "_blank");
    link.setAttribute("rel", "noopener noreferrer");
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
