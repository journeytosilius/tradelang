# Market Mode

`palmscript run market ...` executes source-aware scripts by fetching historical candles directly from supported exchange REST APIs.

## Invocation

```bash
palmscript run market strategy.ps --from 1704067200000 --to 1704153600000
```

Rules:

- the script must declare at least one `source`
- `--from` and `--to` are Unix milliseconds UTC
- `--from` must be strictly less than `--to`

## Supported Source Templates

- `binance.spot("<symbol>")`
- `binance.usdm("<symbol>")`
- `bybit.spot("<symbol>")`
- `bybit.usdt_perps("<symbol>")`
- `gate.spot("<symbol>")`
- `gate.usdt_perps("<symbol>")`

Template-specific interval support:

- `binance.spot`: all supported PalmScript intervals
- `binance.usdm`: all supported PalmScript intervals
- `bybit.spot`: `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w`, `1M`
- `bybit.usdt_perps`: `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w`, `1M`
- `gate.spot`: `1s`, `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h`, `1d`, `1M`
- `gate.usdt_perps`: `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h`, `1d`

Example:

```palmscript
interval 1m
source bn = binance.spot("BTCUSDT")
source bb = bybit.usdt_perps("BTCUSDT")
use bb 1h

plot(bn.close - bb.close)
plot(bb.1h.close)
```

## Fetch Model

Market mode:

- reads declared `source` directives from the script
- determines the required `(source, interval)` feeds from the compiled program
- fetches each required feed directly from the venue
- normalizes venue responses into the canonical typed bar fields `time`, `open`, `high`, `low`, `close`, and `volume`
- runs the VM over the resulting source-aware runtime configuration

## Venue Guardrails

Market mode validates venue-specific constraints before execution.

Current guardrails:

- Bybit source templates reject unsupported intervals such as `1s`, `8h`, and `3d`
- Gate spot and USDT perp templates only accept the interval subsets exposed by their respective candlestick APIs
- Binance spot, Binance USD-M, Bybit, and Gate feeds use venue-specific page sizes internally during pagination

PalmScript fails closed for these constraints. It must not run a strategy on silently truncated exchange history.

## Base Clock

Source-aware scripts still execute on one declared base interval.

The runtime builds that execution clock from the union of all declared-source base-interval bar open times.

On a step where one source has no base candle:

- that source's base fields become `na`
- the other sources still contribute their own current samples
- slower declared intervals keep their last fully closed values until their next close boundary

## Failure Cases

Market mode fails deterministically for:

- scripts with no `source` declarations
- invalid time windows
- request failures
- malformed venue responses
- unsupported source-template intervals
- empty historical windows for a required feed
