# Sorties

Cette page definit les formes de sortie visibles par l'utilisateur dans
PalmScript.

## Formes De Sortie

PalmScript expose trois constructions productrices de sortie :

- `plot(value)`
- `export name = expr`
- `regime name = expr`
- `trigger name = expr`
- `entry long = expr`, `entry1 long = expr`, `entry2 long = expr`,
  `entry3 long = expr`
- `entry short = expr`, `entry1 short = expr`, `entry2 short = expr`,
  `entry3 short = expr`
- `exit long = expr`, `exit short = expr`
- `protect long = order_spec`, `protect short = order_spec`
- `protect_after_target1 long = order_spec`,
  `protect_after_target2 long = order_spec`,
  `protect_after_target3 long = order_spec`
- `protect_after_target1 short = order_spec`,
  `protect_after_target2 short = order_spec`,
  `protect_after_target3 short = order_spec`
- `target long = order_spec`, `target1 long = order_spec`,
  `target2 long = order_spec`, `target3 long = order_spec`
- `target short = order_spec`, `target1 short = order_spec`,
  `target2 short = order_spec`, `target3 short = order_spec`
- `size entry long = expr`, `size entry1 long = expr`,
  `size entry2 long = expr`, `size entry3 long = expr`
- `size entry short = expr`, `size entry1 short = expr`,
  `size entry2 short = expr`, `size entry3 short = expr`
- `size target long = expr`, `size target1 long = expr`,
  `size target2 long = expr`, `size target3 long = expr`
- `size target short = expr`, `size target1 short = expr`,
  `size target2 short = expr`, `size target3 short = expr`

`plot` est un appel builtin. `export`, `regime` et `trigger` sont des declarations.

## `plot`

`plot` emet un point de trace pour l'etape courante.

Regles :

- l'argument doit etre numerique, `series<float>` ou `na`
- l'etape courante produit un point de trace par appel `plot` execute
- `plot` ne cree pas de liaison reutilisable dans le langage
- `plot` n'est pas autorise dans le corps des fonctions definies par
  l'utilisateur

## `export`

`export` publie une serie de sortie nommee :

```palmscript
export trend = ema(spot.close, 20) > ema(spot.close, 50)
```

Regles :

- reserve au top-level
- le nom doit etre unique dans la portee courante
- l'expression peut s'evaluer en numerique, bool, serie numerique, serie bool
  ou `na`
- `void` est rejete

Normalisation des types :

- les `export` numeriques, series numeriques et `na` deviennent
  `series<float>`
- les `export` bool et series bool deviennent `series<bool>`

## `regime`

`regime` publie une serie booleenne persistante d'etat de marche avec nom :

```palmscript
regime trend_long = state(
    ema(spot.close, 20) > ema(spot.close, 50),
    ema(spot.close, 20) < ema(spot.close, 50)
)
```

Regles :

- reserve au top-level
- l'expression doit s'evaluer en `bool`, `series<bool>` ou `na`
- le type de sortie est toujours `series<bool>`
- les noms `regime` deviennent des liaisons reutilisables apres leur declaration
- `regime` est pense pour fonctionner avec `state(...)`, `activated(...)` et `deactivated(...)`
- les diagnostics runtime l'enregistrent avec les series exportees ordinaires

## `trigger`

`trigger` publie une serie de sortie booleenne nommee :

```palmscript
trigger long_entry = spot.close > spot.high[1]
```

Regles :

- reserve au top-level
- l'expression doit s'evaluer en `bool`, `series<bool>` ou `na`
- le type de sortie est toujours `series<bool>`

Regle d'evenement runtime :

- un evenement trigger n'est emis pour une etape que lorsque l'echantillon
  courant du trigger vaut `true`
- `false` et `na` n'emettent pas d'evenement trigger

## Signaux De Strategie De Premiere Classe

PalmScript expose des declarations de signaux de strategie de premiere classe
pour l'execution orientee strategie :

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)
entry short = spot.close < spot.low[1]
exit short = spot.close > ema(spot.close, 20)
```

Regles :

- les quatre declarations sont reservees au top-level
- chaque expression doit s'evaluer en `bool`, `series<bool>` ou `na`
- elles sont compilees en sorties trigger avec des metadata explicites de role
  de signal
- l'emission d'evenements runtime suit les memes regles `true` / `false` /
  `na` que les triggers ordinaires
- `entry long` et `entry short` sont des alias de compatibilite pour
  `entry1 long` et `entry1 short`
- `entry2` et `entry3` sont des signaux d'ajout sequentiels du meme cote, qui
  ne deviennent eligibles qu'apres le remplissage de l'etape precedente dans le
  cycle de position courant

## Declarations `order`

PalmScript expose aussi des declarations `order` top-level qui parametrent la
maniere dont un role de signal est execute :

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)

order entry long = limit(spot.close[1], tif.gtc, false)
order exit long = stop_market(lowest(spot.low, 5)[1], trigger_ref.last)
```

Regles :

- les declarations `order` sont reservees au top-level
- il peut y avoir au plus une declaration `order` par role de signal
- en l'absence de declaration `order`, `market()` est utilise par defaut
- les champs d'ordre numeriques comme `price`, `trigger_price` et
  `expire_time_ms` sont evalues par le runtime comme des series internes
  cachees
- `tif.<variant>` et `trigger_ref.<variant>` sont des litteraux enum types
  verifies a la compilation
- les verifications de compatibilite specifiques au venue sont executees au
  demarrage du backtest, selon la `source` d'execution

## Sorties Attachees

PalmScript expose aussi des sorties attachees de premiere classe afin de
laisser libre le signal discretionnaire `exit` :

```palmscript
entry long = spot.close > spot.high[1]
exit long = spot.close < ema(spot.close, 20)
protect long = stop_market(position.entry_price - 2 * atr(spot.high, spot.low, spot.close, 14), trigger_ref.last)
target long = take_profit_market(
    highest_since(position_event.long_entry_fill, spot.high) + 4,
    trigger_ref.last
)
size target long = 0.5
```

Regles :

- les sorties attachees sont reservees au top-level
- `protect` est l'etape de protection de base pour un cote
- `protect_after_target1`, `protect_after_target2` et `protect_after_target3`
  peuvent reajuster l'ordre protect actif apres chaque remplissage de target
  echelonne
- `target`, `target1`, `target2` et `target3` sont des etapes sequentielles de
  prise de profit attachee ; `target` est un alias de compatibilite pour
  `target1`
- `size entry1..3` et `size target1..3` sont facultatifs et ne s'appliquent
  qu'a l'entree ou au target echelonne correspondant
- le dimensionnement des entrees echelonnees prend en charge :
  - une fraction numerique nue historique telle que `0.5`
  - `capital_fraction(x)`
  - `risk_pct(pct, stop_price)`
- les valeurs `capital_fraction(...)` doivent s'evaluer en une fraction finie
  dans `(0, 1]`
- une fraction de taille d'entree inferieure a `1` laisse du cash pour des
  scale-ins ulterieurs du meme cote
- `risk_pct(...)` est reserve aux entrees dans la v1 et dimensionne a partir du
  prix de remplissage reel et de la distance au stop au moment du remplissage
- si une taille `risk_pct(...)` demande plus que le cash courant ou le
  collateral libre disponible, le backtester borne le remplissage et enregistre
  `capital_limited = true`
- elles ne s'arment qu'apres un remplissage d'entree correspondant
- elles sont reevaluees une fois par barre d'execution tant que la position
  reste ouverte
- seule la protect echelonnee courante et la prochaine target echelonnee sont
  actives en meme temps
- lorsque `target1` est rempli, le moteur bascule de `protect` vers
  `protect_after_target1` si cette derniere est declaree ; sinon il herite de
  la derniere etape protect disponible
- les fractions de taille des targets echelonnes doivent s'evaluer en une
  fraction finie dans `(0, 1]`
- une declaration `size targetN ...` transforme l'etape de target
  correspondante en prise de profit partielle lorsque la fraction est inferieure
  a `1`
- les targets echelonnes sont a usage unique dans un cycle de position et
  s'activent sequentiellement
- si les deux deviennent remplissables sur la meme barre d'execution, `protect`
  l'emporte de maniere deterministe
- `position.*` n'est disponible que dans les declarations `protect` et
  `target`
- `position_event.*` est un espace de noms de series pilote par le backtest qui
  expose les vrais evenements de remplissage comme `position_event.long_entry_fill`
- `position_event.*` expose aussi des evenements de remplissage propres au type
  de sortie, comme `position_event.long_target_fill`,
  `position_event.long_protect_fill` et
  `position_event.long_liquidation_fill`
- les evenements de remplissage echelonnes sont aussi disponibles, notamment
  `position_event.long_entry1_fill`, `position_event.long_entry2_fill`,
  `position_event.long_entry3_fill`, `position_event.long_target1_fill`,
  `position_event.long_target2_fill` et `position_event.long_target3_fill`,
  ainsi que leurs equivalents short
- `last_exit.*`, `last_long_exit.*` et `last_short_exit.*` exposent le snapshot
  du trade cloture le plus recent, globalement ou par cote
- `last_*_exit.kind` se compare a des litteraux enum types comme
  `exit_kind.target` et `exit_kind.liquidation`
- `last_*_exit.stage` expose le numero d'etape du target / protect echelonne
  lorsque c'est applicable
- hors backtest, `position_event.*` est defini mais s'evalue a `false` a
  chaque etape
- hors backtest, `last_*_exit.*` est defini mais s'evalue a `na`

## Compatibilite Legacy Des Triggers

Les scripts de strategie historiques qui utilisent des noms de trigger restent
temporairement pris en charge :

- `trigger long_entry = ...`
- `trigger long_exit = ...`
- `trigger short_entry = ...`
- `trigger short_exit = ...`

Regles de compatibilite :

- si un script declare des signaux `entry` / `exit` de premiere classe, le
  backtester utilise directement ces roles
- si un script ne declare aucun signal de premiere classe, le backtester se
  replie sur les noms de trigger historiques ci-dessus
- les declarations `trigger` ordinaires restent valides pour l'alerting ou des
  consommateurs non strategiques

## Collections De Sorties Runtime

Sur une execution complete, le runtime accumule :

- `plots`
- `exports`
- `triggers`
- `order_fields`
- `trigger_events`
- `alerts`

`alerts` existent actuellement dans les structures de sortie runtime mais ne
sont pas produits par une construction de langage PalmScript de premiere
classe.

## Temps De Sortie Et Indice De Barre

Chaque echantillon de sortie est etiquete avec :

- le `bar_index` courant
- le `time` de l'etape courante

Dans les executions source-aware, le temps de l'etape est l'heure d'ouverture
du pas courant de l'horloge de base.

## Latest Diagnostics Additions

PalmScript now exposes richer machine-readable backtest diagnostics in every public locale build:

- `run backtest`, `run walk-forward`, and `run optimize` accept `--diagnostics summary|full-trace`
- summary mode keeps cohort, drawdown-path, source-alignment, holdout-drift, robustness, and hint data
- full-trace mode adds one typed per-bar decision trace per execution bar
- optimize output now includes top-candidate holdout checks plus parameter stability summaries

## Latest Execution Additions

- `execution` declarations now separate execution routing from market-data `source` bindings.
- Order constructors accept named arguments in addition to the legacy positional form.
- `venue = <execution_alias>` binds an `order`, `protect`, or `target` role to a declared execution alias.
- Named order arguments cannot be mixed with positional arguments in the same constructor call.
- Execution-oriented CLI modes now require at least one declared `execution` target.
