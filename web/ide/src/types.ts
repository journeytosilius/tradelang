export type PublicDatasetId = "btcusdt_binance_spot_4h_4y";

export interface PublicDataset {
  dataset_id: PublicDatasetId;
  display_name: string;
  from: number;
  to: number;
  initial_capital: number;
  fee_bps: number;
  slippage_bps: number;
}

export interface PublicDatasetCatalog {
  datasets: PublicDataset[];
}

export interface DiagnosticPosition {
  offset: number;
  line: number;
  column: number;
}

export interface DiagnosticSpan {
  start: DiagnosticPosition;
  end: DiagnosticPosition;
}

export interface Diagnostic {
  kind: "lex" | "parse" | "type" | "compile";
  message: string;
  span: DiagnosticSpan;
}

export interface HighlightPosition {
  offset: number;
  line: number;
}

export interface HighlightSpan {
  start: HighlightPosition;
  end: HighlightPosition;
}

export interface HighlightToken {
  span: HighlightSpan;
  kind:
    | "keyword"
    | "string"
    | "number"
    | "function"
    | "variable"
    | "type"
    | "field"
    | "operator";
}

export interface CheckResponse {
  diagnostics: Diagnostic[];
  highlights: HighlightToken[];
}

export interface CheckRequest {
  script: string;
}

export type CompletionKind =
  | "keyword"
  | "builtin"
  | "series"
  | "source"
  | "interval"
  | "field"
  | "function"
  | "variable";

export interface CompletionEntry {
  label: string;
  kind: CompletionKind;
  detail: string | null;
  documentation: string | null;
  insert_text: string;
  insert_text_format: "plain_text" | "snippet";
}

export interface CompletionsRequest {
  script: string;
  offset: number;
}

export interface CompletionsResponse {
  items: CompletionEntry[];
}

export interface HoverPosition {
  offset: number;
  line: number;
  column: number;
}

export interface HoverSpan {
  start: HoverPosition;
  end: HoverPosition;
}

export interface HoverInfo {
  span: HoverSpan;
  contents: string;
}

export interface HoverRequest {
  script: string;
  offset: number;
}

export interface HoverResponse {
  hover: HoverInfo | null;
}

export interface BacktestRequest {
  script: string;
  dataset_id: PublicDatasetId;
  from_ms: number;
  to_ms: number;
}

export interface TradeFill {
  execution_alias?: string;
  bar_index?: number;
  time: number;
  action?: string;
  quantity?: number;
  raw_price?: number;
  price: number;
  notional?: number;
  fee?: number;
}

export interface Trade {
  execution_alias?: string;
  side: string;
  entry_module?: string | null;
  quantity?: number;
  entry: TradeFill;
  exit: TradeFill;
  realized_pnl: number;
}

export interface OrderRecord {
  id?: number;
  execution_alias?: string;
  role: string;
  kind: string;
  status: string;
  placed_time: number;
  fill_time?: number | null;
  fill_price: number | null;
  end_reason?: string | null;
}

export interface EquityPoint {
  bar_index?: number;
  time?: number;
  cash?: number;
  equity: number;
  gross_exposure?: number;
  net_exposure?: number;
  open_position_count?: number;
  long_position_count?: number;
  short_position_count?: number;
}

export interface BacktestSummary {
  starting_equity?: number;
  ending_equity: number;
  realized_pnl?: number;
  unrealized_pnl?: number;
  total_return: number;
  sharpe_ratio?: number | null;
  trade_count: number;
  winning_trade_count?: number;
  losing_trade_count?: number;
  win_rate: number;
  max_drawdown: number;
  max_gross_exposure?: number;
  max_net_exposure?: number;
  peak_open_position_count?: number;
}

export interface BacktestDiagnosticsSummary {
  order_fill_rate?: number;
  average_bars_to_fill?: number;
  average_bars_held?: number;
  average_mae_pct?: number;
  average_mfe_pct?: number;
  entered_trade_count?: number;
  closed_trade_count?: number;
  signal_exit_count?: number;
  protect_exit_count?: number;
  target_exit_count?: number;
  reversal_exit_count?: number;
  liquidation_exit_count?: number;
}

export interface SideDiagnosticSummary {
  side: string;
  trade_count: number;
  win_rate: number;
  average_realized_pnl: number;
  average_bars_held: number;
  average_mae_pct: number;
  average_mfe_pct: number;
}

export interface ExitClassificationDiagnosticSummary {
  classification: string;
  trade_count: number;
  win_rate: number;
  average_realized_pnl: number;
}

export interface WeekdayDiagnosticSummary {
  weekday_utc: number;
  trade_count: number;
  win_rate: number;
  total_realized_pnl: number;
}

export interface HourDiagnosticSummary {
  hour_utc: number;
  trade_count: number;
  win_rate: number;
  total_realized_pnl: number;
}

export interface TimeBucketUtcDiagnosticSummary {
  start_hour_utc: number;
  end_hour_utc: number;
  trade_count: number;
  winning_trade_count: number;
  win_rate: number;
  total_realized_pnl: number;
  average_realized_pnl: number;
}

export interface HoldingTimeBucketSummary {
  bucket: string;
  trade_count: number;
  win_rate: number;
  average_realized_pnl: number;
}

export interface EntryModuleDiagnosticSummary {
  name: string;
  trade_count: number;
  long_trade_count: number;
  short_trade_count: number;
  win_rate: number;
  total_realized_pnl: number;
  average_realized_pnl: number;
  average_bars_held: number;
}

export interface CohortDiagnostics {
  by_side: SideDiagnosticSummary[];
  by_exit_classification: ExitClassificationDiagnosticSummary[];
  by_weekday_utc: WeekdayDiagnosticSummary[];
  by_hour_utc: HourDiagnosticSummary[];
  by_time_bucket_utc: TimeBucketUtcDiagnosticSummary[];
  by_holding_time: HoldingTimeBucketSummary[];
  by_entry_module: EntryModuleDiagnosticSummary[];
}

export interface DrawdownDiagnostics {
  longest_drawdown_bars: number;
  current_drawdown_bars: number;
  longest_stagnation_bars: number;
  average_recovery_bars: number;
}

export interface ImprovementHint {
  kind: string;
  metric?: string | null;
  value?: number | null;
}

export interface OverfittingRiskReason {
  kind: string;
  metric?: string | null;
  value?: number | null;
}

export interface OverfittingRiskSummary {
  level: string;
  score: number;
  reasons: OverfittingRiskReason[];
}

export interface TransferRouteDiagnosticSummary {
  from_alias: string;
  to_alias: string;
  transfer_count: number;
  completed_transfer_count: number;
  total_amount: number;
  total_fee: number;
  average_delay_bars: number;
}

export interface TransferDiagnosticsSummary {
  quote_transfer_count: number;
  completed_quote_transfer_count: number;
  pending_quote_transfer_count: number;
  total_quote_amount: number;
  total_quote_fee: number;
  average_delay_bars: number;
  by_route: TransferRouteDiagnosticSummary[];
}

export interface ArbitragePairDiagnosticSummary {
  buy_alias: string;
  sell_alias: string;
  basket_count: number;
  completed_basket_count: number;
  total_realized_pnl: number;
  average_entry_spread_bps: number;
  average_exit_spread_bps: number;
  average_holding_bars: number;
}

export interface ArbitrageDiagnosticsSummary {
  basket_count: number;
  completed_basket_count: number;
  open_basket_count: number;
  total_realized_pnl: number;
  average_entry_spread_bps: number;
  average_exit_spread_bps: number;
  average_holding_bars: number;
  by_pair: ArbitragePairDiagnosticSummary[];
}

export interface BacktestDiagnostics {
  summary: BacktestDiagnosticsSummary;
  cohorts?: CohortDiagnostics;
  drawdown?: DrawdownDiagnostics;
  hints?: ImprovementHint[];
  overfitting_risk?: OverfittingRiskSummary;
  transfer_summary?: TransferDiagnosticsSummary;
  arbitrage?: ArbitrageDiagnosticsSummary;
}

export interface BacktestResult {
  orders: OrderRecord[];
  fills?: TradeFill[];
  trades: Trade[];
  diagnostics: BacktestDiagnostics;
  equity_curve: EquityPoint[];
  summary: BacktestSummary;
  open_positions?: PositionSnapshot[];
}

export interface BacktestResponse {
  dataset: PublicDataset;
  result: BacktestResult;
}

export interface PaperFeedSummary {
  total_feeds: number;
  history_ready_feeds: number;
  live_ready_feeds: number;
  failed_feeds: number;
}

export interface PaperExecutionSource {
  alias: string;
  template: string;
  symbol: string;
}

export interface PriceSnapshot {
  time_ms: number;
  price: number;
  state: "live" | "stale" | "missing";
}

export interface TopOfBookSnapshot {
  time_ms: number;
  best_bid: number;
  best_ask: number;
  mid_price: number;
  state: "live" | "stale" | "missing";
}

export interface PaperFeedSnapshot {
  execution_alias: string;
  template: string;
  symbol: string;
  interval?: string | null;
  arming_state?: string | null;
  history_ready: boolean;
  live_ready: boolean;
  latest_closed_bar_time_ms?: number | null;
  top_of_book?: TopOfBookSnapshot | null;
  last_price?: PriceSnapshot | null;
  mark_price?: PriceSnapshot | null;
  valuation_source?: string | null;
  failure_message?: string | null;
}

export interface PositionSnapshot {
  execution_alias: string;
  side: string;
  quantity: number;
  entry_bar_index: number;
  entry_time: number;
  entry_price: number;
  market_price: number;
  market_time: number;
  unrealized_pnl: number;
  free_collateral?: number | null;
  isolated_margin?: number | null;
  maintenance_margin?: number | null;
  liquidation_price?: number | null;
}

export interface PaperSessionManifest {
  session_id: string;
  mode: "paper";
  created_at_ms: number;
  updated_at_ms: number;
  start_time_ms: number;
  status: string;
  health: string;
  stop_requested: boolean;
  failure_message?: string | null;
  script_path?: string | null;
  script_sha256: string;
  base_interval: string;
  history_capacity: number;
  config: {
    execution_source_aliases: string[];
    initial_capital: number;
    maker_fee_bps: number;
    taker_fee_bps: number;
    slippage_bps: number;
    diagnostics_detail: string;
    leverage?: number | null;
    margin_mode?: string | null;
  };
  execution_sources: PaperExecutionSource[];
  feed_summary: PaperFeedSummary;
  required_feeds: PaperFeedSnapshot[];
  warmup_from_ms?: number | null;
  latest_runtime_to_ms?: number | null;
}

export interface PaperSessionSnapshot {
  session_id: string;
  status: string;
  health: string;
  updated_at_ms: number;
  start_time_ms: number;
  warmup_from_ms?: number | null;
  latest_runtime_to_ms?: number | null;
  latest_closed_bar_time_ms?: number | null;
  summary?: BacktestSummary | null;
  diagnostics_summary?: BacktestDiagnosticsSummary | null;
  open_positions: PositionSnapshot[];
  feed_snapshots: PaperFeedSnapshot[];
  feed_summary: PaperFeedSummary;
  open_order_count: number;
  filled_order_count: number;
  cancelled_order_count: number;
  rejected_order_count: number;
  expired_order_count: number;
  fill_count: number;
  trade_count: number;
  failure_message?: string | null;
}

export interface PaperSessionExport {
  manifest: PaperSessionManifest;
  snapshot?: PaperSessionSnapshot | null;
  latest_result?: BacktestResult | null;
}

export interface PaperSessionLogEvent {
  time_ms: number;
  status: string;
  health: string;
  message: string;
  latest_runtime_to_ms?: number | null;
}

export interface ExecutionDaemonStatus {
  pid: number;
  started_at_ms: number;
  updated_at_ms: number;
  poll_interval_ms: number;
  once: boolean;
  running: boolean;
  stop_requested: boolean;
  active_sessions: string[];
  subscription_count: number;
  armed_feed_count: number;
  connecting_feed_count: number;
  degraded_feed_count: number;
  failed_feed_count: number;
  state_root: string;
}

export interface PaperDashboardSession {
  manifest: PaperSessionManifest;
  snapshot?: PaperSessionSnapshot | null;
}

export interface PaperDashboardOverview {
  daemon?: ExecutionDaemonStatus | null;
  sessions: PaperDashboardSession[];
}

export interface PaperSessionDetailResponse {
  export: PaperSessionExport;
}

export interface PaperSessionLogsResponse {
  session_id: string;
  logs: PaperSessionLogEvent[];
}
