# Builtins

Cette page definit les regles builtin partagees de PalmScript et les helpers
builtin qui ne sont pas des indicateurs.

Les contrats propres aux indicateurs vivent dans la section dediee
[Indicateurs](indicators.md).

## Builtins Executables Face Aux Noms Reserves

PalmScript expose trois surfaces liees :

- les helpers builtin executables et les sorties documentes sur cette page
- les indicateurs executables documentes dans la section
  [Indicateurs](indicators.md)
- un catalogue TA-Lib reserve plus large, decrit dans [TA-Lib Surface](ta-lib.md)

Tous les noms TA-Lib reserves ne sont pas executables aujourd'hui. Les noms
reserves mais pas encore executables produisent des diagnostics de compilation
deterministes au lieu d'etre traites comme des identifiants inconnus.

## Categories De Builtins

PalmScript expose actuellement ces categories de builtin :

- indicateurs : [Trend and Overlap](indicators-trend-and-overlap.md),
  [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md)
  et [Math, Price, and Statistics](indicators-math-price-statistics.md)
- helpers relationnels : `above`, `below`, `between`, `outside`
- helpers de croisement : `cross`, `crossover`, `crossunder`
- helpers de selection de venue : `cheapest`, `richest`, `spread_bps`, `rank_asc`, `rank_desc`
- helpers de null : `na(value)`, `nz(value[, fallback])`,
  `coalesce(value, fallback)`
- helpers de serie et de fenetre : `change`, `highest`, `lowest`,
  `highestbars`, `lowestbars`, `rising`, `falling`, `cum`
- helpers de memoire d'evenement : `state`, `activated`, `deactivated`, `barssince`,
  `valuewhen`, `highest_since`, `lowest_since`, `highestbars_since`,
  `lowestbars_since`, `valuewhen_since`, `count_since`
- sorties : `plot`

Les champs de marche sont selectionnes via des series qualifiees par source
comme `spot.open`, `spot.close` ou `bb.1h.volume`. Seuls les identifiants sont
appelables ; `spot.close()` est donc rejete.

## Helpers De Selection De Venue

### `cheapest(exec_a, exec_b, ...)` et `richest(exec_a, exec_b, ...)`

Regles :

- ils exigent au moins deux alias `execution` declares
- chaque argument doit etre un `execution_alias` ou `na`
- ils comparent le close d'execution courant de chaque alias sur la barre active
- `cheapest(...)` renvoie l'alias avec le close courant le plus bas
- `richest(...)` renvoie l'alias avec le close courant le plus haut
- les alias sans close d'execution courant sur la barre active sont ignores
- si tous les alias references sont indisponibles sur la barre active, le resultat est `na`
- le type de resultat est `execution_alias`

Les resultats de selection sont faits pour une logique ulterieure sur les
aliases d'execution, comme les tests d'egalite ou les helpers de spread. Ils
ne sont pas exportables directement.

### `spread_bps(buy_exec, sell_exec)`

Regles :

- il exige exactement deux alias `execution` declares
- les deux arguments doivent etre `execution_alias` ou `na`
- il est evalue comme `((sell_close - buy_close) / buy_close) * 10000`
- si l'un des alias references n'a pas de close d'execution courant sur la barre active, le resultat est `na`
- le type de resultat est `float` ou `series<float>` selon l'horloge de mise a jour active

### `rank_asc(target_exec, exec_a, exec_b, ...)` et `rank_desc(target_exec, exec_a, exec_b, ...)`

Regles :

- elles exigent au moins trois alias `execution` declares au total : un alias cible et au moins deux alias compares
- le premier argument est l'alias cible ; les arguments restants forment l'ensemble compare
- chaque argument doit etre un `execution_alias` ou `na`
- elles classent les closes d'execution courants a l'interieur de l'ensemble compare fourni
- `rank_asc(...)` attribue le rang `1` au close courant le plus bas
- `rank_desc(...)` attribue le rang `1` au close courant le plus haut
- les egalites sont resolues de maniere deterministe par l'ordre des arguments compares
- les alias sans close d'execution courant sur la barre active sont ignores
- si l'alias cible est indisponible sur la barre active ou absent de l'ensemble classe, le resultat est `na`
- le type de resultat est `float` ou `series<float>` selon l'horloge de mise a jour active

Exemple :

```palmscript
execution bn = binance.spot("BTCUSDT")
execution gt = gate.spot("BTC_USDT")

export buy_gate = cheapest(bn, gt) == gt
export venue_spread_bps = spread_bps(cheapest(bn, gt), richest(bn, gt))
export bn_rank_desc = rank_desc(bn, bn, gt)
```

## Builtins A Valeur Tuple

Les builtins executables a valeur tuple actuels sont :

- `macd(series, fast_length, slow_length, signal_length)` documente dans
  [Trend and Overlap](indicators-trend-and-overlap.md)
- `minmax(series[, length=30])` documente dans
  [Math, Price, and Statistics](indicators-math-price-statistics.md)
- `minmaxindex(series[, length=30])` documente dans
  [Math, Price, and Statistics](indicators-math-price-statistics.md)
- `aroon(high, low[, length=14])` documente dans
  [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md)
- `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])` documente dans
  [Trend and Overlap](indicators-trend-and-overlap.md)
- `donchian(high, low[, length=20])` documente dans
  [Trend and Overlap](indicators-trend-and-overlap.md)

Tous les resultats builtin a valeur tuple doivent etre destructures
immediatement avec `let (...) = ...` avant toute autre utilisation.

## Regles Communes Des Builtins

Regles :

- tous les builtins sont deterministes
- les builtins ne doivent pas effectuer d'I/O, acceder au temps ou au reseau
- `plot` ecrit dans le flux de sortie ; tous les autres builtins sont purs
- les helpers builtin et les indicateurs propagent `na` sauf lorsqu'une regle
  plus specifique remplace ce comportement
- les resultats builtin suivent les horloges de mise a jour impliquees par
  leurs arguments de serie

## Helpers Relationnels

### `above(a, b)` et `below(a, b)`

Regles :

- les deux arguments doivent etre numeriques, `series<float>` ou `na`
- `above(a, b)` s'evalue comme `a > b`
- `below(a, b)` s'evalue comme `a < b`
- si une entree requise vaut `na`, le resultat est `na`
- si l'une des entrees est une serie, le type de resultat est `series<bool>`
- sinon le type de resultat est `bool`

### `between(x, low, high)` et `outside(x, low, high)`

Regles :

- tous les arguments doivent etre numeriques, `series<float>` ou `na`
- `between(x, low, high)` s'evalue comme `low < x and x < high`
- `outside(x, low, high)` s'evalue comme `x < low or x > high`
- si une entree requise vaut `na`, le resultat est `na`
- si un argument est une serie, le type de resultat est `series<bool>`
- sinon le type de resultat est `bool`

## Helpers De Croisement

### `crossover(a, b)`

Regles :

- les deux arguments doivent etre numeriques, `series<float>` ou `na`
- au moins un argument doit etre `series<float>`
- les arguments scalaires sont traites comme des seuils ; leur echantillon
  precedent est donc leur valeur courante
- l'expression s'evalue comme `a > b` au present et `a[1] <= b[1]` au passe
- si un echantillon courant ou precedent requis vaut `na`, le resultat est `na`
- le type de resultat est `series<bool>`

### `crossunder(a, b)`

Regles :

- les deux arguments doivent etre numeriques, `series<float>` ou `na`
- au moins un argument doit etre `series<float>`
- les arguments scalaires sont traites comme des seuils ; leur echantillon
  precedent est donc leur valeur courante
- l'expression s'evalue comme `a < b` au present et `a[1] >= b[1]` au passe
- si un echantillon courant ou precedent requis vaut `na`, le resultat est `na`
- le type de resultat est `series<bool>`

### `cross(a, b)`

Regles :

- les deux arguments suivent le meme contrat que `crossover` et `crossunder`
- l'expression s'evalue comme `crossover(a, b) or crossunder(a, b)`
- si un echantillon courant ou precedent requis vaut `na`, le resultat est `na`
- le type de resultat est `series<bool>`

## Helpers De Serie Et De Fenetre

### `change(series, length)`

Regles :

- il exige exactement deux arguments
- le premier argument doit etre `series<float>`
- le second argument doit etre un litteral entier positif
- l'expression s'evalue comme `series - series[length]`
- si l'echantillon courant ou reference vaut `na`, le resultat est `na`
- le type de resultat est `series<float>`

### `highest(series, length)` et `lowest(series, length)`

Regles :

- le premier argument doit etre `series<float>`
- le second argument doit etre un litteral entier positif
- la fenetre inclut l'echantillon courant
- si l'historique est insuffisant, le resultat est `na`
- si un echantillon requis de la fenetre vaut `na`, le resultat est `na`
- le type de resultat est `series<float>`

L'argument `length` peut etre un litteral entier positif ou une liaison
numerique immuable top-level declaree avec `const` ou `input`.

### `highestbars(series, length)` et `lowestbars(series, length)`

Regles :

- le premier argument doit etre `series<float>`
- le second argument suit la meme regle d'entier positif que `highest` /
  `lowest`
- la fenetre inclut l'echantillon courant
- le resultat est le nombre de barres ecoulees depuis l'echantillon le plus
  haut ou le plus bas dans la fenetre active
- si l'historique est insuffisant, le resultat est `na`
- si un echantillon requis de la fenetre vaut `na`, le resultat est `na`
- le type de resultat est `series<float>`

### `rising(series, length)` et `falling(series, length)`

Regles :

- le premier argument doit etre `series<float>`
- le second argument doit etre un litteral entier positif
- `rising(series, length)` signifie que l'echantillon courant est strictement
  superieur a tous les echantillons precedents dans les `length` dernieres
  barres
- `falling(series, length)` signifie que l'echantillon courant est strictement
  inferieur a tous les echantillons precedents dans les `length` dernieres
  barres
- si l'historique est insuffisant, le resultat est `na`
- si un echantillon requis vaut `na`, le resultat est `na`
- le type de resultat est `series<bool>`

### `cum(value)`

Regles :

- il exige exactement un argument numerique ou `series<float>`
- il renvoie la somme cumulative glissante sur l'horloge de mise a jour de
  l'argument
- si l'echantillon d'entree courant vaut `na`, l'echantillon de sortie courant
  vaut `na`
- les echantillons non-`na` ulterieurs continuent l'accumulation a partir du
  total courant precedent
- le type de resultat est `series<float>`

## Helpers De Null

### `na(value)`

Regles :

- il exige exactement un argument
- il renvoie `true` lorsque l'echantillon courant de l'argument vaut `na`
- il renvoie `false` lorsque l'echantillon courant de l'argument est une valeur
  scalaire concrete
- si l'argument est adosse a une serie, le type de resultat est `series<bool>`
- sinon le type de resultat est `bool`

### `nz(value[, fallback])`

Regles :

- il accepte un ou deux arguments
- avec un argument, les entrees numeriques utilisent `0` et les entrees
  booleennes utilisent `false` comme fallback
- avec deux arguments, le second est renvoye quand le premier vaut `na`
- les deux arguments doivent etre compatibles en type numerique ou booleen
- le type de resultat suit le type leve des operandes

### `coalesce(value, fallback)`

Regles :

- il exige exactement deux arguments
- il renvoie le premier argument lorsqu'il n'est pas `na`
- sinon il renvoie le second argument
- les deux arguments doivent etre compatibles en type numerique ou booleen
- le type de resultat suit le type leve des operandes

## Helpers De Memoire D'Evenement

### `activated(condition)` et `deactivated(condition)`

Regles :

- chacun exige exactement un argument
- l'argument doit etre `series<bool>`
- `activated` renvoie `true` lorsque l'echantillon courant de la condition vaut
  `true` et que l'echantillon precedent valait `false` ou `na`
- `deactivated` renvoie `true` lorsque l'echantillon courant de la condition
  vaut `false` et que l'echantillon precedent valait `true`
- si l'echantillon courant vaut `na`, les deux helpers renvoient `false`
- le type de resultat est `series<bool>`

### `state(enter, exit)`

Regles :

- il exige exactement deux arguments
- les deux arguments doivent etre `series<bool>`
- il renvoie un etat persistant `series<bool>` qui commence a `false`
- `enter = true` avec `exit = false` active l'etat
- `exit = true` avec `enter = false` desactive l'etat
- si les deux arguments valent `true` sur la meme barre, l'etat precedent est conserve
- si un echantillon d'entree courant vaut `na`, cette entree est traitee comme une transition inactive sur la barre courante
- le type de resultat est `series<bool>`

C'est la base prevue pour les declarations `regime` de premiere classe :

```palmscript
regime trend_long = state(close > ema(close, 20), close < ema(close, 20))
export trend_started = activated(trend_long)
```

### `barssince(condition)`

Regles :

- il exige exactement un argument
- l'argument doit etre `series<bool>`
- il renvoie `0` sur les barres ou l'echantillon courant de la condition vaut
  `true`
- il s'incremente sur chaque mise a jour de l'horloge propre a la condition
  apres le dernier evenement vrai
- il renvoie `na` jusqu'au premier evenement vrai
- si l'echantillon courant de la condition vaut `na`, la sortie courante vaut
  `na`
- le type de resultat est `series<float>`

### `valuewhen(condition, source, occurrence)`

Regles :

- il exige exactement trois arguments
- le premier argument doit etre `series<bool>`
- le second argument doit etre `series<float>` ou `series<bool>`
- le troisieme argument doit etre un litteral entier non negatif
- l'occurrence `0` signifie l'evenement vrai le plus recent
- le type de resultat correspond au type du second argument
- il renvoie `na` tant qu'il n'existe pas assez d'evenements vrais
  correspondants
- si l'echantillon courant de la condition vaut `na`, la sortie courante vaut
  `na`
- lorsque l'echantillon courant de la condition vaut `true`, l'echantillon
  courant de `source` est capture pour des occurrences futures

### `highest_since(anchor, source)` et `lowest_since(anchor, source)`

Regles :

- chacun exige exactement deux arguments
- le premier argument doit etre `series<bool>`
- le second argument doit etre `series<float>`
- lorsque l'echantillon courant de l'ancre vaut `true`, une nouvelle epoque
  ancree commence sur la barre courante
- la barre courante contribue immediatement a la nouvelle epoque
- avant la premiere ancre, le resultat est `na`
- les ancres vraies ulterieures abandonnent l'epoque ancree precedente et en
  demarrent une nouvelle
- le type de resultat est `series<float>`

### `highestbars_since(anchor, source)` et `lowestbars_since(anchor, source)`

Regles :

- chacun exige exactement deux arguments
- le premier argument doit etre `series<bool>`
- le second argument doit etre `series<float>`
- ils suivent les memes regles de reinitialisation d'epoque ancree que
  `highest_since` / `lowest_since`
- le resultat est le nombre de barres ecoulees depuis l'echantillon le plus
  haut ou le plus bas a l'interieur de l'epoque ancree courante
- avant la premiere ancre, le resultat est `na`
- le type de resultat est `series<float>`

### `valuewhen_since(anchor, condition, source, occurrence)`

Regles :

- il exige exactement quatre arguments
- les premier et second arguments doivent etre `series<bool>`
- le troisieme argument doit etre `series<float>` ou `series<bool>`
- le quatrieme argument doit etre un litteral entier non negatif
- lorsque l'echantillon courant de l'ancre vaut `true`, les correspondances
  precedentes de `condition` sont oubliees et une nouvelle epoque ancree
  commence sur la barre courante
- l'occurrence `0` signifie l'evenement correspondant le plus recent a
  l'interieur de l'epoque ancree courante
- avant la premiere ancre, le resultat est `na`
- le type de resultat correspond au type du troisieme argument

### `count_since(anchor, condition)`

Regles :

- il exige exactement deux arguments
- les deux arguments doivent etre `series<bool>`
- lorsque l'echantillon courant de l'ancre vaut `true`, le compteur courant est
  reinitialise et une nouvelle epoque ancree commence sur la barre courante
- la barre courante contribue immediatement a la nouvelle epoque ancree
- le compteur n'augmente que sur les barres ou l'echantillon courant de
  `condition` vaut `true`
- avant la premiere ancre, le resultat est `na`
- les ancres vraies ulterieures abandonnent l'epoque ancree precedente et en
  demarrent une nouvelle
- le type de resultat est `series<float>`

## `plot(value)`

`plot` emet un point de trace pour l'etape courante.

Regles :

- il exige exactement un argument
- l'argument doit etre numerique, `series<float>` ou `na`
- le type de resultat de l'expression est `void`
- `plot` ne doit pas etre appele a l'interieur du corps d'une fonction
  definie par l'utilisateur

Au runtime :

- les valeurs numeriques sont enregistrees comme points de trace
- `na` enregistre un point de trace sans valeur numerique

## Horloges De Mise A Jour

Les resultats builtin suivent les horloges de mise a jour de leurs entrees.

Exemples :

- `ema(spot.close, 20)` avance sur l'horloge de base
- `highest(spot.1w.close, 5)` avance sur l'horloge hebdomadaire
- `cum(spot.1w.close - spot.1w.close[1])` avance sur l'horloge hebdomadaire
- `crossover(bb.close, bn.close)` avance lorsque l'une ou l'autre des series
  source referencees avance
- `activated(trend_long)` avance sur l'horloge de `trend_long`
- `barssince(spot.close > spot.close[1])` avance sur l'horloge de cette serie
  de condition
- `valuewhen(trigger_series, bb.1h.close, 0)` avance sur l'horloge de
  `trigger_series`
- `highest_since(position_event.long_entry_fill, spot.high)` avance sur
  l'horloge partagee par l'ancre et la serie source
