# Documentation PalmScript

PalmScript est un langage pour les strategies financieres de series temporelles.
Ce site se concentre sur le langage lui-meme : syntaxe, semantique, builtins et
exemples de code.

## Plan De La Documentation

- `Apprendre` enseigne le langage avec de courts exemples et des parcours executables.
- `Reference` definit la syntaxe acceptee et la semantique du langage.

## Commencer Ici

- Nouveau sur PalmScript : [Vue densemble d'Apprendre](learn/overview.md)
- Vous voulez un premier script executable : [Demarrage Rapide](learn/quickstart.md)
- Vous avez besoin de la definition formelle du langage : [Vue densemble de la Reference](reference/overview.md)
- Vous cherchez les contrats des indicateurs : [Vue densemble des indicateurs](reference/indicators.md)

La demo IDE hebergee garde une interface minimale : un seul editeur, une coque
React et TypeScript avec Monaco, des selecteurs de plage de dates sur
l'historique BTCUSDT disponible, des diagnostics en direct, des snippets
d'autocompletion pour les callables, des panneaux de backtest et des tableaux
trades/orders sans colonne JSON brute. La barre d'outils conserve le logo
PalmScript dans l'en-tete ainsi qu'un commutateur clair/sombre. Le mode sombre
utilise une coque inspiree de VS Code avec un theme d'editeur de style Dracula.
Le point d'entree heberge est `/app/`.
[https://palmscript.dev/app](https://palmscript.dev/app) y redirige.

## Points Forts Du Langage

PalmScript prend en charge :

- une declaration de base obligatoire `interval <...>`
- des declarations `source` nommees pour les donnees de marche
- des series qualifiees par source comme `spot.close` et `perp.1h.close`
- des declarations facultatives `use <alias> <interval>` pour les intervalles supplementaires
- les litteraux, l'arithmetique, les comparaisons, les operateurs unaires, `and` et `or`
- `let`, `const`, `input`, la destructuration de tuples, `export` et `trigger`
- `if / else if / else`
- l'indexation de series avec des offsets litteraux
- des indicateurs, des helpers de signaux, des helpers de memoire d'evenements et des builtins de style TA-Lib
- des declarations de strategie de premiere classe comme `entry`, `exit`, `order`, `protect` et `target`

## Comment Lire La Documentation

Commencez par `Apprendre` si vous ecrivez PalmScript pour la premiere fois.

Utilisez `Reference` lorsque vous avez besoin de regles exactes sur la syntaxe,
la semantique, les builtins, les intervalles ou les sorties.

Le titre d'en-tete reste `PalmScript` pendant le defilement et renvoie vers la
page d'accueil du site principal.
