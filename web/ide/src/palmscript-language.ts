import type { Monaco } from "@monaco-editor/react";
import type * as MonacoEditor from "monaco-editor";

import { fetchCompletions, fetchHover } from "./api";
import type { CompletionEntry } from "./types";

let configured = false;
let providersRegistered = false;

const KEYWORDS = [
  "and",
  "const",
  "else",
  "entry",
  "exit",
  "export",
  "false",
  "fn",
  "if",
  "input",
  "interval",
  "let",
  "na",
  "order",
  "plot",
  "protect",
  "source",
  "target",
  "trigger",
  "true",
  "use",
];

const BUILTINS = [
  "above",
  "atr",
  "below",
  "coalesce",
  "crossover",
  "ema",
  "highest",
  "highest_since",
  "kama",
  "lowest",
  "macd",
  "plot",
  "risk_pct",
  "rsi",
  "sma",
];

export function configurePalmScriptLanguage(monaco: Monaco): void {
  if (configured) {
    return;
  }
  configured = true;

  monaco.languages.register({ id: "palmscript" });
  monaco.languages.setLanguageConfiguration("palmscript", {
    comments: {
      lineComment: "//",
    },
    brackets: [
      ["{", "}"],
      ["[", "]"],
      ["(", ")"],
    ],
    autoClosingPairs: [
      { open: "{", close: "}" },
      { open: "[", close: "]" },
      { open: "(", close: ")" },
      { open: '"', close: '"' },
    ],
  });

  monaco.languages.setMonarchTokensProvider("palmscript", {
    keywords: KEYWORDS,
    builtins: BUILTINS,
    tokenizer: {
      root: [
        [/\/\/.*$/, "comment"],
        [/"[^"]*"/, "string"],
        [/\b\d+(\.\d+)?\b/, "number"],
        [/[{}()[\]]/, "@brackets"],
        [
          /[a-zA-Z_][\w.]*/,
          {
            cases: {
              "@keywords": "keyword",
              "@builtins": "predefined",
              "@default": "identifier",
            },
          },
        ],
        [/[=><!~?:&|+\-*/^%]+/, "operator"],
      ],
    },
  });

  monaco.editor.defineTheme("palmscript-docs", {
    base: "vs",
    inherit: true,
    rules: [
      { token: "keyword", foreground: "0f5d92", fontStyle: "bold" },
      { token: "predefined", foreground: "1b87d6" },
      { token: "identifier", foreground: "1f3142" },
      { token: "number", foreground: "9c4f14" },
      { token: "string", foreground: "1a7f5a" },
      { token: "comment", foreground: "718598" },
      { token: "operator", foreground: "425466" },
    ],
    colors: {
      "editor.background": "#f6f9fc",
      "editor.foreground": "#173246",
      "editor.lineHighlightBackground": "#eef5fb",
      "editorLineNumber.foreground": "#98aabd",
      "editorCursor.foreground": "#1f8de1",
      "editor.selectionBackground": "#cfe4f7",
      "editor.inactiveSelectionBackground": "#dbeaf6",
    },
  });

  monaco.editor.defineTheme("palmscript-dracula", {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "keyword", foreground: "ff79c6", fontStyle: "bold" },
      { token: "predefined", foreground: "8be9fd" },
      { token: "identifier", foreground: "f8f8f2" },
      { token: "number", foreground: "bd93f9" },
      { token: "string", foreground: "f1fa8c" },
      { token: "comment", foreground: "6272a4" },
      { token: "operator", foreground: "ff79c6" },
    ],
    colors: {
      "editor.background": "#282a36",
      "editor.foreground": "#f8f8f2",
      "editor.lineHighlightBackground": "#313445",
      "editorLineNumber.foreground": "#6272a4",
      "editorCursor.foreground": "#ff79c6",
      "editor.selectionBackground": "#44475a",
      "editor.inactiveSelectionBackground": "#3a3c4d",
    },
  });
}

function completionItemKind(
  monaco: Monaco,
  entry: CompletionEntry,
): MonacoEditor.languages.CompletionItemKind {
  switch (entry.kind) {
    case "keyword":
    case "interval":
    case "field":
      return monaco.languages.CompletionItemKind.Keyword;
    case "builtin":
    case "function":
      return monaco.languages.CompletionItemKind.Function;
    case "source":
      return monaco.languages.CompletionItemKind.Module;
    default:
      return monaco.languages.CompletionItemKind.Variable;
  }
}

export function registerPalmScriptLanguageProviders(monaco: Monaco): void {
  if (providersRegistered) {
    return;
  }
  providersRegistered = true;

  monaco.languages.registerHoverProvider("palmscript", {
    async provideHover(model, position) {
      const offset = model.getOffsetAt(position);
      const response = await fetchHover({
        script: model.getValue(),
        offset,
      });
      if (!response.hover) {
        return null;
      }

      const { hover } = response;
      return {
        contents: [{ value: hover.contents }],
        range: new monaco.Range(
          hover.span.start.line + 1,
          hover.span.start.column + 1,
          hover.span.end.line + 1,
          hover.span.end.column + 1,
        ),
      };
    },
  });

  monaco.languages.registerCompletionItemProvider("palmscript", {
    triggerCharacters: [".", "("],
    async provideCompletionItems(model, position) {
      const offset = model.getOffsetAt(position);
      const response = await fetchCompletions({
        script: model.getValue(),
        offset,
      });
      const word = model.getWordUntilPosition(position);
      const range = new monaco.Range(
        position.lineNumber,
        word.startColumn,
        position.lineNumber,
        word.endColumn,
      );
      return {
        suggestions: response.items.map((entry) => ({
          label: entry.label,
          kind: completionItemKind(monaco, entry),
          detail: entry.detail ?? undefined,
          documentation: entry.documentation
            ? {
                value: entry.documentation,
              }
            : undefined,
          insertText: entry.label,
          range,
        })),
      };
    },
  });
}
