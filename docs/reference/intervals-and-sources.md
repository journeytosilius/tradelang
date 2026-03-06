# Intervals and Sources

This page defines the normative interval and source rules for PalmScript.

## Supported Intervals

PalmScript accepts the interval literals listed in [Interval Table](intervals.md).

Intervals are case-sensitive.

## Base Interval

Every script has exactly one base interval:

```palmscript
interval 1m
```

The base interval defines the execution clock.

## Source-Less Scripts

A script with no `source` declarations is source-less.

Rules:

- bare market series such as `close` and `volume` are valid
- `use <interval>` declares additional intervals for qualified references such as `1w.close`
- the runtime prepares those intervals by rolling the raw CSV feed upward when possible

## Source-Aware Scripts

A script with one or more `source` declarations is source-aware.

Example:

```palmscript
interval 1m
source hl = hyperliquid.perps("BTC")
source bn = binance.spot("BTCUSDT")
use hl 1h

plot(bn.close - hl.1h.close)
```

Rules:

- market series must be source-qualified
- each declared source contributes a base feed on the script base interval
- `use <alias> <interval>` declares additional intervals for that source
- `<alias>.<interval>.<field>` is valid without `use` when `<interval>` equals the base interval
- lower-than-base interval references are rejected

## Supported Source Templates

PalmScript currently supports these first-class templates:

- `binance.spot("<symbol>")`
- `binance.usdm("<symbol>")`
- `hyperliquid.spot("<symbol>")`
- `hyperliquid.perps("<symbol>")`

Interval support is template-specific.

Rules:

- `binance.spot` accepts all supported PalmScript interval literals
- `binance.usdm` accepts all supported PalmScript interval literals
- `hyperliquid.spot` rejects `1s` and `6h`
- `hyperliquid.perps` rejects `1s` and `6h`

Operational fetch constraints are also template-specific:

- Hyperliquid REST only exposes the most recent `5000` candles per feed
- market mode must reject any Hyperliquid feed request that exceeds that retention window
- Binance feeds are paginated internally and do not have the same whole-window retention cap in PalmScript market mode

## Source Field Set

All source templates are normalized into the same canonical OHLCV schema:

```text
time,open,high,low,close,volume
```

Rules:

- `time` is the candle open time in Unix milliseconds UTC
- `open`, `high`, `low`, `close`, and `volume` are numeric
- venue-specific extra fields are not exposed directly in the language

## Equal, Higher, and Lower Intervals

PalmScript distinguishes three cases for a referenced interval relative to the base interval:

- equal interval: valid
- higher interval: valid if declared with `use`, except that source-aware explicit equal-interval references do not need `use`
- lower interval: rejected

This rule exists in both source-less and source-aware scripts.

## Source-Less Runtime Semantics

In CSV mode:

- the raw CSV file is treated as one base source of OHLCV bars
- the runtime executes on the declared base interval
- declared higher intervals are prepared by strict roll-up before VM execution

The detailed CSV contract is defined in [CSV Mode](../tooling/csv-mode.md).

## Source-Aware Runtime Semantics

In market mode:

- the runtime fetches the required `(source, interval)` feeds directly from the venues
- the base execution timeline is the union of all declared-source base-interval bar open times
- if one source has no base bar at a timeline step, that source contributes `na` for that step
- slower source intervals retain their last fully closed value until their next close boundary

The detailed fetch contract is defined in [Market Mode](../tooling/market-mode.md).

## No-Lookahead Guarantee

PalmScript must not expose a higher-interval candle before that candle is fully closed.

This applies equally to:

- source-less qualified intervals such as `1w.close`
- source-aware qualified intervals such as `hl.1h.close`

## Runtime Alignment Rules

Prepared feeds must be aligned to their declared intervals.

The runtime rejects feeds that are:

- misaligned to the interval boundary
- unsorted
- duplicated at one interval open time
