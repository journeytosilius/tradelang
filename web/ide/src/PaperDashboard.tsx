import { useEffect, useState } from "react";

import {
  fetchPaperOverview,
  fetchPaperSessionDetail,
  fetchPaperSessionLogs,
} from "./api";
import {
  formatDateLabel,
  formatNumber,
  formatPercent,
  formatTimeLabel,
} from "./formatters";
import type {
  ArbitragePairDiagnosticSummary,
  BacktestDiagnostics,
  BacktestResult,
  BacktestSummary,
  ExecutionDaemonStatus,
  PaperDashboardOverview,
  PaperDashboardSession,
  PaperFeedSnapshot,
  PaperSessionDetailResponse,
  PaperSessionExport,
  PaperSessionLogEvent,
  PaperSessionLogsResponse,
  PaperSessionSnapshot,
  PositionSnapshot,
  TransferRouteDiagnosticSummary,
} from "./types";
import { LineChart, MetricCard } from "./ui";

const POLL_MS = 3_000;

export function PaperDashboard() {
  const [overview, setOverview] = useState<PaperDashboardOverview | null>(null);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [detail, setDetail] = useState<PaperSessionDetailResponse | null>(null);
  const [logs, setLogs] = useState<PaperSessionLogsResponse | null>(null);
  const [status, setStatus] = useState("Loading paper sessions...");
  const [overviewLoading, setOverviewLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);

  useEffect(() => {
    document.title = "PalmScript Paper Dashboard";
  }, []);

  useEffect(() => {
    let cancelled = false;

    async function loadOverview() {
      try {
        const nextOverview = await fetchPaperOverview();
        if (cancelled) {
          return;
        }
        setOverview(nextOverview);
        setOverviewLoading(false);
        setStatus(buildOverviewStatus(nextOverview.daemon, nextOverview.sessions.length));
        setSelectedSessionId((current) => {
          if (current && nextOverview.sessions.some((session) => session.manifest.session_id === current)) {
            return current;
          }
          return nextOverview.sessions[0]?.manifest.session_id ?? null;
        });
      } catch (error) {
        if (!cancelled) {
          setOverviewLoading(false);
          setStatus(error instanceof Error ? error.message : "Failed to load paper sessions.");
        }
      }
    }

    void loadOverview();
    const handle = window.setInterval(() => {
      void loadOverview();
    }, POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(handle);
    };
  }, []);

  useEffect(() => {
    if (!selectedSessionId) {
      setDetail(null);
      setLogs(null);
      return;
    }
    const sessionId = selectedSessionId;

    let cancelled = false;

    async function loadDetail() {
      setDetailLoading(true);
      try {
        const [nextDetail, nextLogs] = await Promise.all([
          fetchPaperSessionDetail(sessionId),
          fetchPaperSessionLogs(sessionId),
        ]);
        if (cancelled) {
          return;
        }
        setDetail(nextDetail);
        setLogs(nextLogs);
      } catch (error) {
        if (!cancelled) {
          setStatus(error instanceof Error ? error.message : "Failed to load session detail.");
        }
      } finally {
        if (!cancelled) {
          setDetailLoading(false);
        }
      }
    }

    void loadDetail();
    const handle = window.setInterval(() => {
      void loadDetail();
    }, POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(handle);
    };
  }, [selectedSessionId]);

  const selectedSession =
    overview?.sessions.find((session) => session.manifest.session_id === selectedSessionId) ?? null;
  const exportData = detail?.export ?? null;
  const snapshot = exportData?.snapshot ?? selectedSession?.snapshot ?? null;
  const result = exportData?.latest_result ?? null;
  const summary = snapshot?.summary ?? result?.summary ?? null;
  const diagnostics = result?.diagnostics ?? null;

  return (
    <div className="app-shell app-shell--paper">
      <header className="app-header app-header--paper">
        <div className="app-header__brand app-header__brand--paper">
          <img
            className="app-header__logo"
            src="./ide/palmscript-logo.png"
            alt="PalmScript"
          />
          <div className="paper-heading">
            <span className="paper-heading__eyebrow">Paper Trading Monitor</span>
            <h1 className="paper-heading__title">Live strategy dashboard</h1>
          </div>
        </div>
        <div className="paper-header__stats">
          <MetricCard
            label="Daemon"
            value={daemonHeadline(overview?.daemon ?? null)}
            detail={overview?.daemon ? `poll ${overview.daemon.poll_interval_ms} ms` : "idle"}
            tone={overview?.daemon?.running ? "positive" : "negative"}
          />
          <MetricCard
            label="Sessions"
            value={String(overview?.sessions.length ?? 0)}
            detail={selectedSession ? sessionLabel(selectedSession) : "no selection"}
          />
          <MetricCard
            label="Feed Hub"
            value={overview?.daemon ? String(overview.daemon.subscription_count) : "0"}
            detail={overview?.daemon ? `${overview.daemon.armed_feed_count} armed` : "no daemon"}
          />
        </div>
        <div className="app__status">{status}</div>
      </header>

      <main className="paper-layout">
        <aside className="paper-sidebar panel">
          <div className="panel__titlebar">
            <h2 className="panel__title">Strategies</h2>
            <span className="panel__meta">
              {overviewLoading ? "Refreshing" : `${overview?.sessions.length ?? 0} tracked`}
            </span>
          </div>
          <div className="paper-session-list">
            {overview?.sessions.length ? (
              overview.sessions.map((session) => {
                const active = session.manifest.session_id === selectedSessionId;
                const sessionSummary = session.snapshot?.summary ?? null;
                return (
                  <button
                    key={session.manifest.session_id}
                    className={`paper-session-card${active ? " paper-session-card--active" : ""}`}
                    type="button"
                    onClick={() => setSelectedSessionId(session.manifest.session_id)}
                  >
                    <div className="paper-session-card__header">
                      <strong>{sessionLabel(session)}</strong>
                      <span className={`status-pill status-pill--${toneForStatus(session.manifest.health)}`}>
                        {session.manifest.health}
                      </span>
                    </div>
                    <span className="paper-session-card__meta">
                      {session.manifest.execution_sources
                        .map((source) => `${source.alias}:${source.template}`)
                        .join(" · ")}
                    </span>
                    <div className="paper-session-card__stats">
                      <span>{sessionSummary ? formatPercent(sessionSummary.total_return * 100) : "NA"}</span>
                      <span>{sessionSummary ? formatNumber(sessionSummary.ending_equity) : "No snapshot"}</span>
                    </div>
                  </button>
                );
              })
            ) : (
              <div className="empty-state">No paper sessions have been submitted.</div>
            )}
          </div>
        </aside>

        <section className="paper-main">
          {selectedSession && snapshot ? (
            <>
              <section className="panel">
                <div className="panel__titlebar">
                  <div>
                    <h2 className="panel__title">{sessionLabel(selectedSession)}</h2>
                    <span className="panel__meta">
                      {selectedSession.manifest.base_interval} · started{" "}
                      {formatTimeLabel(selectedSession.manifest.start_time_ms)}
                    </span>
                  </div>
                  <div className="paper-status-row">
                    <span className={`status-pill status-pill--${toneForStatus(snapshot.health)}`}>
                      {snapshot.health}
                    </span>
                    <span className={`status-pill status-pill--${toneForStatus(snapshot.status)}`}>
                      {snapshot.status}
                    </span>
                    {detailLoading ? <span className="panel__meta">Updating…</span> : null}
                  </div>
                </div>
                <div className="summary-grid summary-grid--paper">
                  <MetricCard label="Ending Equity" value={summary ? formatNumber(summary.ending_equity) : "NA"} />
                  <MetricCard
                    label="Total Return"
                    value={summary ? formatPercent(summary.total_return * 100) : "NA"}
                    tone={summary && summary.total_return >= 0 ? "positive" : "negative"}
                  />
                  <MetricCard
                    label="Realized PnL"
                    value={summary?.realized_pnl !== undefined ? formatSigned(summary.realized_pnl) : "NA"}
                    tone={summary?.realized_pnl !== undefined && summary.realized_pnl >= 0 ? "positive" : "negative"}
                  />
                  <MetricCard
                    label="Unrealized PnL"
                    value={summary?.unrealized_pnl !== undefined ? formatSigned(summary.unrealized_pnl) : "NA"}
                    tone={summary?.unrealized_pnl !== undefined && summary.unrealized_pnl >= 0 ? "positive" : "negative"}
                  />
                  <MetricCard label="Trades" value={String(snapshot.trade_count)} />
                  <MetricCard label="Win Rate" value={summary ? formatPercent(summary.win_rate * 100) : "NA"} />
                  <MetricCard
                    label="Sharpe"
                    value={summary?.sharpe_ratio !== undefined && summary?.sharpe_ratio !== null ? formatNumber(summary.sharpe_ratio, 3) : "NA"}
                  />
                  <MetricCard
                    label="Max Drawdown"
                    value={summary ? formatNumber(summary.max_drawdown) : "NA"}
                    tone="negative"
                  />
                  <MetricCard label="Open Positions" value={String(snapshot.open_positions.length)} />
                  <MetricCard label="Open Orders" value={String(snapshot.open_order_count)} />
                  <MetricCard
                    label="Fill Rate"
                    value={
                      diagnostics?.summary.order_fill_rate !== undefined
                        ? formatPercent(diagnostics.summary.order_fill_rate * 100)
                        : "NA"
                    }
                  />
                  <MetricCard
                    label="Avg Hold"
                    value={
                      diagnostics?.summary.average_bars_held !== undefined
                        ? `${formatNumber(diagnostics.summary.average_bars_held)} bars`
                        : "NA"
                    }
                  />
                </div>
              </section>

              <section className="paper-grid paper-grid--hero">
                <section className="panel">
                  <div className="panel__titlebar">
                    <h2 className="panel__title">Equity Curve</h2>
                    <span className="panel__meta">
                      {result?.equity_curve.length ?? 0} points
                    </span>
                  </div>
                  {result?.equity_curve && result.equity_curve.length > 1 ? (
                    <LineChart
                      series={[
                        {
                          values: result.equity_curve.map((point) => point.equity),
                          stroke: "#1f8de1",
                          fill: "rgba(31, 141, 225, 0.14)",
                        },
                      ]}
                    />
                  ) : (
                    <div className="empty-state">No equity curve yet.</div>
                  )}
                </section>

                <section className="panel">
                  <div className="panel__titlebar">
                    <h2 className="panel__title">Exposure</h2>
                    <span className="panel__meta">
                      max gross {summary?.max_gross_exposure !== undefined ? formatNumber(summary.max_gross_exposure) : "NA"}
                    </span>
                  </div>
                  {result?.equity_curve && result.equity_curve.length > 1 ? (
                    <>
                      <LineChart
                        series={[
                          {
                            values: result.equity_curve.map((point) => point.gross_exposure ?? 0),
                            stroke: "#f59e0b",
                          },
                          {
                            values: result.equity_curve.map((point) => Math.abs(point.net_exposure ?? 0)),
                            stroke: "#ef4444",
                          },
                        ]}
                      />
                      <div className="legend-row">
                        <span><i className="legend-swatch legend-swatch--amber" /> Gross</span>
                        <span><i className="legend-swatch legend-swatch--red" /> Net</span>
                      </div>
                    </>
                  ) : (
                    <div className="empty-state">No exposure curve yet.</div>
                  )}
                </section>
              </section>

              <section className="paper-grid">
                <section className="panel">
                  <div className="panel__titlebar">
                    <h2 className="panel__title">Feed Health</h2>
                    <span className="panel__meta">
                      {snapshot.feed_summary.live_ready_feeds}/{snapshot.feed_summary.total_feeds} live
                    </span>
                  </div>
                  <div className="summary-grid">
                    <MetricCard label="History Ready" value={String(snapshot.feed_summary.history_ready_feeds)} />
                    <MetricCard label="Live Ready" value={String(snapshot.feed_summary.live_ready_feeds)} />
                    <MetricCard label="Failed" value={String(snapshot.feed_summary.failed_feeds)} tone={snapshot.feed_summary.failed_feeds > 0 ? "negative" : "neutral"} />
                    <MetricCard
                      label="Latest Closed Bar"
                      value={
                        snapshot.latest_closed_bar_time_ms
                          ? formatTimeLabel(snapshot.latest_closed_bar_time_ms)
                          : "NA"
                      }
                    />
                  </div>
                  <div className="list">
                    {snapshot.feed_snapshots.length ? (
                      snapshot.feed_snapshots.map((feed, index) => (
                        <article className="list-card" key={`${feed.execution_alias}-${index}`}>
                          <strong>
                            {feed.execution_alias} · {feed.symbol}
                          </strong>
                          <span>
                            {feed.template} · {feed.interval ?? selectedSession.manifest.base_interval} · {feed.arming_state ?? "n/a"}
                          </span>
                          <span>
                            top {feed.top_of_book ? formatNumber(feed.top_of_book.mid_price, 4) : "NA"} · last{" "}
                            {feed.last_price ? formatNumber(feed.last_price.price, 4) : "NA"} · mark{" "}
                            {feed.mark_price ? formatNumber(feed.mark_price.price, 4) : "NA"}
                          </span>
                          {feed.failure_message ? <span>{feed.failure_message}</span> : null}
                        </article>
                      ))
                    ) : (
                      <div className="empty-state">No feed snapshots yet.</div>
                    )}
                  </div>
                </section>

                <section className="panel">
                  <div className="panel__titlebar">
                    <h2 className="panel__title">Open Positions</h2>
                    <span className="panel__meta">{snapshot.open_positions.length}</span>
                  </div>
                  <div className="list">
                    {snapshot.open_positions.length ? (
                      snapshot.open_positions.map((position, index) => (
                        <article className="list-card" key={`${position.execution_alias}-${index}`}>
                          <strong>
                            {position.execution_alias} · {position.side} · {formatNumber(position.quantity, 4)}
                          </strong>
                          <span>
                            entry {formatNumber(position.entry_price)} · mark {formatNumber(position.market_price)}
                          </span>
                          <span>unrealized {formatSigned(position.unrealized_pnl)}</span>
                          {renderMarginLine(position)}
                        </article>
                      ))
                    ) : (
                      <div className="empty-state">No open positions.</div>
                    )}
                  </div>
                </section>
              </section>

              <section className="paper-grid">
                <section className="panel">
                  <div className="panel__titlebar">
                    <h2 className="panel__title">Recent Trades</h2>
                    <span className="panel__meta">{result?.trades.length ?? 0}</span>
                  </div>
                  <div className="list">
                    {result?.trades?.length ? (
                      [...result.trades]
                        .slice(-50)
                        .reverse()
                        .map((trade, index) => (
                          <article className="list-card" key={index}>
                            <strong>
                              {trade.execution_alias ?? "session"} · {trade.side} · {trade.entry_module ?? "entry"}
                            </strong>
                            <span>
                              {formatTimeLabel(trade.entry.time)} → {formatTimeLabel(trade.exit.time)}
                            </span>
                            <span>
                              entry {formatNumber(trade.entry.price)} · exit {formatNumber(trade.exit.price)} · pnl{" "}
                              {formatSigned(trade.realized_pnl)}
                            </span>
                          </article>
                        ))
                    ) : (
                      <div className="empty-state">No trades yet.</div>
                    )}
                  </div>
                </section>

                <section className="panel">
                  <div className="panel__titlebar">
                    <h2 className="panel__title">Orders</h2>
                    <span className="panel__meta">{result?.orders.length ?? snapshot.open_order_count}</span>
                  </div>
                  <div className="list">
                    {result?.orders?.length ? (
                      [...result.orders]
                        .slice(-50)
                        .reverse()
                        .map((order, index) => (
                          <article className="list-card" key={index}>
                            <strong>
                              {order.execution_alias ?? "session"} · {order.role} · {order.kind}
                            </strong>
                            <span>
                              {order.status}
                              {order.end_reason ? ` · ${order.end_reason}` : ""}
                            </span>
                            <span>
                              placed {formatTimeLabel(order.placed_time)} · fill{" "}
                              {order.fill_time ? formatTimeLabel(order.fill_time) : "NA"} · px{" "}
                              {order.fill_price === null ? "NA" : formatNumber(order.fill_price)}
                            </span>
                          </article>
                        ))
                    ) : (
                      <div className="empty-state">No order history yet.</div>
                    )}
                  </div>
                </section>
              </section>

              <section className="paper-grid">
                <section className="panel">
                  <div className="panel__titlebar">
                    <h2 className="panel__title">Diagnostics</h2>
                    <span className="panel__meta">cohorts and risk</span>
                  </div>
                  {diagnostics ? (
                    <div className="paper-diagnostics">
                      <div className="summary-grid">
                        <MetricCard label="Average Bars To Fill" value={formatMetric(diagnostics.summary.average_bars_to_fill)} />
                        <MetricCard label="Average MAE %" value={formatMetric(diagnostics.summary.average_mae_pct)} />
                        <MetricCard label="Average MFE %" value={formatMetric(diagnostics.summary.average_mfe_pct)} />
                        <MetricCard label="Signal Exits" value={String(diagnostics.summary.signal_exit_count ?? 0)} />
                      </div>
                      <DiagnosticTable
                        title="By Side"
                        rows={(diagnostics.cohorts?.by_side ?? []).map((entry) => [
                          entry.side,
                          String(entry.trade_count),
                          formatPercent(entry.win_rate * 100),
                          formatNumber(entry.average_realized_pnl),
                        ])}
                        headers={["Side", "Trades", "Win", "Avg PnL"]}
                      />
                      <DiagnosticTable
                        title="Exit Classes"
                        rows={(diagnostics.cohorts?.by_exit_classification ?? []).map((entry) => [
                          entry.classification,
                          String(entry.trade_count),
                          formatPercent(entry.win_rate * 100),
                          formatNumber(entry.average_realized_pnl),
                        ])}
                        headers={["Exit", "Trades", "Win", "Avg PnL"]}
                      />
                      <DiagnosticTable
                        title="Weekday"
                        rows={(diagnostics.cohorts?.by_weekday_utc ?? []).map((entry) => [
                          `UTC ${entry.weekday_utc}`,
                          String(entry.trade_count),
                          formatPercent(entry.win_rate * 100),
                          formatNumber(entry.total_realized_pnl),
                        ])}
                        headers={["Bucket", "Trades", "Win", "Total PnL"]}
                      />
                    </div>
                  ) : (
                    <div className="empty-state">Detailed diagnostics are not available yet.</div>
                  )}
                </section>

                <section className="panel">
                  <div className="panel__titlebar">
                    <h2 className="panel__title">Drawdown and Hints</h2>
                    <span className="panel__meta">
                      {diagnostics?.overfitting_risk?.level ?? "unknown"} risk
                    </span>
                  </div>
                  {diagnostics ? (
                    <div className="paper-diagnostics">
                      <div className="summary-grid">
                        <MetricCard label="Longest Drawdown" value={formatBars(diagnostics.drawdown?.longest_drawdown_bars)} />
                        <MetricCard label="Current Drawdown" value={formatBars(diagnostics.drawdown?.current_drawdown_bars)} />
                        <MetricCard label="Stagnation" value={formatBars(diagnostics.drawdown?.longest_stagnation_bars)} />
                        <MetricCard label="Recovery" value={formatBarsFloat(diagnostics.drawdown?.average_recovery_bars)} />
                      </div>
                      <div className="list">
                        {(diagnostics.hints ?? []).length ? (
                          diagnostics.hints?.map((hint, index) => (
                            <article className="list-card" key={index}>
                              <strong>{hint.kind}</strong>
                              <span>
                                {hint.metric ?? "metric"} {hint.value !== null && hint.value !== undefined ? formatNumber(hint.value) : "NA"}
                              </span>
                            </article>
                          ))
                        ) : (
                          <div className="empty-state">No improvement hints.</div>
                        )}
                      </div>
                    </div>
                  ) : (
                    <div className="empty-state">No drawdown diagnostics yet.</div>
                  )}
                </section>
              </section>

              <section className="paper-grid">
                <section className="panel">
                  <div className="panel__titlebar">
                    <h2 className="panel__title">Transfer and Arbitrage</h2>
                    <span className="panel__meta">portfolio extras</span>
                  </div>
                  {diagnostics ? (
                    <div className="paper-diagnostics">
                      <DiagnosticTable
                        title="Transfers"
                        rows={(diagnostics.transfer_summary?.by_route ?? []).map((route) =>
                          transferRouteRow(route),
                        )}
                        headers={["Route", "Count", "Completed", "Fee"]}
                      />
                      <DiagnosticTable
                        title="Arbitrage"
                        rows={(diagnostics.arbitrage?.by_pair ?? []).map((pair) =>
                          arbitragePairRow(pair),
                        )}
                        headers={["Pair", "Baskets", "Completed", "PnL"]}
                      />
                    </div>
                  ) : (
                    <div className="empty-state">No transfer or arbitrage diagnostics.</div>
                  )}
                </section>

                <section className="panel">
                  <div className="panel__titlebar">
                    <h2 className="panel__title">Session Log</h2>
                    <span className="panel__meta">{logs?.logs.length ?? 0} events</span>
                  </div>
                  <div className="list">
                    {logs?.logs.length ? (
                      [...logs.logs]
                        .slice(-40)
                        .reverse()
                        .map((event: PaperSessionLogEvent, index) => (
                          <article className="list-card" key={index}>
                            <strong>{event.message}</strong>
                            <span>
                              {formatTimeLabel(event.time_ms)} · {event.status} · {event.health}
                            </span>
                          </article>
                        ))
                    ) : (
                      <div className="empty-state">No log events yet.</div>
                    )}
                  </div>
                </section>
              </section>
            </>
          ) : (
            <section className="panel">
              <div className="empty-state empty-state--large">
                Select a running strategy to inspect live paper metrics.
              </div>
            </section>
          )}
        </section>
      </main>
    </div>
  );
}

function buildOverviewStatus(
  daemon: ExecutionDaemonStatus | null | undefined,
  sessionCount: number,
): string {
  if (!daemon) {
    return sessionCount === 0
      ? "No paper daemon status found. Submit a session and start `execution serve`."
      : "Sessions are persisted, but the daemon has not written status yet.";
  }
  return daemon.running
    ? `Daemon online with ${daemon.subscription_count} subscriptions and ${sessionCount} tracked session(s).`
    : "Daemon status file exists, but the process is not currently running.";
}

function daemonHeadline(daemon: ExecutionDaemonStatus | null): string {
  if (!daemon) {
    return "offline";
  }
  return daemon.running ? "running" : "stopped";
}

function sessionLabel(session: PaperDashboardSession): string {
  const script = session.manifest.script_path?.split("/").pop();
  if (script) {
    return script;
  }
  const primary = session.manifest.execution_sources[0];
  return primary ? `${primary.template}:${primary.symbol}` : session.manifest.session_id;
}

function toneForStatus(status: string): "positive" | "negative" | "neutral" {
  const value = status.toLowerCase();
  if (value === "live" || value === "running") {
    return "positive";
  }
  if (value === "failed" || value === "stopped" || value === "degraded") {
    return "negative";
  }
  return "neutral";
}

function formatSigned(value: number): string {
  const prefix = value >= 0 ? "+" : "";
  return `${prefix}${formatNumber(value)}`;
}

function renderMarginLine(position: PositionSnapshot) {
  if (
    position.free_collateral === undefined &&
    position.isolated_margin === undefined &&
    position.liquidation_price === undefined
  ) {
    return null;
  }
  return (
    <span>
      collateral {formatMaybe(position.free_collateral)} · isolated {formatMaybe(position.isolated_margin)} · liq{" "}
      {formatMaybe(position.liquidation_price)}
    </span>
  );
}

function formatMaybe(value: number | null | undefined): string {
  return value === null || value === undefined ? "NA" : formatNumber(value);
}

function formatMetric(value: number | null | undefined): string {
  return value === null || value === undefined ? "NA" : formatNumber(value);
}

function formatBars(value: number | null | undefined): string {
  return value === null || value === undefined ? "NA" : `${value} bars`;
}

function formatBarsFloat(value: number | null | undefined): string {
  return value === null || value === undefined ? "NA" : `${formatNumber(value)} bars`;
}

function transferRouteRow(route: TransferRouteDiagnosticSummary): string[] {
  return [
    `${route.from_alias} → ${route.to_alias}`,
    String(route.transfer_count),
    String(route.completed_transfer_count),
    formatNumber(route.total_fee),
  ];
}

function arbitragePairRow(pair: ArbitragePairDiagnosticSummary): string[] {
  return [
    `${pair.buy_alias} / ${pair.sell_alias}`,
    String(pair.basket_count),
    String(pair.completed_basket_count),
    formatNumber(pair.total_realized_pnl),
  ];
}

function DiagnosticTable({
  title,
  headers,
  rows,
}: {
  title: string;
  headers: string[];
  rows: string[][];
}) {
  return (
    <div className="diagnostic-table">
      <div className="diagnostic-table__title">{title}</div>
      {rows.length ? (
        <table>
          <thead>
            <tr>
              {headers.map((header) => (
                <th key={header}>{header}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, index) => (
              <tr key={index}>
                {row.map((cell, cellIndex) => (
                  <td key={cellIndex}>{cell}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <div className="empty-state">No data.</div>
      )}
    </div>
  );
}
