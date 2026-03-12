# Structure Lexicale

Le code source PalmScript est tokenise avant le parsing. Le lexer doit
preserver l'ordre source et les spans, et il doit rejeter les caracteres
inconnus ainsi que les litteraux d'intervalle invalides.

## Mots-Cles

Les tokens suivants sont des mots-cles reserves :

- `fn`
- `let`
- `interval`
- `source`
- `use`
- `const`
- `input`
- `export`
- `regime`
- `trigger`
- `entry`
- `exit`
- `protect`
- `target`
- `order`
- `if`
- `else`
- `and`
- `or`
- `true`
- `false`
- `na`

Ces mots-cles ne doivent pas etre utilises la ou un identifiant est requis.

## Identifiants

Un identifiant :

- doit commencer par une lettre ASCII ou `_`
- peut ensuite contenir des lettres ASCII, des chiffres ou `_`

Exemples :

- `trend`
- `_tmp1`
- `weekly_basis`

## Litteraux

### Litteraux Numeriques

Les litteraux numeriques sont analyses comme des `f64`.

Formes acceptees :

- `1`
- `14`
- `3.5`

Formes rejetees :

- la notation exponentielle comme `1e6`
- les formes a point initial comme `.5`

Les nombres negatifs s'expriment avec l'operateur unaire `-`, pas avec un
token de litteral signe distinct.

### Litteraux Booleens

Les litteraux booleens sont :

- `true`
- `false`

### Litteral De Valeur Manquante

`na` est le litteral de valeur manquante.

### Litteraux De Chaine

Les litteraux de chaine sont actuellement acceptes uniquement la ou la
grammaire les autorise dans les declarations `source`.

Ils :

- sont delimites par `"`
- peuvent contenir les echappements de base pour `"`, `\\`, `\\n`, `\\r`, `\\t`
- ne doivent pas traverser une nouvelle ligne non echappee

## Commentaires

Seuls les commentaires sur une ligne sont pris en charge :

```palmscript
// regime de tendance
let fast = ema(spot.close, 5)
```

Un token `/` isole est l'operateur arithmetique de division. `//` demarre un
commentaire sur une seule ligne.

## Separateurs D'Instructions

Les instructions sont separees par :

- les nouvelles lignes
- les points-virgules

Les nouvelles lignes a l'interieur de parentheses ou de crochets ne terminent
pas une instruction.

## Litteraux D'Intervalle

Les litteraux d'intervalle sont sensibles a la casse. L'ensemble accepte est
defini dans [Table des intervalles](intervals.md).

Par exemple :

- `1w` est valide
- `1M` est valide
- `1W` est invalide

## Note Sur `optimize`

`optimize` est maintenant un mot cle reserve. Il sert de suffixe de metadonnees dans `input ... optimize(...)` et ne peut plus etre reutilise comme identifiant ordinaire.
