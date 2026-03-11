# Quickstart

## 1. Open The Browser IDE

Use the hosted IDE at [https://palmscript.dev/app/](https://palmscript.dev/app/).

## 2. Paste A Script

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 5)
let slow = sma(spot.close, 10)

export trend = fast > slow
plot(spot.close)
```

## 3. Review Diagnostics

The editor checks the script as you type and shows any compile diagnostics in the right-hand panel.

## 4. Run A Backtest

Pick a date range and press `Run Backtest` to execute the script against the available BTCUSDT history in the app.

Next: [First Strategy](first-strategy.md)
