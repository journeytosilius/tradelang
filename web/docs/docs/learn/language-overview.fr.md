# Vue Densemble Du Langage

Les scripts PalmScript sont des fichiers source de haut niveau composes de
declarations et d'instructions.

Blocs courants :

- `interval <...>` pour l'horloge d'execution de base
- des declarations `source` pour les series adossees au marche
- des declarations facultatives `use <alias> <interval>` pour les intervalles supplementaires
- des fonctions de haut niveau
- `let`, `const`, `input`, la destructuration de tuples, `export`, `trigger`, `entry` / `exit` et `order`
- `if / else if / else`
- des expressions construites avec des operateurs, des appels et de l'indexation
- des builtins auxiliaires comme `crossover`, `activated`, `barssince` et `valuewhen`
- des litteraux enum typés `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>` et `exit_kind.<variant>`

## Forme Du Script

Les scripts PalmScript executables nomment explicitement leurs sources de donnees :

```palmscript
interval 1m
source bn = binance.spot("BTCUSDT")
source hl = hyperliquid.perps("BTC")

plot(bn.close - hl.close)
```

## Modele Mental

- chaque script a un intervalle de base
- les scripts executables declarent une ou plusieurs liaisons `source`
- les series de marche sont toujours qualifiees par source
- les valeurs de serie evoluent dans le temps
- les intervalles superieurs ne se mettent a jour que lorsque leurs bougies se ferment completement
- l'historique manquant ou les donnees source non alignees apparaissent comme `na`
- `plot`, `export`, `trigger` et les declarations de strategie emettent des resultats apres chaque etape d'execution

## Ou Aller Pour Les Regles Exactes

- syntaxe et tokens : [Structure lexicale](../reference/lexical-structure.md) et [Grammaire](../reference/grammar.md)
- declarations et visibilite : [Declarations et portee](../reference/declarations-and-scope.md)
- expressions et semantique : [Semantique d'evaluation](../reference/evaluation-semantics.md)
- regles des series de marche : [Intervalles et sources](../reference/intervals-and-sources.md)
- indicateurs et builtins auxiliaires : [Indicateurs](../reference/indicators.md) et [Builtins](../reference/builtins.md)
- sorties : [Sorties](../reference/outputs.md)
