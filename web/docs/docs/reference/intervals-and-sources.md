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
- Gate candlestick APIs use Unix seconds and PalmScript normalizes them into Unix milliseconds UTC
- Gate spot and futures pagination is windowed by time because the public API does not allow `limit` with `from` / `to`
- Gate spot and futures requests are capped at 1000 candles per HTTP call so venue range limits do not produce avoidable `400 Bad Request` failures
- Binance, Bybit, and Gate feeds are paginated internally
- venue fetch failures surface the HTTP status together with the request URL and a truncated response-body snippet when the venue returns one
- base URLs can be overridden with `PALMSCRIPT_BINANCE_SPOT_BASE_URL`, `PALMSCRIPT_BINANCE_USDM_BASE_URL`, `PALMSCRIPT_BYBIT_BASE_URL`, and `PALMSCRIPT_GATE_BASE_URL`; Gate accepts either the host root such as `https://api.gateio.ws` or the full `/api/v4` base URL

## Source Field Set

All source templates are normalized into the same canonical market fields:

- `time`
- `open`
- `high`
- `low`
- `close`
- `volume`

Rules:

- `time` is the candle open time in Unix milliseconds UTC
- price and volume fields are numeric
- venue-specific extra fields are not exposed in the language

## Equal, Higher, and Lower Intervals

PalmScript distinguishes three cases for a referenced interval relative to the base interval:

- equal interval: valid
- higher interval: valid if declared with `use <alias> <interval>`
- lower interval: rejected

## Runtime Semantics

In market mode:

- PalmScript fetches the required `(source, interval)` feeds directly from the venues
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
