# Inicio Rapido

## 1. Abra O IDE No Navegador

Use o IDE hospedado em [https://palmscript.dev/app/](https://palmscript.dev/app/).

## 2. Cole Um Script

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 5)
let slow = sma(spot.close, 10)

export trend = fast > slow
plot(spot.close)
```

## 3. Revise Os Diagnosticos

O editor valida o script enquanto voce digita e mostra qualquer diagnostico de
compilacao no painel da direita.

## 4. Rode Um Backtest

Escolha um intervalo de datas e pressione `Run Backtest` para executar o script
sobre o historico disponivel de BTCUSDT dentro da app.

Proximo: [Primeira Estrategia](first-strategy.md)
