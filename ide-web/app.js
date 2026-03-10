const SESSION_KEY = "palmscript-ide-session";
const MODEL_URI = "inmemory:///strategy.palm";
const DAY_MS = 24 * 60 * 60 * 1000;
const DEFAULT_SOURCE = `interval 4h
source spot = binance.spot("BTCUSDT")
use spot 1d
use spot 1w

let fast = ema(spot.close, 13)
let slow = ema(spot.close, 89)
let daily_fast = ema(spot.1d.close, 30)
let daily_slow = ema(spot.1d.close, 80)
let weekly_fast = ema(spot.1w.close, 5)
let weekly_slow = ema(spot.1w.close, 13)

entry long = above(fast, slow) and above(daily_fast, daily_slow) and above(weekly_fast, weekly_slow)
exit long = below(fast, slow)

plot(fast - slow)
export trend_long_state = above(fast, slow)
`;

const state = {
  datasets: [],
  dataset: null,
  editor: null,
  model: null,
  monaco: null,
  lsp: null,
  diagnostics: [],
  lastBacktest: null,
};

function toLspPosition(position) {
  return {
    line: position.lineNumber - 1,
    character: position.column - 1,
  };
}

function sessionId() {
  let value = window.localStorage.getItem(SESSION_KEY);
  if (!value) {
    value = globalThis.crypto?.randomUUID?.() ?? `session-${Date.now()}`;
    window.localStorage.setItem(SESSION_KEY, value);
  }
  return value;
}

function setStatus(text) {
  document.getElementById("status-text").textContent = text;
}

function formatDateInput(ms) {
  return new Date(ms).toISOString().slice(0, 10);
}

function parseDateInput(value) {
  return Date.parse(`${value}T00:00:00Z`);
}

function selectedWindow() {
  const fromValue = document.getElementById("from-date").value;
  const toValue = document.getElementById("to-date").value;
  if (!fromValue || !toValue) {
    throw new Error("select a valid backtest date range");
  }
  return {
    fromMs: parseDateInput(fromValue),
    toMs: parseDateInput(toValue) + DAY_MS,
  };
}

function syncDateInputs(changedInputId) {
  const fromInput = document.getElementById("from-date");
  const toInput = document.getElementById("to-date");
  if (!fromInput.value || !toInput.value) {
    return;
  }
  if (fromInput.value <= toInput.value) {
    return;
  }
  if (changedInputId === "from-date") {
    toInput.value = fromInput.value;
  } else {
    fromInput.value = toInput.value;
  }
}

async function fetchJson(path, options = {}) {
  const response = await fetch(new URL(path, window.location.href), {
    ...options,
    headers: {
      "content-type": "application/json",
      "x-palmscript-session": sessionId(),
      ...(options.headers ?? {}),
    },
  });
  const payload = await response.json();
  if (!response.ok) {
    throw new Error(payload.error ?? `request failed: ${response.status}`);
  }
  return payload;
}

function loadMonaco() {
  return new Promise((resolve) => {
    window.require.config({
      paths: {
        vs: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs",
      },
    });
    window.require(["vs/editor/editor.main"], () => resolve(window.monaco));
  });
}

class LspClient {
  constructor(url) {
    this.url = url;
    this.socket = null;
    this.nextId = 1;
    this.pending = new Map();
    this.notificationHandlers = new Map();
  }

  async connect() {
    await new Promise((resolve, reject) => {
      this.socket = new WebSocket(this.url);
      this.socket.onopen = () => resolve();
      this.socket.onerror = (event) => reject(new Error(`LSP socket failed: ${event.type}`));
      this.socket.onmessage = (event) => this.handleMessage(event.data);
      this.socket.onclose = () => {
        for (const [, pending] of this.pending) {
          pending.reject(new Error("LSP socket closed"));
        }
        this.pending.clear();
      };
    });
    await this.request("initialize", {
      processId: null,
      rootUri: null,
      capabilities: {},
      clientInfo: { name: "palmscript-browser-ide", version: "0.1.0" },
    });
    this.notify("initialized", {});
  }

  handleMessage(raw) {
    const message = JSON.parse(raw);
    if (message.id !== undefined && (message.result !== undefined || message.error !== undefined)) {
      const pending = this.pending.get(message.id);
      if (!pending) {
        return;
      }
      this.pending.delete(message.id);
      if (message.error) {
        pending.reject(new Error(message.error.message || "LSP request failed"));
      } else {
        pending.resolve(message.result);
      }
      return;
    }
    if (!message.method) {
      return;
    }
    const handlers = this.notificationHandlers.get(message.method) ?? [];
    for (const handler of handlers) {
      handler(message.params);
    }
  }

  onNotification(method, handler) {
    const handlers = this.notificationHandlers.get(method) ?? [];
    handlers.push(handler);
    this.notificationHandlers.set(method, handlers);
  }

  request(method, params) {
    const id = this.nextId++;
    const payload = { jsonrpc: "2.0", id, method, params };
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify(payload));
    });
  }

  notify(method, params) {
    this.socket.send(JSON.stringify({ jsonrpc: "2.0", method, params }));
  }
}

function renderDiagnostics(diagnostics) {
  const container = document.getElementById("diagnostics-list");
  container.innerHTML = "";
  if (diagnostics.length === 0) {
    container.innerHTML = '<div class="muted">No diagnostics.</div>';
    return;
  }
  for (const diagnostic of diagnostics) {
    const item = document.createElement("div");
    item.className = "diagnostic-item";
    item.innerHTML = `<strong>${diagnostic.message}</strong><div class="muted">line ${diagnostic.range.start.line + 1}, column ${diagnostic.range.start.character + 1}</div>`;
    container.appendChild(item);
  }
}

function setMarkers(diagnostics) {
  const markers = diagnostics.map((diagnostic) => ({
    startLineNumber: diagnostic.range.start.line + 1,
    startColumn: diagnostic.range.start.character + 1,
    endLineNumber: diagnostic.range.end.line + 1,
    endColumn: diagnostic.range.end.character + 1,
    severity: state.monaco.MarkerSeverity.Error,
    message: diagnostic.message,
  }));
  state.monaco.editor.setModelMarkers(state.model, "palmscript", markers);
  renderDiagnostics(diagnostics);
}

function buildLspUrl() {
  const url = new URL("api/lsp", window.location.href);
  url.protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  url.searchParams.set("session", sessionId());
  return url.toString();
}

function bindMonacoProviders() {
  const monaco = state.monaco;
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
      { open: "\"", close: "\"" },
    ],
    surroundingPairs: [
      { open: "{", close: "}" },
      { open: "[", close: "]" },
      { open: "(", close: ")" },
      { open: "\"", close: "\"" },
    ],
  });
  monaco.languages.setMonarchTokensProvider("palmscript", {
    keywords: [
      "interval",
      "source",
      "use",
      "fn",
      "let",
      "const",
      "input",
      "order",
      "export",
      "trigger",
      "entry",
      "exit",
      "protect",
      "target",
      "size",
      "long",
      "short",
      "if",
      "else",
      "and",
      "or",
      "true",
      "false",
      "na",
    ],
    tokenizer: {
      root: [
        [/\/\/.*$/, "comment"],
        [/\b\d+(?:\.\d+)?\b/, "number"],
        [/\b\d+(?:s|m|h|d|w|M)\b/, "type"],
        [/"/, { token: "string.quote", bracket: "@open", next: "@string" }],
        [
          /[A-Za-z_][A-Za-z0-9_]*/,
          {
            cases: {
              "@keywords": "keyword",
              "@default": "identifier",
            },
          },
        ],
        [/[{}[\]()]/, "@brackets"],
        [/[=><!~?:&|+\-*/^.]+/, "operator"],
        [/[.,]/, "delimiter"],
      ],
      string: [
        [/[^\\"]+/, "string"],
        [/\\./, "string.escape"],
        [/"/, { token: "string.quote", bracket: "@close", next: "@pop" }],
      ],
    },
  });
  monaco.editor.defineTheme("palmscript-light", {
    base: "vs",
    inherit: true,
    rules: [
      { token: "keyword", foreground: "156fbe", fontStyle: "bold" },
      { token: "string", foreground: "177bc8" },
      { token: "number", foreground: "2e9cf3" },
      { token: "comment", foreground: "647b92", fontStyle: "italic" },
      { token: "operator", foreground: "183247" },
      { token: "delimiter", foreground: "647b92" },
      { token: "type", foreground: "0f5f9f" },
      { token: "function", foreground: "156fbe" },
      { token: "variable", foreground: "183247" },
      { token: "parameter", foreground: "177bc8" },
      { token: "namespace", foreground: "0f5f9f" },
    ],
    colors: {
      "editor.background": "#fcfdff",
      "editor.foreground": "#183247",
      "editor.lineHighlightBackground": "#edf4fc",
      "editor.selectionBackground": "#d6e9fa",
      "editor.inactiveSelectionBackground": "#e8f3fd",
      "editorCursor.foreground": "#156fbe",
      "editorLineNumber.foreground": "#8ca4bb",
      "editorLineNumber.activeForeground": "#156fbe",
      "editorIndentGuide.background1": "#d8e4f1",
      "editorIndentGuide.activeBackground1": "#9dc5ea",
      "editorWidget.background": "#ffffff",
      "editorWidget.border": "#c9daeb",
      "editorSuggestWidget.background": "#ffffff",
      "editorSuggestWidget.border": "#c9daeb",
      "editorHoverWidget.background": "#ffffff",
      "editorHoverWidget.border": "#c9daeb",
    },
  });
  monaco.languages.registerCompletionItemProvider("palmscript", {
    triggerCharacters: [".", "("],
    provideCompletionItems: async (model, position) => {
      const result = await state.lsp.request("textDocument/completion", {
        textDocument: { uri: MODEL_URI },
        position: toLspPosition(position),
      });
      const items = Array.isArray(result) ? result : result?.items ?? [];
      return {
        suggestions: items.map((item) => ({
          label: item.label,
          kind: monaco.languages.CompletionItemKind.Text,
          insertText: item.insertText ?? item.label,
          detail: item.detail,
          documentation: item.documentation?.value ?? item.documentation,
          range: undefined,
        })),
      };
    },
  });
  monaco.languages.registerHoverProvider("palmscript", {
    provideHover: async (_model, position) => {
      const result = await state.lsp.request("textDocument/hover", {
        textDocument: { uri: MODEL_URI },
        position: toLspPosition(position),
      });
      if (!result?.contents) {
        return null;
      }
      const value = Array.isArray(result.contents)
        ? result.contents.map((entry) => entry.value ?? entry).join("\n\n")
        : result.contents.value ?? result.contents;
      return {
        range: result.range
          ? new monaco.Range(
              result.range.start.line + 1,
              result.range.start.character + 1,
              result.range.end.line + 1,
              result.range.end.character + 1,
            )
          : undefined,
        contents: [{ value }],
      };
    },
  });
  monaco.languages.registerDefinitionProvider("palmscript", {
    provideDefinition: async (_model, position) => {
      const result = await state.lsp.request("textDocument/definition", {
        textDocument: { uri: MODEL_URI },
        position: toLspPosition(position),
      });
      if (!result) {
        return null;
      }
      return {
        uri: state.model.uri,
        range: new monaco.Range(
          result.range.start.line + 1,
          result.range.start.character + 1,
          result.range.end.line + 1,
          result.range.end.character + 1,
        ),
      };
    },
  });
  monaco.languages.registerDocumentFormattingEditProvider("palmscript", {
    provideDocumentFormattingEdits: async () => {
      const result = await state.lsp.request("textDocument/formatting", {
        textDocument: { uri: MODEL_URI },
        options: { tabSize: 4, insertSpaces: true },
      });
      return (result ?? []).map((edit) => ({
        range: new monaco.Range(
          edit.range.start.line + 1,
          edit.range.start.character + 1,
          edit.range.end.line + 1,
          edit.range.end.character + 1,
        ),
        text: edit.newText,
      }));
    },
  });
  monaco.languages.registerDocumentSemanticTokensProvider("palmscript", {
    getLegend: () => ({
      tokenTypes: [
        "keyword",
        "string",
        "number",
        "function",
        "variable",
        "parameter",
        "namespace",
        "type",
      ],
      tokenModifiers: [],
    }),
    provideDocumentSemanticTokens: async () => {
      const result = await state.lsp.request("textDocument/semanticTokens/full", {
        textDocument: { uri: MODEL_URI },
      });
      return {
        data: new Uint32Array(result?.data ?? []),
      };
    },
    releaseDocumentSemanticTokens: () => {},
  });
}

function debounce(fn, delayMs) {
  let timeoutId = null;
  return (...args) => {
    if (timeoutId !== null) {
      clearTimeout(timeoutId);
    }
    timeoutId = setTimeout(() => fn(...args), delayMs);
  };
}

function updateTabs() {
  const buttons = document.querySelectorAll(".tab-button");
  const panels = document.querySelectorAll(".tab-panel");
  for (const button of buttons) {
    button.addEventListener("click", () => {
      for (const other of buttons) {
        other.classList.toggle("active", other === button);
      }
      for (const panel of panels) {
        panel.classList.toggle("active", panel.id === `tab-${button.dataset.tab}`);
      }
    });
  }
}

function renderEquityChart(result) {
  const svg = document.getElementById("equity-chart");
  svg.innerHTML = "";
  const points = result.equity_curve ?? [];
  if (points.length === 0) {
    return;
  }
  const min = Math.min(...points.map((point) => point.equity));
  const max = Math.max(...points.map((point) => point.equity));
  const span = Math.max(max - min, 1);
  const polyline = points
    .map((point, index) => {
      const x = (index / Math.max(points.length - 1, 1)) * 480;
      const y = 165 - ((point.equity - min) / span) * 140;
      return `${x},${y}`;
    })
    .join(" ");
  const area = document.createElementNS("http://www.w3.org/2000/svg", "polyline");
  area.setAttribute("fill", "none");
  area.setAttribute("stroke", "#156fbe");
  area.setAttribute("stroke-width", "3");
  area.setAttribute("points", polyline);
  svg.appendChild(area);
}

function renderList(containerId, items, formatter) {
  const container = document.getElementById(containerId);
  container.innerHTML = "";
  if (!items || items.length === 0) {
    container.innerHTML = '<div class="muted">No data.</div>';
    return;
  }
  for (const item of items.slice(0, 50)) {
    const node = document.createElement("div");
    node.className = containerId === "tab-trades" ? "trade-item" : "order-item";
    node.innerHTML = formatter(item);
    container.appendChild(node);
  }
}

function formatSummaryNumber(value, digits = 2) {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return "NA";
  }
  return value.toFixed(digits);
}

function formatSummaryPercent(value) {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return "NA";
  }
  return `${value.toFixed(2)}%`;
}

function summaryValueClass(value) {
  if (typeof value !== "number" || Number.isNaN(value) || value === 0) {
    return "summary-value";
  }
  return value > 0 ? "summary-value positive" : "summary-value negative";
}

function renderSummaryCard(label, value, extraClass = "") {
  const className = extraClass ? `summary-card ${extraClass}` : "summary-card";
  return `<div class="${className}"><span class="summary-label">${label}</span><div class="summary-value">${value}</div></div>`;
}

async function loadCatalogs() {
  const datasets = await fetchJson("api/datasets");
  state.datasets = datasets.datasets;
  state.dataset = state.datasets[0] ?? null;
  if (!state.dataset) {
    throw new Error("no public IDE dataset is available");
  }

  const fromInput = document.getElementById("from-date");
  const toInput = document.getElementById("to-date");
  const firstAvailableDay = formatDateInput(state.dataset.from);
  const lastAvailableDay = formatDateInput(state.dataset.to - DAY_MS);
  const defaultFromMs = Math.max(state.dataset.from, state.dataset.to - 365 * DAY_MS);

  fromInput.min = firstAvailableDay;
  fromInput.max = lastAvailableDay;
  fromInput.value = formatDateInput(defaultFromMs);

  toInput.min = firstAvailableDay;
  toInput.max = lastAvailableDay;
  toInput.value = lastAvailableDay;

  fromInput.addEventListener("change", () => syncDateInputs("from-date"));
  toInput.addEventListener("change", () => syncDateInputs("to-date"));

  setStatus(
    `${state.dataset.display_name} available from ${firstAvailableDay} to ${lastAvailableDay}`,
  );
}

function renderSelectedRange(response) {
  const from = formatDateInput(response.dataset.from);
  const to = formatDateInput(response.dataset.to - DAY_MS);
  return `${from} -> ${to}`;
}

function renderSummaryDatasetLabel(response) {
  if (!response.dataset?.display_name) {
    return "unknown";
  }
  return `${response.dataset.display_name} (${renderSelectedRange(response)})`;
}

async function runBacktest() {
  const window = selectedWindow();
  if (!state.dataset) {
    throw new Error("no public IDE dataset is available");
  }
  setStatus("Running backtest…");
  try {
    const response = await fetchJson("api/backtest", {
      method: "POST",
      body: JSON.stringify({
        script: state.editor.getValue(),
        dataset_id: state.dataset.dataset_id,
        from_ms: window.fromMs,
        to_ms: window.toMs,
      }),
    });
    renderBacktest(response);
    setStatus(`Backtest complete for ${renderSelectedRange(response)}`);
  } catch (error) {
    setStatus(error.message);
  }
}

function renderBacktest(response) {
  state.lastBacktest = response;
  const summary = response.result.summary;
  const totalReturnPct = summary.total_return * 100;
  const summaryOutput = document.getElementById("summary-output");
  summaryOutput.classList.remove("summary-empty");
  summaryOutput.innerHTML = [
    renderSummaryCard("Dataset", renderSummaryDatasetLabel(response), "wide"),
    renderSummaryCard("Ending Equity", formatSummaryNumber(summary.ending_equity)),
    renderSummaryCard("Trades", `${summary.trade_count}`),
    `<div class="summary-card"><span class="summary-label">Total Return</span><div class="${summaryValueClass(totalReturnPct)}">${formatSummaryPercent(totalReturnPct)}</div></div>`,
    renderSummaryCard("Win Rate", formatSummaryPercent(summary.win_rate * 100)),
    renderSummaryCard("Max Drawdown", formatSummaryNumber(summary.max_drawdown), "wide"),
  ].join("");
  renderEquityChart(response.result);
  renderList("tab-trades", response.result.trades, (trade) => {
    return `<strong>${trade.side}</strong> ${new Date(trade.entry.time).toISOString().slice(0, 10)} -> ${new Date(trade.exit.time).toISOString().slice(0, 10)}<div class="muted">entry ${trade.entry.price.toFixed(2)} / exit ${trade.exit.price.toFixed(2)} / pnl ${trade.realized_pnl.toFixed(2)}</div>`;
  });
  renderList("tab-orders", response.result.orders, (order) => {
    return `<strong>${order.role}</strong> ${order.kind} / ${order.status}<div class="muted">placed ${order.placed_time ?? "na"} fill ${order.fill_price ?? "na"}</div>`;
  });
}

async function setupEditor() {
  state.monaco = await loadMonaco();
  bindMonacoProviders();
  state.model = state.monaco.editor.createModel(DEFAULT_SOURCE, "palmscript", state.monaco.Uri.parse(MODEL_URI));
  state.editor = state.monaco.editor.create(document.getElementById("editor"), {
    model: state.model,
    theme: "palmscript-light",
    automaticLayout: true,
    fontSize: 14,
    minimap: { enabled: false },
    fontFamily: "JetBrains Mono, monospace",
  });

  state.lsp = new LspClient(buildLspUrl());
  state.lsp.onNotification("textDocument/publishDiagnostics", (params) => {
    state.diagnostics = params.diagnostics ?? [];
    setMarkers(state.diagnostics);
  });
  await state.lsp.connect();
  state.lsp.notify("textDocument/didOpen", {
    textDocument: {
      uri: MODEL_URI,
      languageId: "palmscript",
      version: 1,
      text: state.model.getValue(),
    },
  });

  let version = 1;
  const sendChange = debounce(() => {
    version += 1;
    state.lsp.notify("textDocument/didChange", {
      textDocument: { uri: MODEL_URI, version },
      contentChanges: [{ text: state.model.getValue() }],
    });
  }, 150);
  state.model.onDidChangeContent(sendChange);
}

function bindActions() {
  document.getElementById("run-button").addEventListener("click", runBacktest);
}

async function init() {
  updateTabs();
  bindActions();
  await loadCatalogs();
  await setupEditor();
  setStatus("Ready");
}

init().catch((error) => {
  console.error(error);
  setStatus(error.message);
});
