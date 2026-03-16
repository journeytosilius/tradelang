# PalmScript

<p align="center">
  <img src="editors/vscode/images/palmscript.png" alt="PalmScript logo" width="220">
</p>

## What Is PalmScript

PalmScript is a deterministic language and VM for financial time-series
analysis, signal research, and strategy execution logic.

It is built for:

- writing indicator and signal scripts against market data
- running deterministic market replays and backtests
- optimizing inputs over historical windows
- driving local paper-trading sessions from the same compiled strategy

The language keeps market-data inputs and execution venues explicit:

- `source` declares market data feeds
- `execution` declares order-routing venues
- `order ...` declares how entries and exits are placed

Documentation and tooling:

- Docs: <https://palmscript.dev/docs/>
- Hosted IDE: <https://palmscript.dev/>
- Local docs source: [web/docs/docs/index.md](/mnt/4tbscratch/projects/tradelang/web/docs/docs/index.md)
- CLI reference: [web/docs/docs/reference/cli.md](/mnt/4tbscratch/projects/tradelang/web/docs/docs/reference/cli.md)

## Language Examples

Indicator-only script:

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

fast = sma(spot.close, 20)
slow = sma(spot.close, 50)

plot(fast)
plot(slow)
plot(fast - slow)
```

Trading script with explicit execution and orders:

```palmscript
interval 4h
source spot = binance.spot("BTCUSDT")
execution spot = binance.spot("BTCUSDT")

input fast_len = 21
input slow_len = 55

fast = ema(spot.close, fast_len)
slow = ema(spot.close, slow_len)

entry long = crossover(fast, slow)
exit long = crossunder(fast, slow)
entry short = false
exit short = false

order entry long = market(venue = spot)
order exit long = market(venue = spot)
order entry short = market(venue = spot)
order exit short = market(venue = spot)

plot(fast - slow)
```

More examples live under
[crates/palmscript/examples/strategies](/mnt/4tbscratch/projects/tradelang/crates/palmscript/examples/strategies).

## CLI

Build the CLI:

```bash
cargo build --bin palmscript
```

Common commands:

```bash
# Validate a script
target/debug/palmscript check crates/palmscript/examples/strategies/sma_cross.ps

# Replay market data without order simulation
target/debug/palmscript run market \
  crates/palmscript/examples/strategies/sma_cross.ps \
  --from 1704067200000 --to 1704153600000

# Run a backtest
target/debug/palmscript run backtest \
  crates/palmscript/examples/strategies/venue_orders_backtest.ps \
  --from 1704067200000 --to 1704153600000 \
  --execution-source exec \
  --maker-fee-bps 2 --taker-fee-bps 5

# Optimize inputs
target/debug/palmscript run optimize \
  crates/palmscript/examples/strategies/adaptive_trend_backtest.ps \
  --from 1646611200000 --to 1772841600000 \
  --train-bars 252 --test-bars 63 --step-bars 63 \
  --trials 50 --preset-out best.json

# Queue and drive a local paper session
target/debug/palmscript run paper \
  crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps \
  --execution-source bb \
  --maker-fee-bps 2 --taker-fee-bps 5
target/debug/palmscript execution serve

# Read embedded docs
target/debug/palmscript docs --list
target/debug/palmscript docs --all
```

For the full command surface, use the embedded help or the CLI reference:

```bash
target/debug/palmscript --help
target/debug/palmscript run --help
target/debug/palmscript execution --help
```

Containerized paper-trading assets live under
[infra/docker/Dockerfile.paper](/mnt/4tbscratch/projects/tradelang/infra/docker/Dockerfile.paper),
[infra/docker/paper-entrypoint.sh](/mnt/4tbscratch/projects/tradelang/infra/docker/paper-entrypoint.sh),
and
[infra/docker/paper-sessions.toml](/mnt/4tbscratch/projects/tradelang/infra/docker/paper-sessions.toml).
The intended layout is:

- bundled example strategies available at `/usr/share/palmscript/strategies`
- optional custom strategies mounted at `/strategies`
- persistent execution state mounted at `/var/lib/palmscript/execution`
- paper-session config mounted at `/etc/palmscript/paper-sessions.toml`

The paper container now also serves a live monitoring UI at `/paper` on port
`8080`. It lists all persisted paper sessions, shows an explicit strategy
picker plus per-strategy run selection, and polls real-time paper metrics such
as equity, PnL, open positions, trades, orders, drawdown, feed health, and
session logs.

Build and run:

```bash
docker build -f infra/docker/Dockerfile.paper -t palmscript-paper .
docker run --rm \
  -v "$(pwd)/.paper-state:/var/lib/palmscript/execution" \
  -v "$(pwd)/infra/docker/paper-sessions.toml:/etc/palmscript/paper-sessions.toml:ro" \
  -p 8080:8080 \
  palmscript-paper
```
