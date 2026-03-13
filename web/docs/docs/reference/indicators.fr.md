# Vue Densemble Des Indicateurs

Cette section definit la surface d'indicateurs executable de PalmScript.

Utilisez [Builtins](builtins.md) pour les regles partagees sur les appels, les
helpers builtin, `plot` et la destructuration de tuples qui s'appliquent dans
tout le langage.

## Familles D'Indicateurs

PalmScript documente actuellement les indicateurs dans ces familles :

- [Trend and Overlap](indicators-trend-and-overlap.md)
- [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md)
- [Math, Price, and Statistics](indicators-math-price-statistics.md)

## Regles Partagees Des Indicateurs

Regles :

- les noms d'indicateurs sont des identifiants builtin, donc ils sont appeles
  directement, par exemple `ema(spot.close, 20)`
- les entrees des indicateurs doivent toujours suivre les regles de series
  qualifiees par source de [Intervalles et sources](intervals-and-sources.md)
- les arguments de longueur optionnels utilisent les valeurs TA-Lib par defaut
  documentees sur les pages de famille
- les arguments de type longueur decrits comme litteraux doivent etre des
  litteraux entiers dans le code source
- les indicateurs a valeur tuple doivent etre destructures avec `let (...) = ...`
  avant toute autre utilisation
- les sorties d'indicateurs suivent l'horloge de mise a jour impliquee par
  leurs entrees series
- les indicateurs propagent `na` sauf si le contrat specifique de
  l'indicateur indique autre chose

## Indicateurs A Valeur Tuple

Les indicateurs a valeur tuple actuels sont :

- `macd(series, fast_length, slow_length, signal_length)`
- `minmax(series[, length=30])`
- `minmaxindex(series[, length=30])`
- `aroon(high, low[, length=14])`
- `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])`
- `donchian(high, low[, length=20])`

Ils doivent etre immediatement destructures :

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let (line, signal, hist) = macd(spot.close, 12, 26, 9)
plot(line)
```

## Noms TA-Lib Executables Ou Reserves

PalmScript reserve un catalogue TA-Lib plus large que ce qu'il execute
aujourd'hui.

- ces pages d'indicateurs definissent le sous-ensemble executable
- [TA-Lib Surface](ta-lib.md) definit la surface plus large de noms reserves et
  de metadata
- un nom TA-Lib reserve mais pas encore executable produit un diagnostic de
  compilation deterministe
