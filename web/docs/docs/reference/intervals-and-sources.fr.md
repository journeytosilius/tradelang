# Intervalles Et Sources

Cette page definit les regles normatives des intervalles et des sources dans
PalmScript.

## Intervalles Pris En Charge

PalmScript accepte les litteraux d'intervalle listes dans la
[Table des intervalles](intervals.md). Les intervalles sont sensibles a la
casse.

## Intervalle De Base

Chaque script declare exactement un intervalle de base :

```palmscript
interval 1m
```

L'intervalle de base definit l'horloge d'execution.

## Sources Nommees

Les scripts executables declarent une ou plusieurs sources nommees adossees a
des exchanges :

```palmscript
interval 1m
source bb = bybit.usdt_perps("BTCUSDT")
source bn = binance.spot("BTCUSDT")
use bb 1h

plot(bn.close - bb.1h.close)
```

Regles :

- au moins une declaration `source` est requise
- les series de marche doivent etre qualifiees par source
- chaque source declaree fournit un flux de base sur l'intervalle de base du
  script
- `use <alias> <interval>` declare un intervalle supplementaire pour cette
  source
- `<alias>.<field>` reference cette source sur l'intervalle de base
- `<alias>.<interval>.<field>` reference cette source sur l'intervalle nomme
- les references a un intervalle inferieur a l'intervalle de base sont
  rejetees

## Templates De Source Pris En Charge

PalmScript prend actuellement en charge ces templates de premiere classe :

- `binance.spot("<symbol>")`
- `binance.usdm("<symbol>")`
- `bybit.spot("<symbol>")`
- `bybit.usdt_perps("<symbol>")`
- `gate.spot("<symbol>")`
- `gate.usdt_perps("<symbol>")`

La prise en charge des intervalles depend du template :

- `binance.spot` accepte tous les intervalles PalmScript pris en charge
- `binance.usdm` accepte tous les intervalles PalmScript pris en charge
- `bybit.spot` accepte `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w` et `1M`
- `bybit.usdt_perps` accepte `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w` et `1M`
- `gate.spot` accepte `1s`, `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h`, `1d` et `1M`
- `gate.usdt_perps` accepte `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h` et `1d`

Les contraintes operationnelles de recuperation dependent aussi du template :

- Bybit utilise des symboles natifs du venue comme `BTCUSDT`
- Gate utilise des symboles natifs du venue comme `BTC_USDT`
- les klines REST Bybit arrivent en ordre decroissant et PalmScript les reordonne avant la validation d'alignement
- les API de chandeliers Gate utilisent des secondes Unix et PalmScript les normalise en millisecondes Unix UTC
- la pagination Gate spot et futures est decoupee par fenetres temporelles, car l'API publique n'autorise pas `limit` avec `from` / `to`
- les requetes Gate spot et futures sont limitees a 1000 chandeliers par appel HTTP afin d'eviter des `400 Bad Request` causes par des plages trop larges
- les flux Binance, Bybit et Gate sont pagines en interne
- lorsqu'une recuperation de venue echoue, PalmScript affiche l'URL de la requete et un extrait tronque du corps de reponse lorsqu'il existe, autant pour les rejets HTTP non-200 que pour les payloads JSON malformes
- les URL de base peuvent etre surchargees avec
  `PALMSCRIPT_BINANCE_SPOT_BASE_URL`, `PALMSCRIPT_BINANCE_USDM_BASE_URL`,
  `PALMSCRIPT_BYBIT_BASE_URL` et `PALMSCRIPT_GATE_BASE_URL`; pour Gate, la
  racine de l'hote, par exemple `https://api.gateio.ws`, et l'URL de base
  complete `/api/v4` sont toutes deux acceptees

## Ensemble Des Champs De Source

Tous les templates de source sont normalises vers les memes champs de marche
canoniques :

- `time`
- `open`
- `high`
- `low`
- `close`
- `volume`

Regles :

- `time` est l'heure d'ouverture de la bougie en millisecondes Unix UTC
- les champs de prix et de volume sont numeriques
- les champs supplementaires specifiques au venue ne sont pas exposes dans le
  langage

## Intervalles Egaux, Superieurs Et Inferieurs

PalmScript distingue trois cas pour un intervalle reference relativement a
l'intervalle de base :

- intervalle egal : valide
- intervalle superieur : valide s'il est declare avec `use <alias> <interval>`
- intervalle inferieur : rejete

## Semantique Runtime

En mode marche :

- PalmScript recupere directement depuis les venues les flux `(source, interval)`
  requis
- la timeline d'execution de base est l'union des heures d'ouverture des barres
  d'intervalle de base de toutes les sources declarees
- si une source n'a pas de barre de base a une etape de la timeline, cette
  source fournit `na` pour cette etape
- les intervalles de source plus lents conservent leur derniere valeur
  completement cloturee jusqu'a leur prochaine frontiere de cloture

## Garantie Sans Lookahead

PalmScript ne doit pas exposer une bougie d'intervalle superieur avant sa
cloture complete.

Cela s'applique aux intervalles qualifies source-aware comme `bb.1h.close`.

## Regles D'Alignement Runtime

Les flux prepares doivent etre alignes sur leurs intervalles declares.

Le runtime rejette les flux qui sont :

- mal alignes sur la frontiere de l'intervalle
- non tries
- dupliques pour une meme heure d'ouverture d'intervalle
