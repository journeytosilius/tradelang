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
  time: number;
  price: number;
}

export interface Trade {
  side: string;
  entry: TradeFill;
  exit: TradeFill;
  realized_pnl: number;
}

export interface OrderRecord {
  role: string;
  kind: string;
  status: string;
  placed_time: number;
  fill_price: number | null;
}

export interface EquityPoint {
  equity: number;
}

export interface BacktestSummary {
  ending_equity: number;
  total_return: number;
  trade_count: number;
  win_rate: number;
  max_drawdown: number;
}

export interface BacktestDiagnosticsSummary {
  entered_trade_count: number;
  closed_trade_count: number;
}

export interface BacktestDiagnostics {
  summary: BacktestDiagnosticsSummary;
}

export interface BacktestResult {
  orders: OrderRecord[];
  trades: Trade[];
  diagnostics: BacktestDiagnostics;
  equity_curve: EquityPoint[];
  summary: BacktestSummary;
}

export interface BacktestResponse {
  dataset: PublicDataset;
  result: BacktestResult;
}
