# Math, Price, And Statistics Indicators

Cette page definit les transformations mathematiques executables, les
transformations de prix et les indicateurs orientes statistiques de PalmScript.

## Transformations Mathematiques TA-Lib

Ces builtins sont actuellement executables :

- `acos(real)`
- `asin(real)`
- `atan(real)`
- `ceil(real)`
- `cos(real)`
- `cosh(real)`
- `exp(real)`
- `floor(real)`
- `ln(real)`
- `log10(real)`
- `sin(real)`
- `sinh(real)`
- `sqrt(real)`
- `tan(real)`
- `tanh(real)`

Regles :

- chacune exige exactement un argument numerique ou `series<float>`
- si l'entree est une serie, le type de resultat est `series<float>`
- si l'entree est scalaire, le type de resultat est `float`
- si l'entree est `na`, le resultat est `na`

## Transformations Arithmetiques Et De Prix TA-Lib

Ces builtins sont actuellement executables :

- `add(a, b)`
- `div(a, b)`
- `mult(a, b)`
- `sub(a, b)`
- `avgprice(open, high, low, close)`
- `bop(open, high, low, close)`
- `medprice(high, low)`
- `typprice(high, low, close)`
- `wclprice(high, low, close)`

Regles :

- tous les arguments doivent etre numeriques, `series<float>` ou `na`
- si un argument est une serie, le type de resultat est `series<float>`
- sinon le type de resultat est `float`
- si une entree requise vaut `na`, le resultat est `na`

Regle OHLC supplementaire :

- `bop` renvoie `(close - open) / (high - low)` et renvoie `0` lorsque
  `high - low <= 0`

## `max(series[, length=30])`, `min(series[, length=30])` et `sum(series[, length=30])`

Regles :

- le premier argument doit etre `series<float>`
- la fenetre trainante optionnelle vaut `30` par defaut
- si elle est fournie, la fenetre doit etre un litteral entier superieur ou
  egal a `2`
- la fenetre inclut l'echantillon courant
- si l'historique est insuffisant, le resultat est `na`
- si un echantillon requis de la fenetre vaut `na`, le resultat est `na`
- le type de resultat est `series<float>`

## `avgdev(series[, length=14])`

Regles :

- le premier argument doit etre `series<float>`
- `length` vaut `14` par defaut
- s'il est fourni, `length` doit etre un litteral entier superieur ou egal a
  `2`
- le type de resultat est `series<float>`
- si l'historique est insuffisant, l'echantillon courant est `na`
- si la fenetre requise contient `na`, l'echantillon courant est `na`

## `maxindex(series[, length=30])` et `minindex(series[, length=30])`

Regles :

- le premier argument doit etre `series<float>`
- `length` vaut `30` par defaut
- s'il est fourni, `length` doit etre un litteral entier superieur ou egal a
  `2`
- `maxindex` et `minindex` renvoient `series<float>` contenant l'indice de
  barre absolu en `f64`
- si l'historique est insuffisant, l'echantillon courant est `na`
- si la fenetre requise contient `na`, l'echantillon courant est `na`

## `minmax(series[, length=30])` et `minmaxindex(series[, length=30])`

Regles :

- le premier argument doit etre `series<float>`
- `length` vaut `30` par defaut
- s'il est fourni, `length` doit etre un litteral entier superieur ou egal a
  `2`
- `minmax` renvoie un tuple `(min_value, max_value)` dans l'ordre de sortie
  TA-Lib
- `minmaxindex` renvoie un tuple `(min_index, max_index)` dans l'ordre de
  sortie TA-Lib
- les sorties a valeur tuple doivent etre destructurees avant toute autre
  utilisation
- si l'historique est insuffisant, l'echantillon courant est `na`
- si la fenetre requise contient `na`, l'echantillon courant est `na`

## `stddev(series[, length=5[, deviations=1.0]])` et `var(series[, length=5[, deviations=1.0]])`

Regles :

- le premier argument doit etre `series<float>`
- `length` vaut `5` par defaut
- s'il est fourni, `length` doit etre un litteral entier
- `stddev` exige `length >= 2`
- `var` autorise `length >= 1`
- `deviations` vaut `1.0` par defaut
- `stddev` multiplie la racine carree de la variance glissante par
  `deviations`
- `var` ignore l'argument `deviations` pour correspondre a TA-Lib
- le type de resultat est `series<float>`
- si l'historique est insuffisant, l'echantillon courant est `na`
- si la fenetre requise contient `na`, l'echantillon courant est `na`

## `beta(series0, series1[, length=5])` et `correl(series0, series1[, length=30])`

Regles :

- les deux entrees doivent etre `series<float>`
- `beta` vaut `length=5` par defaut
- `correl` vaut `length=30` par defaut
- s'il est fourni, `length` doit etre un litteral entier respectant le minimum
  TA-Lib de ce builtin
- `beta` suit la formulation a base de rendements de TA-Lib ; il ne produit
  donc une sortie qu'apres `length + 1` echantillons source
- `correl` renvoie la correlation de Pearson des deux series d'entree brutes
- le type de resultat est `series<float>`
- si l'historique est insuffisant, l'echantillon courant est `na`
- si la fenetre requise contient `na`, l'echantillon courant est `na`

## `percentile(series[, length=20[, percentage=50.0]])`

Rules:

- the first argument must be `series<float>`
- omitted `length` defaults to `20`
- omitted `percentage` defaults to `50.0`
- if provided, `length` must be an integer literal greater than or equal to `1`
- if provided, `percentage` must be a numeric scalar
- `percentage` is clamped into the inclusive `0..100` range
- the trailing window is sorted and sampled with linear interpolation between adjacent ranks
- if insufficient history exists, or any required sample is `na`, the result is `na`
- the result type is `series<float>`

## `zscore(series[, length=20])`

Rules:

- the first argument must be `series<float>`
- omitted `length` defaults to `20`
- if provided, `length` must be an integer literal greater than or equal to `1`
- `zscore` evaluates the current sample against the trailing-window mean and population standard deviation
- if the trailing variance is `0`, `zscore` returns `0`
- if insufficient history exists, or any required sample is `na`, the result is `na`
- the result type is `series<float>`

## `ulcer_index(series[, length=14])`

Rules:

- the first argument must be `series<float>`
- omitted `length` defaults to `14`
- if provided, `length` must be an integer literal greater than or equal to `1`
- `ulcer_index` measures rolling drawdown severity in percentage terms over the trailing window
- it tracks the running peak across the window from oldest to newest, squares percentage drawdowns, averages them, and returns the square root
- if insufficient history exists, or any required sample is `na`, the result is `na`
- the result type is `series<float>`
