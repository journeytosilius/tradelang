# Inicio Rapido

## 1. Abre El IDE Del Navegador

Usa el IDE alojado en [https://palmscript.dev/app/](https://palmscript.dev/app/).

## 2. Pega Un Script

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 5)
let slow = sma(spot.close, 10)

export trend = fast > slow
plot(spot.close)
```

## 3. Revisa Los Diagnosticos

El editor valida el script mientras escribes y muestra cualquier diagnostico de
compilacion en el panel de la derecha.

## 4. Ejecuta Un Backtest

Elige un rango de fechas y pulsa `Run Backtest` para ejecutar el script sobre
el historial disponible de BTCUSDT dentro de la app.

Siguiente: [Primera Estrategia](first-strategy.md)
