const DAY_MS = 24 * 60 * 60 * 1000;

export const DEFAULT_SOURCE = `interval 4h
source bn = binance.spot("BTCUSDT")
source gt = gate.spot("BTC_USDT")
use bn 1d
use gt 1d

let bn_fast = ema(bn.close, 13)
let bn_slow = ema(bn.close, 55)
let gt_fast = ema(gt.close, 13)
let gt_slow = ema(gt.close, 55)
let bn_daily = ema(bn.1d.close, 20)
let gt_daily = ema(gt.1d.close, 20)
let spread = (bn.close - gt.close) / gt.close

let trend_confirmed = above(bn_fast, bn_slow) and above(gt_fast, gt_slow)
let daily_confirmed = above(bn.1d.close, bn_daily) and above(gt.1d.close, gt_daily)

entry long = trend_confirmed and daily_confirmed and spread < -0.002
exit long = below(bn_fast, bn_slow) or spread > 0.002

plot(spread * 10000)
export spread_bps = spread * 10000
`;

export function dateInputValue(timeMs: number): string {
  return new Date(timeMs).toISOString().slice(0, 10);
}

export function parseDateInput(value: string): number {
  return Date.parse(`${value}T00:00:00Z`);
}

export function defaultWindowForDataset(dataset: {
  from: number;
  to: number;
}): { from: string; to: string } {
  const datasetEnd = dataset.to - DAY_MS;
  const yearWindowStart = Math.max(dataset.from, dataset.to - 365 * DAY_MS);
  return {
    from: dateInputValue(yearWindowStart),
    to: dateInputValue(datasetEnd),
  };
}

export function formatNumber(value: number, digits = 2): string {
  return new Intl.NumberFormat("en-US", {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value);
}

export function formatPercent(value: number): string {
  return `${formatNumber(value, 2)}%`;
}

export function formatDateLabel(timeMs: number): string {
  return new Intl.DateTimeFormat("en-US", {
    year: "numeric",
    month: "short",
    day: "2-digit",
    timeZone: "UTC",
  }).format(timeMs);
}

export function formatTimeLabel(timeMs: number): string {
  return new Intl.DateTimeFormat("en-US", {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    timeZone: "UTC",
  }).format(timeMs);
}

export function clampWindow(
  dataset: { from: number; to: number },
  from: string,
  to: string,
): { fromMs: number; toMs: number } {
  const fromMs = parseDateInput(from);
  const toMs = parseDateInput(to) + DAY_MS;

  if (!Number.isFinite(fromMs) || !Number.isFinite(toMs)) {
    throw new Error("Choose a valid From and To date.");
  }
  if (fromMs >= toMs) {
    throw new Error("The From date must be before the To date.");
  }
  if (fromMs < dataset.from || toMs > dataset.to) {
    throw new Error(
      `The selected window must stay inside ${formatDateLabel(dataset.from)} to ${formatDateLabel(dataset.to - DAY_MS)}.`,
    );
  }
  return { fromMs, toMs };
}
