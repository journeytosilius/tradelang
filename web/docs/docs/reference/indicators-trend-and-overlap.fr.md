# Trend And Overlap Indicators

Cette page definit les indicateurs de tendance et de recouvrement executables
de PalmScript.

## `sma(series, length)`

Regles :

- il exige exactement deux arguments
- le premier argument doit etre `series<float>`
- le second argument doit etre un litteral entier positif
- le type de resultat est `series<float>`
- si l'historique est insuffisant, l'echantillon courant est `na`
- si la fenetre requise contient `na`, l'echantillon courant est `na`

## `ema(series, length)`

Regles :

- il exige exactement deux arguments
- le premier argument doit etre `series<float>`
- le second argument doit etre un litteral entier positif
- le type de resultat est `series<float>`
- la serie renvoie `na` jusqu'a ce que la fenetre de seed soit disponible

## `ma(series, length, ma_type)`

Regles :

- il exige exactement trois arguments
- le premier argument doit etre `series<float>`
- le second argument doit etre un litteral entier positif
- le troisieme argument doit etre une valeur typee `ma_type.<variant>`
- le type de resultat est `series<float>`
- toutes les variantes `ma_type` sont implementees
- `ma_type.mama` reproduit le comportement de TA-Lib amont et ignore le
  parametre `length` explicite en utilisant les valeurs par defaut MAMA
  `fast_limit=0.5` et `slow_limit=0.05`

## `apo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])` et `ppo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`

Regles :

- le premier argument doit etre `series<float>`
- `fast_length` et `slow_length` valent `12` et `26` par defaut
- s'ils sont fournis, `fast_length` et `slow_length` doivent etre des
  litteraux entiers superieurs ou egaux a `2`
- si elle est fournie, le quatrieme argument doit etre une valeur typee
  `ma_type.<variant>`
- en l'absence de `ma_type`, la valeur par defaut est `ma_type.sma`
- `apo` renvoie `fast_ma - slow_ma`
- `ppo` renvoie `((fast_ma - slow_ma) / slow_ma) * 100`
- si la moyenne mobile lente vaut `0`, `ppo` renvoie `0`
- le meme ensemble executable de variantes `ma_type` que `ma(...)` est pris en
  charge
- le type de resultat est `series<float>`

## `macd(series, fast_length, slow_length, signal_length)`

Regles :

- il exige exactement quatre arguments
- le premier argument doit etre `series<float>`
- les arguments restants doivent etre des litteraux entiers positifs
- le type de resultat est un tuple de trois series dans l'ordre TA-Lib :
  `(macd_line, signal, histogram)`
- le resultat doit etre destructure avant d'etre utilise dans `plot`,
  `export`, les conditions ou d'autres expressions

## `macdfix(series[, signal_length=9])`

Regles :

- le premier argument doit etre `series<float>`
- `signal_length` vaut `9` par defaut
- s'il est fourni, `signal_length` doit etre un litteral entier positif
- le type de resultat est un tuple de trois series dans l'ordre TA-Lib :
  `(macd_line, signal, histogram)`
- le resultat doit etre destructure avant d'etre utilise dans `plot`,
  `export`, les conditions ou d'autres expressions

## `macdext(series[, fast_length=12[, fast_ma=ma_type.sma[, slow_length=26[, slow_ma=ma_type.sma[, signal_length=9[, signal_ma=ma_type.sma]]]]]])`

Regles :

- le premier argument doit etre `series<float>`
- les longueurs omises utilisent les valeurs par defaut TA-Lib `12`, `26` et
  `9`
- `fast_length` et `slow_length` doivent etre des litteraux entiers superieurs
  ou egaux a `2`
- `signal_length` doit etre un litteral entier superieur ou egal a `1`
- chaque argument de moyenne mobile doit etre une valeur typee
  `ma_type.<variant>`
- le meme ensemble executable de variantes `ma_type` que `ma(...)` est pris en
  charge
- le type de resultat est un tuple de trois series dans l'ordre TA-Lib :
  `(macd_line, signal, histogram)`
- le resultat doit etre destructure avant toute autre utilisation

## `bbands(series[, length=5[, deviations_up=2.0[, deviations_down=2.0[, ma_type=ma_type.sma]]]])`

Regles :

- le premier argument doit etre `series<float>`
- `length` vaut `5` par defaut
- s'il est fourni, `length` doit etre un litteral entier positif
- s'ils sont fournis, `deviations_up` et `deviations_down` doivent etre des
  scalaires numeriques
- s'il est fourni, le cinquieme argument doit etre une valeur typee
  `ma_type.<variant>`
- le type de resultat est un tuple de trois series dans l'ordre TA-Lib :
  `(upper, middle, lower)`
- le resultat doit etre destructure avant d'etre utilise dans `plot`,
  `export`, les conditions ou d'autres expressions

## `accbands(high, low, close[, length=20])`

Regles :

- les trois premiers arguments doivent etre `series<float>`
- en l'absence de `length`, la valeur par defaut TA-Lib est `20`
- s'il est fourni, `length` doit etre un litteral entier superieur ou egal a
  `2`
- le type de resultat est un tuple de trois series dans l'ordre TA-Lib :
  `(upper, middle, lower)`
- le resultat doit etre destructure avant toute autre utilisation

## `dema(series[, length=30])`, `tema(series[, length=30])`, `trima(series[, length=30])`, `kama(series[, length=30])`, `t3(series[, length=5[, volume_factor=0.7]])` et `trix(series[, length=30])`

Regles :

- le premier argument doit etre `series<float>`
- `length` vaut `30` par defaut pour `dema`, `tema`, `trima`, `kama` et
  `trix`
- `t3` utilise `length=5` et `volume_factor=0.7` par defaut
- s'il est fourni, `length` doit etre un litteral entier positif
- s'il est fourni, `volume_factor` doit etre un scalaire numerique
- le type de resultat est `series<float>`

## `mavp(series, periods, minimum_period, maximum_period, ma_type)`

Regles :

- les deux premiers arguments doivent etre `series<float>`
- `minimum_period` et `maximum_period` doivent etre des litteraux entiers
  superieurs ou egaux a `2`
- le cinquieme argument doit etre une valeur typee `ma_type.<variant>`
- la famille de moyennes mobiles correspond au meme sous-ensemble executable de
  `ma_type` que `ma(...)`
- `periods` est borne barre par barre dans `[minimum_period, maximum_period]`
- le type de resultat est `series<float>`

## `mama(series[, fast_limit=0.5[, slow_limit=0.05]])`

Regles :

- le premier argument doit etre `series<float>`
- `fast_limit` et `slow_limit` valent `0.5` et `0.05` par defaut
- s'ils sont fournis, les deux arguments optionnels doivent etre des scalaires
  numeriques
- le type de resultat est un tuple de deux series dans l'ordre TA-Lib :
  `(mama, fama)`
- le resultat doit etre destructure avant toute autre utilisation

## `ht_dcperiod(series)`, `ht_dcphase(series)`, `ht_phasor(series)`, `ht_sine(series)`, `ht_trendline(series)` et `ht_trendmode(series)`

Regles :

- chaque fonction exige exactement un argument `series<float>`
- `ht_dcperiod`, `ht_dcphase` et `ht_trendline` renvoient `series<float>`
- `ht_trendmode` renvoie `series<float>` avec les valeurs de tendance `0` / `1`
  de TA-Lib
- `ht_phasor` renvoie un tuple a deux valeurs `(inphase, quadrature)`
- `ht_sine` renvoie un tuple a deux valeurs `(sine, lead_sine)`
- les resultats tuple doivent etre destructures avant toute autre utilisation
- ces indicateurs suivent le comportement de warmup des transformations
  de Hilbert de TA-Lib et produisent `na` jusqu'a ce que le lookback requis soit
  satisfait

## `sar(high, low[, acceleration=0.02[, maximum=0.2]])` et `sarext(high, low[, ...])`

Regles :

- `high` et `low` doivent etre `series<float>`
- tous les parametres optionnels du SAR sont des scalaires numeriques
- `sar` renvoie le Parabolic SAR standard
- `sarext` expose les controles SAR etendus de TA-Lib et renvoie des valeurs
  negatives pendant une position short, conformement au comportement amont de
  TA-Lib
- le type de resultat est `series<float>`

## `wma(series[, length=30])`

Regles :

- le premier argument doit etre `series<float>`
- `length` vaut `30` par defaut
- s'il est fourni, `length` doit etre un litteral entier superieur ou egal a
  `2`
- le type de resultat est `series<float>`
- si l'historique est insuffisant, l'echantillon courant est `na`
- si la fenetre requise contient `na`, l'echantillon courant est `na`

## `midpoint(series[, length=14])` et `midprice(high, low[, length=14])`

Regles :

- `midpoint` exige `series<float>` comme premier argument
- `midprice` exige `series<float>` pour `high` et `low`
- la fenetre trainante optionnelle vaut `14` par defaut
- si elle est fournie, la fenetre doit etre un litteral entier superieur ou
  egal a `2`
- la fenetre inclut l'echantillon courant
- si l'historique est insuffisant, le resultat est `na`
- si un echantillon requis de la fenetre vaut `na`, le resultat est `na`
- le type de resultat est `series<float>`

## `linearreg(series[, length=14])`, `linearreg_angle(series[, length=14])`, `linearreg_intercept(series[, length=14])`, `linearreg_slope(series[, length=14])` et `tsf(series[, length=14])`

Regles :

- le premier argument doit etre `series<float>`
- `length` vaut `14` par defaut
- s'il est fourni, `length` doit etre un litteral entier superieur ou egal a
  `2`
- si l'historique est insuffisant, l'echantillon courant est `na`
- si la fenetre requise contient `na`, l'echantillon courant est `na`
- `linearreg` renvoie la valeur ajustee sur la barre courante
- `linearreg_angle` renvoie l'angle de la pente ajustee
- `linearreg_intercept` renvoie l'ordonnee a l'origine ajustee
- `linearreg_slope` renvoie la pente ajustee
- `tsf` renvoie la prevision a une etape
- le type de resultat est `series<float>`

## `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])`

Rules:

- the first three arguments must be `series<float>`
- omitted `atr_length` defaults to `10`
- omitted `multiplier` defaults to `3.0`
- if provided, `atr_length` must be an integer literal greater than or equal to `1`
- if provided, `multiplier` must be a numeric scalar
- `supertrend` returns a 2-tuple `(line, bullish)`
- `line` is the active carried band and `bullish` is the persistent regime direction
- the ATR component uses Wilder smoothing and requires prior-close history, so the result is `na` until the lookback is satisfied
- tuple-valued outputs must be destructured before further use

## `donchian(high, low[, length=20])`

Rules:

- the first two arguments must be `series<float>`
- omitted `length` defaults to `20`
- if provided, `length` must be an integer literal greater than or equal to `1`
- `donchian` returns a 3-tuple `(upper, middle, lower)`
- `upper` is the trailing highest high, `lower` is the trailing lowest low, and `middle` is `(upper + lower) / 2`
- if insufficient history exists, or any required sample is `na`, the current tuple is `(na, na, na)`
- tuple-valued outputs must be destructured before further use
