# Demarrage Rapide

## 1. Ouvrir L'IDE Navigateur

Utilisez l'IDE heberge sur [https://palmscript.dev/app/](https://palmscript.dev/app/).

## 2. Coller Un Script

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 5)
let slow = sma(spot.close, 10)

export trend = fast > slow
plot(spot.close)
```

## 3. Verifier Les Diagnostics

L'editeur verifie le script pendant la frappe et affiche tout diagnostic de
compilation dans le panneau de droite.

## 4. Lancer Un Backtest

Choisissez une plage de dates et appuyez sur `Run Backtest` pour executer le
script sur l'historique BTCUSDT disponible dans l'application.

Ensuite : [Premiere Strategie](first-strategy.md)
