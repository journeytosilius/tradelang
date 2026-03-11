import { useEffect, useRef, useState } from "react";
import Editor, { type Monaco, type OnMount } from "@monaco-editor/react";
import type * as MonacoEditor from "monaco-editor";

import { checkScript, fetchDatasets, runBacktest } from "./api";
import {
  clampWindow,
  dateInputValue,
  defaultWindowForDataset,
  DEFAULT_SOURCE,
  formatDateLabel,
  formatNumber,
  formatPercent,
  formatTimeLabel,
} from "./formatters";
import {
  configurePalmScriptLanguage,
  registerPalmScriptLanguageProviders,
} from "./palmscript-language";
import type {
  BacktestResponse,
  Diagnostic,
  PublicDataset,
} from "./types";

const CHECK_DEBOUNCE_MS = 250;
const THEME_STORAGE_KEY = "palmscript.ide.theme";

type ThemeMode = "light" | "dark";

function initialThemeMode(): ThemeMode {
  if (typeof window === "undefined") {
    return "light";
  }

  const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
  if (stored === "light" || stored === "dark") {
    return stored;
  }

  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

export function App() {
  const [script, setScript] = useState(DEFAULT_SOURCE);
  const [dataset, setDataset] = useState<PublicDataset | null>(null);
  const [fromDate, setFromDate] = useState("");
  const [toDate, setToDate] = useState("");
  const [diagnostics, setDiagnostics] = useState<Diagnostic[]>([]);
  const [backtest, setBacktest] = useState<BacktestResponse | null>(null);
  const [status, setStatus] = useState("Loading curated dataset...");
  const [checking, setChecking] = useState(true);
  const [running, setRunning] = useState(false);
  const [themeMode, setThemeMode] = useState<ThemeMode>(initialThemeMode);
  const editorRef = useRef<MonacoEditor.editor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<Monaco | null>(null);
  const checkRequestId = useRef(0);

  useEffect(() => {
    let cancelled = false;

    fetchDatasets()
      .then((catalog) => {
        if (cancelled) {
          return;
        }
        const firstDataset = catalog.datasets[0] ?? null;
        setDataset(firstDataset);
        if (firstDataset) {
          const window = defaultWindowForDataset(firstDataset);
          setFromDate(window.from);
          setToDate(window.to);
          setStatus(
            `${firstDataset.display_name} available from ${formatDateLabel(firstDataset.from)} to ${formatDateLabel(firstDataset.to - 24 * 60 * 60 * 1000)}`,
          );
        } else {
          setStatus("No curated dataset is available.");
        }
      })
      .catch((error: Error) => {
        if (!cancelled) {
          setStatus(error.message);
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const currentRequestId = ++checkRequestId.current;
    setChecking(true);

    const handle = window.setTimeout(() => {
      checkScript({ script })
        .then((response) => {
          if (checkRequestId.current !== currentRequestId) {
            return;
          }
          setDiagnostics(response.diagnostics);
          if (response.diagnostics.length === 0) {
            setStatus((current) =>
              current.startsWith("Backtest complete") ? current : "Ready",
            );
          } else {
            setStatus(`${response.diagnostics.length} diagnostic(s) need attention`);
          }
        })
        .catch((error: Error) => {
          if (checkRequestId.current === currentRequestId) {
            setStatus(error.message);
          }
        })
        .finally(() => {
          if (checkRequestId.current === currentRequestId) {
            setChecking(false);
          }
        });
    }, CHECK_DEBOUNCE_MS);

    return () => window.clearTimeout(handle);
  }, [script]);

  useEffect(() => {
    const editor = editorRef.current;
    const monaco = monacoRef.current;
    const model = editor?.getModel();
    if (!editor || !monaco || !model) {
      return;
    }

    monaco.editor.setModelMarkers(
      model,
      "palmscript-check",
      diagnostics.map((diagnostic) => ({
        startLineNumber: diagnostic.span.start.line + 1,
        startColumn: diagnostic.span.start.column + 1,
        endLineNumber: diagnostic.span.end.line + 1,
        endColumn: diagnostic.span.end.column + 1,
        message: diagnostic.message,
        severity: monaco.MarkerSeverity.Error,
      })),
    );
  }, [diagnostics]);

  useEffect(() => {
    document.documentElement.dataset.theme = themeMode;
    document.documentElement.style.colorScheme = themeMode;
    window.localStorage.setItem(THEME_STORAGE_KEY, themeMode);
  }, [themeMode]);

  const handleEditorMount: OnMount = (editor, monaco) => {
    editorRef.current = editor;
    monacoRef.current = monaco;
    registerPalmScriptLanguageProviders(monaco);
  };

  async function handleRunBacktest() {
    if (!dataset) {
      setStatus("No curated dataset is available.");
      return;
    }

    let window;
    try {
      window = clampWindow(dataset, fromDate, toDate);
    } catch (error) {
      setStatus(error instanceof Error ? error.message : "Invalid date range.");
      return;
    }

    setRunning(true);
    setStatus("Running backtest...");

    try {
      const response = await runBacktest({
        script,
        dataset_id: dataset.dataset_id,
        from_ms: window.fromMs,
        to_ms: window.toMs,
      });
      setBacktest(response);
      setStatus(
        `Backtest complete for ${formatDateLabel(response.dataset.from)} to ${formatDateLabel(response.dataset.to - 24 * 60 * 60 * 1000)}`,
      );
    } catch (error) {
      setStatus(error instanceof Error ? error.message : "Backtest failed.");
    } finally {
      setRunning(false);
    }
  }

  const statusClassName =
    diagnostics.length > 0 ? "app__status app__status--error" : "app__status";
  const editorTheme =
    themeMode === "dark" ? "palmscript-dracula" : "palmscript-docs";

  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="app-header__brand">
          <img
            className="app-header__logo"
            src="./ide/palmscript-logo.png"
            alt="PalmScript"
          />
        </div>
        <div className="app-header__controls">
          <label className="field">
            <span className="field__label">From</span>
            <input
              className="field__input"
              type="date"
              value={fromDate}
              max={toDate || undefined}
              onChange={(event) => setFromDate(event.target.value)}
            />
          </label>
          <label className="field">
            <span className="field__label">To</span>
            <input
              className="field__input"
              type="date"
              value={toDate}
              min={fromDate || undefined}
              onChange={(event) => setToDate(event.target.value)}
            />
          </label>
          <button
            aria-label={
              themeMode === "dark"
                ? "Switch to light mode"
                : "Switch to dark mode"
            }
            className="theme-toggle"
            type="button"
            onClick={() =>
              setThemeMode((current) => (current === "dark" ? "light" : "dark"))
            }
          >
            {themeMode === "dark" ? "Light Mode" : "Dark Mode"}
          </button>
          <button
            className="run-button"
            type="button"
            disabled={running || checking}
            onClick={handleRunBacktest}
          >
            {running ? "Running..." : "Run Backtest"}
          </button>
        </div>
        <div className={statusClassName}>{status}</div>
      </header>

      <main className="app-main">
        <section className="panel panel--editor">
          <div className="panel__titlebar">
            <h1 className="panel__title">Strategy</h1>
            <span className="panel__meta">Monaco Editor</span>
          </div>
          <div className="editor-frame">
            <Editor
              beforeMount={configurePalmScriptLanguage}
              defaultLanguage="palmscript"
              defaultValue={DEFAULT_SOURCE}
              onMount={handleEditorMount}
              onChange={(value) => setScript(value ?? "")}
              options={{
                automaticLayout: true,
                fontFamily:
                  "JetBrains Mono, ui-monospace, SFMono-Regular, Menlo, monospace",
                fontSize: 15,
                minimap: { enabled: false },
                padding: { top: 16, bottom: 16 },
                scrollBeyondLastLine: false,
                smoothScrolling: true,
                wordWrap: "on",
              }}
              theme={editorTheme}
              value={script}
            />
          </div>
        </section>

        <aside className="results-rail">
          <section className="panel">
            <div className="panel__titlebar">
              <h2 className="panel__title">Diagnostics</h2>
              <span className="panel__meta">
                {diagnostics.length === 0 ? "Clean" : `${diagnostics.length} issues`}
              </span>
            </div>
            <div className="list">
              {diagnostics.length === 0 ? (
                <div className="empty-state">No diagnostics.</div>
              ) : (
                diagnostics.map((diagnostic, index) => (
                  <article className="list-card list-card--diagnostic" key={index}>
                    <strong>{diagnostic.message}</strong>
                    <span>
                      line {diagnostic.span.start.line + 1}, column{" "}
                      {diagnostic.span.start.column + 1}
                    </span>
                  </article>
                ))
              )}
            </div>
          </section>

          <section className="panel">
            <div className="panel__titlebar">
              <h2 className="panel__title">Backtest Summary</h2>
              <span className="panel__meta">
                {backtest ? backtest.dataset.display_name : "No run yet"}
              </span>
            </div>
            {backtest ? (
              <div className="summary-grid">
                <MetricCard
                  label="Dataset"
                  value={`${formatDateLabel(backtest.dataset.from)} -> ${formatDateLabel(backtest.dataset.to - 24 * 60 * 60 * 1000)}`}
                />
                <MetricCard
                  label="Ending Equity"
                  value={formatNumber(backtest.result.summary.ending_equity)}
                />
                <MetricCard
                  label="Total Return"
                  tone={
                    backtest.result.summary.total_return >= 0 ? "positive" : "negative"
                  }
                  value={formatPercent(backtest.result.summary.total_return * 100)}
                />
                <MetricCard
                  label="Trades"
                  value={String(backtest.result.summary.trade_count)}
                />
                <MetricCard
                  label="Win Rate"
                  value={formatPercent(backtest.result.summary.win_rate * 100)}
                />
                <MetricCard
                  label="Max Drawdown"
                  tone="negative"
                  value={formatNumber(backtest.result.summary.max_drawdown)}
                />
              </div>
            ) : (
              <div className="empty-state">No run yet.</div>
            )}
          </section>

          <section className="panel">
            <div className="panel__titlebar">
              <h2 className="panel__title">Equity Curve</h2>
            </div>
            {backtest && backtest.result.equity_curve.length > 1 ? (
              <EquityChart points={backtest.result.equity_curve.map((point) => point.equity)} />
            ) : (
              <div className="empty-state">No curve yet.</div>
            )}
          </section>

          <section className="panel">
            <div className="panel__titlebar">
              <h2 className="panel__title">Trades</h2>
              <span className="panel__meta">
                {backtest ? backtest.result.trades.length : 0}
              </span>
            </div>
            <div className="list">
              {backtest && backtest.result.trades.length > 0 ? (
                backtest.result.trades.slice(0, 50).map((trade, index) => (
                  <article className="list-card" key={index}>
                    <strong>
                      {trade.side} · {formatTimeLabel(trade.entry.time)}
                    </strong>
                    <span>
                      entry {formatNumber(trade.entry.price)} / exit{" "}
                      {formatNumber(trade.exit.price)} / pnl{" "}
                      {formatNumber(trade.realized_pnl)}
                    </span>
                  </article>
                ))
              ) : (
                <div className="empty-state">No trades.</div>
              )}
            </div>
          </section>

          <section className="panel">
            <div className="panel__titlebar">
              <h2 className="panel__title">Orders</h2>
              <span className="panel__meta">
                {backtest ? backtest.result.orders.length : 0}
              </span>
            </div>
            <div className="list">
              {backtest && backtest.result.orders.length > 0 ? (
                backtest.result.orders.slice(0, 50).map((order, index) => (
                  <article className="list-card" key={index}>
                    <strong>
                      {order.role} · {order.kind} · {order.status}
                    </strong>
                    <span>
                      placed {formatTimeLabel(order.placed_time)} / fill{" "}
                      {order.fill_price === null
                        ? "NA"
                        : formatNumber(order.fill_price)}
                    </span>
                  </article>
                ))
              ) : (
                <div className="empty-state">No orders.</div>
              )}
            </div>
          </section>
        </aside>
      </main>
    </div>
  );
}

function MetricCard({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: string;
  tone?: "neutral" | "positive" | "negative";
}) {
  return (
    <article className={`metric-card metric-card--${tone}`}>
      <span className="metric-card__label">{label}</span>
      <strong className="metric-card__value">{value}</strong>
    </article>
  );
}

function EquityChart({ points }: { points: number[] }) {
  const width = 560;
  const height = 180;
  const min = Math.min(...points);
  const max = Math.max(...points);
  const span = max - min || 1;
  const path = points
    .map((point, index) => {
      const x = (index / Math.max(points.length - 1, 1)) * width;
      const y = height - ((point - min) / span) * (height - 12) - 6;
      return `${index === 0 ? "M" : "L"} ${x.toFixed(2)} ${y.toFixed(2)}`;
    })
    .join(" ");

  return (
    <svg className="equity-chart" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none">
      <defs>
        <linearGradient id="equity-fill" x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor="rgba(31,141,225,0.24)" />
          <stop offset="100%" stopColor="rgba(31,141,225,0.04)" />
        </linearGradient>
      </defs>
      <rect className="equity-chart__bg" height={height} rx="18" width={width} x="0" y="0" />
      <path className="equity-chart__area" d={`${path} L ${width} ${height} L 0 ${height} Z`} />
      <path className="equity-chart__line" d={path} />
    </svg>
  );
}
