# Momentum, Volume, And Volatility Indicators

Cette page definit les indicateurs executables de momentum, oscillateurs,
volume et volatilite de PalmScript.

## `rsi(series, length)`

Regles :

- il exige exactement deux arguments
- le premier argument doit etre `series<float>`
- le second argument doit etre un litteral entier positif
- le type de resultat est `series<float>`
- la serie renvoie `na` tant qu'il n'existe pas assez d'historique pour
  initialiser l'etat de l'indicateur

## `roc(series[, length=10])`, `mom(series[, length=10])`, `rocp(series[, length=10])`, `rocr(series[, length=10])` et `rocr100(series[, length=10])`

Regles :

- le premier argument doit etre `series<float>`
- l'argument optionnel `length` doit etre un litteral entier positif
- en l'absence de `length`, la valeur par defaut TA-Lib est `10`
- `roc` s'evalue comme `((series - series[length]) / series[length]) * 100`
- `mom` s'evalue comme `series - series[length]`
- `rocp` s'evalue comme `(series - series[length]) / series[length]`
- `rocr` s'evalue comme `series / series[length]`
- `rocr100` s'evalue comme `(series / series[length]) * 100`
- si l'echantillon courant ou reference vaut `na`, le resultat est `na`
- si `series[length]` vaut `0`, `roc`, `rocp`, `rocr` et `rocr100` renvoient
  `na`

## `cmo(series[, length=14])`

Regles :

- le premier argument doit etre `series<float>`
- en l'absence de `length`, la valeur par defaut TA-Lib est `14`
- s'il est fourni, `length` doit etre un litteral entier superieur ou egal a
  `2`
- `cmo` utilise l'etat de gains / pertes lisse selon Wilder a la maniere de
  TA-Lib
- le type de resultat est `series<float>`
- si la somme des gains et pertes lisses vaut `0`, `cmo` renvoie `0`

## `cci(high, low, close[, length=14])`

Regles :

- les trois premiers arguments doivent etre `series<float>`
- en l'absence de `length`, la valeur par defaut TA-Lib est `14`
- s'il est fourni, `length` doit etre un litteral entier superieur ou egal a
  `2`
- `cci` utilise la moyenne du prix typique et l'ecart moyen sur la fenetre
  demandee
- si le delta de prix typique courant ou l'ecart moyen vaut `0`, `cci`
  renvoie `0`
- le type de resultat est `series<float>`

## `aroon(high, low[, length=14])` et `aroonosc(high, low[, length=14])`

Regles :

- les deux premiers arguments doivent etre `series<float>`
- en l'absence de `length`, la valeur par defaut TA-Lib est `14`
- s'il est fourni, `length` doit etre un litteral entier superieur ou egal a
  `2`
- `aroon` utilise une fenetre high/low de `length + 1` pour correspondre au
  lookback TA-Lib
- `aroon` renvoie un tuple `(aroon_down, aroon_up)` dans l'ordre de sortie
  TA-Lib
- `aroonosc` renvoie `aroon_up - aroon_down`
- les sorties a valeur tuple doivent etre destructurees avant toute autre
  utilisation

## `plus_dm(high, low[, length=14])`, `minus_dm(high, low[, length=14])`, `plus_di(high, low, close[, length=14])`, `minus_di(high, low, close[, length=14])`, `dx(high, low, close[, length=14])`, `adx(high, low, close[, length=14])` et `adxr(high, low, close[, length=14])`

Regles :

- tous les arguments de prix doivent etre `series<float>`
- en l'absence de `length`, la valeur par defaut TA-Lib est `14`
- s'il est fourni, `length` doit etre un litteral entier positif
- `plus_dm` et `minus_dm` renvoient le mouvement directionnel lisse selon
  Wilder
- `plus_di` et `minus_di` renvoient les indicateurs directionnels de Wilder
- `dx` renvoie l'ecart directionnel absolu multiplie par 100
- `adx` renvoie la moyenne Wilder de `dx`
- `adxr` renvoie la moyenne entre l'`adx` courant et l'`adx` retarde
- si un prix requis sur la barre active est `na`, le resultat pour cette barre est `na`
- le type de resultat est `series<float>`

## `atr(high, low, close[, length=14])` et `natr(high, low, close[, length=14])`

Regles :

- tous les arguments doivent etre `series<float>`
- en l'absence de `length`, la valeur par defaut TA-Lib est `14`
- s'il est fourni, `length` doit etre un litteral entier positif
- `atr` s'initialise a partir de l'average true range initial puis applique le
  lissage Wilder
- `natr` renvoie `(atr / close) * 100`
- si un prix requis sur la barre active est `na`, le resultat pour cette barre est `na`
- le type de resultat est `series<float>`

## `willr(high, low, close[, length=14])`

Regles :

- les trois premiers arguments doivent etre `series<float>`
- en l'absence de `length`, la valeur par defaut TA-Lib est `14`
- s'il est fourni, `length` doit etre un litteral entier superieur ou egal a
  `2`
- `willr` utilise le plus haut et le plus bas trainants sur la fenetre
  demandee
- le type de resultat est `series<float>`
- si l'amplitude plus-haut / plus-bas vaut `0`, `willr` renvoie `0`

## `mfi(high, low, close, volume[, length=14])` et `imi(open, close[, length=14])`

Regles :

- tous les arguments doivent etre `series<float>`
- en l'absence de `length`, la valeur par defaut TA-Lib est `14`
- s'il est fourni, `length` doit etre un litteral entier positif
- `mfi` utilise le prix typique et le money flow sur une fenetre trainante
- `imi` utilise le mouvement intraday open-close sur la fenetre demandee
- le type de resultat est `series<float>`

## `stoch(high, low, close[, fast_k=5[, slow_k=3[, slow_k_ma=ma_type.sma[, slow_d=3[, slow_d_ma=ma_type.sma]]]]])`, `stochf(high, low, close[, fast_k=5[, fast_d=3[, fast_d_ma=ma_type.sma]]])` et `stochrsi(series[, time_period=14[, fast_k=5[, fast_d=3[, fast_d_ma=ma_type.sma]]]])`

Regles :

- tous les arguments de prix ou de source doivent etre `series<float>`
- les periodes omises utilisent les valeurs par defaut TA-Lib
- les longueurs `fast_k`, `slow_k`, `fast_d` et `slow_d` doivent etre des
  litteraux entiers positifs
- `time_period` pour `stochrsi` doit etre un litteral entier superieur ou egal
  a `2`
- tous les arguments de moyenne mobile doivent etre des valeurs typees
  `ma_type.<variant>`
- `stoch` renvoie `(slowk, slowd)` dans l'ordre TA-Lib
- `stochf` renvoie `(fastk, fastd)` dans l'ordre TA-Lib
- `stochrsi` renvoie `(fastk, fastd)` dans l'ordre TA-Lib
- les sorties a valeur tuple doivent etre destructurees avant toute autre
  utilisation

## `ad(high, low, close, volume)`, `adosc(high, low, close, volume[, fast_length=3[, slow_length=10]])` et `obv(series, volume)`

Regles :

- tous les arguments doivent etre `series<float>`
- `ad` renvoie la ligne cumulative accumulation / distribution
- `adosc` renvoie la difference entre les EMA rapide et lente de la ligne
  accumulation / distribution
- en l'absence de `fast_length` et `slow_length`, les valeurs par defaut
  TA-Lib `3` et `10` sont utilisees
- `obv` s'initialise avec le `volume` courant puis ajoute ou soustrait ensuite
  le volume selon la direction du prix
- si l'echantillon de prix ou de volume requis vaut `na`, le resultat est `na`
- le type de resultat est `series<float>`

## `trange(high, low, close)`

Regles :

- tous les arguments doivent etre `series<float>`
- le premier echantillon de sortie est `na`
- les echantillons suivants utilisent la semantique TA-Lib du true range a
  partir de `high` courant, `low` courant et `close` precedent
- si un echantillon requis vaut `na`, le resultat est `na`
- le type de resultat est `series<float>`

## `anchored_vwap(anchor, price, volume)`

Rules:

- `anchor` must be `series<bool>`
- `price` and `volume` must be `series<float>`
- when the current `anchor` sample is `true`, the running VWAP resets on that same bar
- the anchor bar is included in the new anchored accumulation window
- if the current anchor, price, or volume sample is `na`, the current output sample is `na`
- if cumulative anchored volume is `0`, the current output sample is `na`
- the result type is `series<float>`
