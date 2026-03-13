# Declarations Et Portee

Cette page definit les formes de liaison acceptees par PalmScript ainsi que les
regles de visibilite qui leur sont attachees.

## Formes Reservees Au Top-Level

Les formes suivantes ne doivent apparaitre qu'au top-level d'un script :

- `interval`
- `source`
- `use`
- `fn`
- `const`
- `input`
- `export`
- `regime`
- `trigger`
- `cooldown`
- `max_bars_in_trade`
- `entry`
- `exit`
- `protect`
- `target`

Les instructions top-level `let`, `if` et les instructions d'expression sont
autorisees.

## Intervalle De Base

Chaque script doit declarer exactement un intervalle de base :

```palmscript
interval 1m
```

Le compilateur rejette un script sans `interval` de base ou avec plus d'un
`interval` de base.

## Declarations `source`

Une declaration `source` prend cette forme :

```palmscript
source bb = bybit.usdt_perps("BTCUSDT")
```

Regles :

- l'alias doit etre un identifiant
- l'alias doit etre unique parmi toutes les sources declarees
- le template doit se resoudre vers un template de source pris en charge
- l'argument symbole doit etre un litteral de chaine

## Declarations `use`

Les intervalles supplementaires sont declares par source :

```palmscript
use bb 1h
```

Regles :

- l'alias doit nommer une source declaree
- l'intervalle ne doit pas etre inferieur a l'intervalle de base
- les declarations `use <alias> <interval>` dupliquees sont rejetees
- un intervalle egal a l'intervalle de base est accepte mais redondant

## Fonctions

Les fonctions definies par l'utilisateur sont des declarations top-level a
corps d'expression :

```palmscript
fn cross_signal(a, b) = a > b and a[1] <= b[1]
```

Regles :

- les noms de fonction doivent etre uniques
- un nom de fonction ne doit pas entrer en collision avec un nom builtin
- les noms de parametres au sein d'une fonction doivent etre uniques
- les graphes de fonctions recursifs ou cycliques sont rejetes
- les corps de fonction peuvent referencer leurs parametres, les series de
  source declarees, et les liaisons immuables top-level `const` / `input`
- les corps de fonction ne doivent pas appeler `plot`
- les corps de fonction ne doivent pas capturer des liaisons `let` depuis des
  portees d'instruction englobantes

Les fonctions sont specialisees selon le type des arguments et l'horloge de
mise a jour.

## Liaisons `let`

`let` cree une liaison dans la portee de bloc courante :

```palmscript
let basis = ema(spot.close, 20)
```

Regles :

- un `let` duplique dans la meme portee est rejete
- les portees internes peuvent masquer des liaisons externes
- la valeur liee peut etre scalaire ou serie
- `na` est autorise et traite comme un substitut de type numerique pendant la
  compilation

PalmScript prend aussi en charge la destructuration de tuples pour les
resultats immediats de builtin a valeur tuple :

```palmscript
let (line, signal, hist) = macd(spot.close, 12, 26, 9)
```

Regles supplementaires :

- la destructuration de tuples est une forme `let` de premiere classe
- le cote droit doit actuellement etre un resultat immediat de builtin a valeur
  tuple
- l'arite du tuple doit correspondre exactement
- les expressions a valeur tuple doivent etre destructurees avant toute autre
  utilisation

## `const` Et `input`

PalmScript prend en charge des liaisons immuables top-level pour la
configuration des strategies :

```palmscript
input fast_len = 21
const neutral_rsi = 50
```

Regles :

- les deux formes sont reservees au top-level
- les noms dupliques dans la meme portee sont rejetes
- dans la v1, les deux formes sont limitees aux scalaires : `float`, `bool`,
  `ma_type`, `tif`, `trigger_ref`, `position_side`, `exit_kind` ou `na`
- dans la v1, `input` n'existe qu'a la compilation
- les valeurs `input` doivent etre des litteraux scalaires ou des litteraux
  enum
- les valeurs `const` peuvent referencer des liaisons `const` / `input`
  declarees auparavant et des builtins scalaires purs
- les builtins a fenetre et l'indexation de series acceptent des liaisons
  numeriques immuables partout ou un litteral entier est requis

## Sorties

`export`, `regime`, `trigger`, les signaux de strategie de premiere classe et les
declarations de backtest orientees ordre sont reserves au top-level :

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
regime trend_long = state(ema(spot.close, 20) > ema(spot.close, 50), ema(spot.close, 20) < ema(spot.close, 50))
trigger long_entry = spot.close > spot.high[1]
entry1 long = spot.close > spot.high[1]
entry2 long = crossover(spot.close, ema(spot.close, 20))
order entry1 long = limit(spot.close[1], tif.gtc, false)
protect long = stop_market(position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref.last)
protect_after_target1 long = stop_market(position.entry_price, trigger_ref.last)
target1 long = take_profit_market(position.entry_price + 4, trigger_ref.last)
target2 long = take_profit_market(position.entry_price + 8, trigger_ref.last)
size entry1 long = 0.5
size entry2 long = 0.5
size entry3 long = risk_pct(0.01, stop_price)
size target1 long = 0.5
```

Regles :

- toutes les formes sont reservees au top-level
- les noms dupliques dans la meme portee sont rejetes
- `regime` exige `bool`, `series<bool>` ou `na` et sert aux series persistantes d'etat de marche
- les noms `regime` deviennent des liaisons apres leur point de declaration et sont enregistres avec les diagnostics exportes ordinaires
- les noms `trigger` deviennent des liaisons apres leur point de declaration
- `entry long` et `entry short` sont des alias de compatibilite pour
  `entry1 long` et `entry1 short`
- `entry1`, `entry2` et `entry3` sont des declarations de signaux d'entree
  echelonnes pour le backtest
- `exit long` et `exit short` restent des sorties discretionnaires completes
  sur une seule etape
- `cooldown long|short = <bars>` bloque les nouvelles entrees du meme cote
  pendant les `<bars>` barres d execution suivantes apres une cloture complete
  de ce cote
- `max_bars_in_trade long|short = <bars>` force une sortie market du meme cote
  a la prochaine ouverture d execution quand la position a ete tenue pendant
  `<bars>` barres d execution
- ces deux controles declaratifs exigent en v1 une expression scalaire entiere
  non negative resolue a la compilation
- `order entry ...` et `order exit ...` attachent un template d'execution au
  role de signal correspondant
- `protect`, `protect_after_target1..3` et `target1..3` declarent des sorties
  attachees echelonnees qui ne s'arment que tant que la position correspondante
  est ouverte
- `size entry1..3 long|short` permet de dimensionner facultativement une entree
  echelonnee avec `capital_fraction(x)` / la semantique historique par fraction
  numerique nue, ou `risk_pct(pct, stop_price)` pour un dimensionnement base
  sur le risque
- `size target1..3 long|short` permet de dimensionner facultativement un
  remplissage `target` echelonne comme fraction de la position ouverte
- une seule declaration `order` est autorisee par role de signal
- une seule declaration est autorisee par role echelonne
- si un role de signal n'a pas de declaration `order` explicite, le backtester
  utilise implicitement `market()`
- `size entry ...` et `size target ...` exigent chacun une declaration
  `order ...` echelonnee ou `target ...` attachee correspondante pour le meme
  role
- `risk_pct(...)` n'est valide que sur les declarations de taille d'entree
  echelonnee dans la v1
- les sorties attachees echelonnees sont sequentielles : seule la prochaine
  etape de target et la protect courante sont actives en meme temps
- `position.*` n'est disponible que dans les declarations `protect` et
  `target`
- `position_event.*` est disponible partout ou un `series<bool>` est valide et
  sert a ancrer la logique sur les vrais remplissages du backtest
- les champs `position_event` actuels sont :
  `long_entry_fill`, `short_entry_fill`, `long_exit_fill`, `short_exit_fill`,
  `long_protect_fill`, `short_protect_fill`, `long_target_fill`,
  `short_target_fill`, `long_signal_exit_fill`, `short_signal_exit_fill`,
  `long_reversal_exit_fill`, `short_reversal_exit_fill`,
  `long_liquidation_fill` et `short_liquidation_fill`
- des champs de remplissage echelonnes sont aussi disponibles :
  `long_entry1_fill` .. `long_entry3_fill`, `short_entry1_fill` ..
  `short_entry3_fill`, `long_target1_fill` .. `long_target3_fill` et
  `short_target1_fill` .. `short_target3_fill`
- `last_exit.*`, `last_long_exit.*` et `last_short_exit.*` sont disponibles
  partout ou des expressions ordinaires sont valides
- les champs `last_*_exit` actuels sont `kind`, `stage`, `side`, `price`,
  `time`, `bar_index`, `realized_pnl`, `realized_return` et `bars_held`
- `last_*_exit.kind` inclut `exit_kind.liquidation` en plus des types de
  sortie existants
- les scripts historiques de type `trigger long_entry = ...` restent pris en
  charge comme pont de compatibilite lorsqu'aucun signal de premiere classe
  n'est present

## Portee Conditionnelle

`if` introduit deux portees enfants :

```palmscript
if spot.close > spot.open {
    let x = 1
} else {
    let x = 0
}
```

Regles :

- la condition doit s'evaluer en `bool`, `series<bool>` ou `na`
- les deux branches ont des portees independantes
- les liaisons creees dans une branche ne sont pas visibles en dehors du `if`

## Metadonnees D Optimisation Sur `input`

Les `input` numeriques peuvent declarer directement des metadonnees de recherche :

```palmscript
input fast_len = 21 optimize(int, 8, 34, 1)
input atr_mult = 2.5 optimize(float, 1.5, 4.0, 0.25)
input weekly_bias = 21 optimize(choice, 13, 21, 34)
```

Regles :

- `optimize(int, low, high[, step])` exige une valeur par defaut entiere dans la plage inclusive et alignee sur le pas
- `optimize(float, low, high[, step])` exige une valeur par defaut finie dans la plage inclusive
- `optimize(choice, v1, v2, ...)` exige que la valeur par defaut soit l une des valeurs numeriques listees
- ces metadonnees decrivent seulement l espace de recherche de l optimiseur ; elles ne changent pas la valeur compilee du `input`

## Latest Portfolio Additions

- PalmScript now reserves `max_positions`, `max_long_positions`, `max_short_positions`, `max_gross_exposure_pct`, `max_net_exposure_pct`, and `portfolio_group`.
- These declarations are top-level only and compile-time only.
- Portfolio mode activates when backtest-oriented CLI commands receive repeated `--execution-source` flags.
- Portfolio mode shares one equity ledger across the selected aliases and blocks only the new entries that would exceed the configured caps.

## Latest Execution Additions

- PalmScript now reserves `execution` as a top-level declaration separate from `source`.
- `execution exec = bybit.usdt_perps("BTCUSDT")` declares an execution target without creating new market series.
- Matching `source` and `execution` aliases may mirror each other when the template and symbol are the same.
- Order constructors now accept named arguments, and `venue = exec` binds that order role to a declared execution alias.
- Positional and named order arguments cannot be mixed in the same order constructor call.
- Execution-oriented CLI modes now require at least one declared `execution` target.
