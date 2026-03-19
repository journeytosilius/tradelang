# Vue Densemble Du Langage

Les scripts PalmScript sont des fichiers source de haut niveau composes de
declarations et d'instructions.

Blocs courants :

- `interval <...>` pour l'horloge d'execution de base
- des declarations `source` pour les series adossees au marche
- des declarations facultatives `use <alias> <interval>` pour les intervalles supplementaires
- des fonctions de haut niveau
- `let`, `const`, `input`, la destructuration de tuples, `export`, `regime`, `trigger`, `entry` / `exit` et `order`
- des controles declaratifs de backtest comme `cooldown long = 12` et `max_bars_in_trade short = 48`
- `if / else if / else`
- des expressions construites avec des operateurs, des appels et de l'indexation
- des builtins auxiliaires comme `crossover`, `state`, `activated`, `barssince` et `valuewhen`
- des litteraux enum typés `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>` et `exit_kind.<variant>`

## Forme Du Script

Les scripts PalmScript executables nomment explicitement leurs sources de donnees :

```palmscript
interval 1m
source bn = binance.spot("BTCUSDT")
source bb = bybit.usdt_perps("BTCUSDT")

plot(bn.close - bb.close)
```

## Modele Mental

- chaque script a un intervalle de base
- les scripts executables declarent une ou plusieurs liaisons `source`
- les series de marche sont toujours qualifiees par source
- les valeurs de serie evoluent dans le temps
- les intervalles superieurs ne se mettent a jour que lorsque leurs bougies se ferment completement
- l'historique manquant ou les donnees source non alignees apparaissent comme `na`
- `plot`, `export`, `regime`, `trigger` et les declarations de strategie emettent des resultats apres chaque etape d'execution
- `cooldown` et `max_bars_in_trade` sont des declarations de nombre de barres en compilation pour expliciter la re-entree et les sorties temporelles

## Ou Aller Pour Les Regles Exactes

- syntaxe et tokens : [Structure lexicale](../reference/lexical-structure.md) et [Grammaire](../reference/grammar.md)
- declarations et visibilite : [Declarations et portee](../reference/declarations-and-scope.md)
- expressions et semantique : [Semantique d'evaluation](../reference/evaluation-semantics.md)
- regles des series de marche : [Intervalles et sources](../reference/intervals-and-sources.md)
- indicateurs et builtins auxiliaires : [Indicateurs](../reference/indicators.md) et [Builtins](../reference/builtins.md)
- sorties : [Sorties](../reference/outputs.md)

## Metadonnees D Optimisation

Les `input` numeriques peuvent maintenant declarer des metadonnees de recherche directement dans le script :

```palmscript
input fast_len = 21 optimize(int, 8, 34, 1)
```

Ainsi, `run optimize` peuvent inferer l espace de recherche depuis le script lui-meme quand `--param` n est pas fourni.

## Latest Portfolio Additions

- PalmScript now reserves `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, and `portfolio_group`.
- These declarations are top-level only and compile-time only.
- Portfolio mode activates when backtest-oriented CLI commands receive repeated `--execution-source` flags.
- Portfolio mode shares one equity ledger across the selected aliases and blocks only the new entries that would exceed the configured caps.

## Latest Execution Additions

- PalmScript now supports separate top-level `execution` declarations for order-routing targets.
- `source` stays the market-data surface, while `execution` declares where orders are intended to route.
- Order declarations can target a declared execution alias expression with named arguments such as `venue = exec` or `venue = current_execution()`.
