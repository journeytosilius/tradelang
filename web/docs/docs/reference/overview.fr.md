# Vue Densemble De La Reference

Cette section est la definition normative de PalmScript telle qu'elle est
documentee publiquement.

Si une page de guide et une page de reference divergent, la reference fait foi.

## Ce Que Definit Cette Section

- structure lexicale
- grammaire
- regles de declarations et de portee
- types et valeurs
- semantique des series et de l'indexation
- semantique d'evaluation
- regles des intervalles et des sources
- contrats des builtins et des indicateurs
- semantique des sorties
- classes de diagnostics

## Ce Qui Est Implemente Aujourd'hui

La surface actuelle de PalmScript comprend :

- exactement une directive de base `interval <...>` de niveau top-level par script
- un ou plusieurs alias `source` nommes par script executable
- des series qualifiees par source comme `spot.close` ou `hl.1h.close`
- des intervalles supplementaires via `use <alias> <interval>`
- des declarations `fn` top-level a corps d'expression
- `let`, `const`, `input`, la destructuration de tuples, `export`, `trigger`, les `entry` / `exit` de premiere classe et `order`
- une indexation de series uniquement litterale, des litteraux enum types `ma_type.<variant>`, `tif.<variant>`, `trigger_ref.<variant>`, `position_side.<variant>` et `exit_kind.<variant>`, ainsi qu'une logique booleenne deterministe a trois valeurs
- une surface builtin de style TA-Lib ou certains noms sont executables aujourd'hui et ou des noms reserves supplementaires sont exposes via des diagnostics

## Limites Actuelles

- `interval`, `source`, `use`, `fn`, `const`, `input`, `export`, `trigger`, `entry`, `exit` et `order` sont reserves au top-level
- les identifiants de marche nus comme `close` ne sont pas valides dans les scripts executables
- les intervalles source superieurs exigent `use <alias> <interval>`
- seuls les identifiants sont appelables
- les litteraux de chaine ne sont valides qu'a l'interieur des declarations `source`
- l'indexation de series exige un litteral entier non negatif
- les resultats de builtins a tuple doivent etre immediatement destructures avec `let (...) = ...` avant toute autre utilisation

## Comment Le Lire

- commencez par [Structure lexicale](lexical-structure.md) et [Grammaire](grammar.md) pour la syntaxe acceptee
- utilisez [Declarations et portee](declarations-and-scope.md) pour les regles de liaison et de visibilite
- utilisez [Semantique d'evaluation](evaluation-semantics.md) et [Intervalles et sources](intervals-and-sources.md) pour comprendre le sens du langage
- utilisez [Builtins](builtins.md), [Indicateurs](indicators.md) et [Sorties](outputs.md) pour le comportement des appels et des sorties
