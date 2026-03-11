# Premiere Strategie

Cette strategie s'execute sur des bougies d'une minute, calcule deux moyennes
mobiles et transforme ce croisement en un flux simple d'entree et de sortie
long only.

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 5)
let slow = sma(spot.close, 10)

export trend = fast > slow
entry long = crossover(fast, slow)
exit long = crossunder(fast, slow)

order entry long = market()
```

## Ce Que Cela Introduit

- `interval 1m` fixe l'horloge d'execution de base
- `source spot = ...` relie un marche adosse a un exchange
- `spot.close` est une serie de base qualifiee par source
- `let` lie des expressions reutilisables
- `export` publie une serie de sortie nommee
- `entry long = ...` emet un signal d'entree long
- `exit long = ...` emet un signal de sortie long
- `order entry long = market()` indique au backtester comment executer le signal d'entree

## L'Essayer Dans L'IDE Navigateur

Ouvrez [https://palmscript.dev/app/](https://palmscript.dev/app/), collez le
script dans l'editeur et executez-le sur l'historique BTCUSDT disponible avec
les controles de date de l'en-tete. Vous devriez voir le panneau de diagnostics
rester vide, puis le resume du backtest, les trades et les orders se remplir a
partir des signaux de croisement.

## L'Etendre Avec Un Contexte De Timeframe Superieur

```palmscript
interval 1d
source spot = binance.spot("BTCUSDT")
use spot 1w

let weekly_basis = ema(spot.1w.close, 8)
export bullish = spot.close > weekly_basis
entry long = bullish and crossover(spot.close, weekly_basis)
exit long = crossunder(spot.close, weekly_basis)
order entry long = market()
```

Pour les regles exactes derriere `spot.1w.close`, les signaux de premiere
classe `entry` / `exit`, l'indexation et le comportement sans lookahead, voir :

- [Series et indexation](../reference/series-and-indexing.md)
- [Intervalles et sources](../reference/intervals-and-sources.md)
- [Sorties](../reference/outputs.md)
- [Semantique d'evaluation](../reference/evaluation-semantics.md)
