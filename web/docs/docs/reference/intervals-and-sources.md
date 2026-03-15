# Intervals and Sources

This page defines the normative interval and source rules for PalmScript.

## Supported Intervals

PalmScript accepts the interval literals listed in [Interval Table](intervals.md). Intervals are case-sensitive.

## Base Interval

Every script declares exactly one base interval:

```palmscript
interval 1m
```

The base interval defines the execution clock.

## Named Sources

Executable scripts declare one or more named exchange-backed sources:

```palmscript
interval 1m
source bb = bybit.usdt_perps("BTCUSDT")
source bn = binance.spot("BTCUSDT")
use bb 1h

plot(bn.close - bb.1h.close)
```

Rules:

- at least one `source` declaration is required
- market series must be source-qualified
- each declared source contributes a base feed on the script base interval
- `use <alias> <interval>` declares an additional interval for that source
- `<alias>.<field>` refers to that source on the base interval
- `<alias>.<interval>.<field>` refers to that source on the named interval
- lower-than-base interval references are rejected

## Supported Source Templates

PalmScript currently supports these first-class templates:

- `binance.spot("<symbol>")`
- `binance.usdm("<symbol>")`
- `bybit.spot("<symbol>")`
- `bybit.usdt_perps("<symbol>")`
- `gate.spot("<symbol>")`
- `gate.usdt_perps("<symbol>")`

Interval support is template-specific:

- `binance.spot` accepts all supported PalmScript intervals
- `binance.usdm` accepts all supported PalmScript intervals
- `bybit.spot` accepts `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w`, and `1M`
- `bybit.usdt_perps` accepts `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w`, and `1M`
- `gate.spot` accepts `1s`, `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h`, `1d`, and `1M`
- `gate.usdt_perps` accepts `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h`, and `1d`

Operational fetch constraints are also template-specific:

- Bybit uses venue-native symbols such as `BTCUSDT`
- Gate uses venue-native symbols such as `BTC_USDT`
- Bybit REST klines arrive reverse-sorted and PalmScript reorders them before runtime alignment checks
- Bybit spot and perp kline timestamps may arrive as JSON integers or integer-like strings; PalmScript accepts both forms directly
- Gate candlestick APIs use Unix seconds and PalmScript normalizes them into Unix milliseconds UTC
- Gate spot and futures pagination is windowed by time because the public API does not allow `limit` with `from` / `to`
- Gate spot and futures requests are capped at 1000 candles per HTTP call so venue range limits do not produce avoidable `400 Bad Request` failures
- Binance, Bybit, and Gate feeds are paginated internally
- venue fetch failures surface the request URL and a truncated response-body snippet when available, including non-200 HTTP failures and malformed JSON payloads
- base URLs can be overridden with `PALMSCRIPT_BINANCE_SPOT_BASE_URL`, `PALMSCRIPT_BINANCE_USDM_BASE_URL`, `PALMSCRIPT_BYBIT_BASE_URL`, and `PALMSCRIPT_GATE_BASE_URL`; Gate accepts either the host root such as `https://api.gateio.ws` or the full `/api/v4` base URL

## Source Field Set

All source templates expose the canonical OHLCV field set:

- `time`
- `open`
- `high`
- `low`
- `close`
- `volume`

Rules:

- `time` is the candle open time in Unix milliseconds UTC
- price and volume fields are numeric

`binance.usdm("<symbol>")` also exposes these historical-only auxiliary fields:

- `funding_rate`
- `mark_price`
- `index_price`
- `premium_index`
- `basis`

Rules:

- auxiliary fields are only valid on `binance.usdm` source aliases
- auxiliary fields keep the same flat source-qualified syntax as OHLCV fields: `<alias>.<field>` and `<alias>.<interval>.<field>`
- historical modes fetch auxiliary datasets automatically when the script references them
- `mark_price`, `index_price`, and `premium_index` resolve to close-equivalent scalar series for the selected interval
- `funding_rate` and `basis` are normalized as carry-forward scalar series on the selected interval and stay `na` until the first fetched event or snapshot
- `run paper` rejects scripts that reference these auxiliary fields until live polling is implemented

## Equal, Higher, and Lower Intervals

PalmScript distinguishes three cases for a referenced interval relative to the base interval:

- equal interval: valid
- higher interval: valid if declared with `use <alias> <interval>`
- lower interval: rejected

## Runtime Semantics

In market mode:

- PalmScript fetches the required `(source, interval, field-family)` data directly from the venues
- the base execution timeline is the union of all declared-source base-interval bar open times
- if one source has no base bar at a timeline step, that source contributes `na` for that step
- slower source intervals retain their last fully closed value until their next close boundary

## No-Lookahead Guarantee

PalmScript must not expose a higher-interval candle before that candle is fully closed.

This applies to source-aware qualified intervals such as `bb.1h.close`.

## Runtime Alignment Rules

Prepared feeds must be aligned to their declared intervals.

The runtime rejects feeds that are:

- misaligned to the interval boundary
- unsorted
- duplicated at one interval open time
