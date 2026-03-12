# TA-Lib Surface

PalmScript は、upstream TA-Lib commit `1bdf54384036852952b8b4cb97c09359ae407bd0` に固定された型付き TA-Lib 統合レイヤーを含みます。

PalmScript はまだ TA-Lib の全関数カタログを実行可能な言語 surface としては公開していませんが、そのより広いカタログ名は予約されており、その surface に必要な型付き言語機能を使います。

- `ma_type.<variant>` enum literal
- 複数出力 TA-Lib builtin のタプル分解
- 固定された TA-Lib metadata snapshot
- 生成済み 161 関数カタログ

現在の metadata 駆動 surface の挙動:

- 161 個の TA-Lib 関数名はすべて builtin 名として予約されている
- IDE completion と hover は生成済み TA-Lib signature と summary を表示できる
- まだ実装されていないカタログ関数を呼ぶと、不明な識別子として扱われる代わりに決定的な compile diagnostic が返る
- `tests/data/ta_lib/` にある committed oracle fixture が、実装済み subset を upstream C library と照合する

言い換えると:

- 予約名 coverage はランタイム実行 coverage より広い
- IDE / catalog の可視性は、その関数が実行可能であることを意味しない
- 実行可能 subset の権威ある文書は [Builtins](builtins.md) と [Indicators](indicators.md) セクション

実装済み TA-Lib 風 builtin:

- `ma(series, length, ma_type)`
- `apo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`
- `ppo(series[, fast_length=12[, slow_length=26[, ma_type=ma_type.sma]]])`
- `macd(series, fast_length, slow_length, signal_length)`
- `macdfix(series[, signal_length=9])`
- `macdext(series[, fast_length=12[, fast_ma=ma_type.sma[, slow_length=26[, slow_ma=ma_type.sma[, signal_length=9[, signal_ma=ma_type.sma]]]]]])`
- unary math transforms: `acos`, `asin`, `atan`, `ceil`, `cos`, `cosh`, `exp`, `floor`, `ln`, `log10`, `sin`, `sinh`, `sqrt`, `tan`, `tanh`
- math operators: `add`, `div`, `mult`, `sub`, `max`, `min`, `sum`, `maxindex`, `minindex`, `minmax`, `minmaxindex`
- price transforms: `avgprice`, `medprice`, `typprice`, `wclprice`
- overlap helpers: `accbands`, `bbands`, `dema`, `ema`, `kama`, `ma`, `mavp`, `midpoint`, `midprice`, `sar`, `sarext`, `sma`, `t3`, `tema`, `trima`, `wma`
- cycle helpers: `ht_dcperiod`, `ht_dcphase`, `ht_phasor`, `ht_sine`, `ht_trendline`, `ht_trendmode`, `mama`
- statistics helpers: `avgdev`, `stddev`, `var`, `linearreg`, `linearreg_angle`, `linearreg_intercept`, `linearreg_slope`, `tsf`, `beta`, `correl`
- momentum helpers: `adx`, `adxr`, `apo`, `aroon`, `aroonosc`, `bop`, `cci`, `cmo`, `dx`, `imi`, `mfi`, `minus_di`, `minus_dm`, `mom`, `plus_di`, `plus_dm`, `ppo`, `roc`, `rocp`, `rocr`, `rocr100`, `stoch`, `stochf`, `stochrsi`, `trix`, `willr`
- volume and volatility helpers: `ad`, `adosc`, `atr`, `natr`, `obv`, `trange`

現在の `ma_type` variant:

- `ma_type.sma`
- `ma_type.ema`
- `ma_type.wma`
- `ma_type.dema`
- `ma_type.tema`
- `ma_type.trima`
- `ma_type.kama`
- `ma_type.mama`
- `ma_type.t3`

現在の `ma_type` variant はすべて `ma(...)`, `apo(...)`, `ppo(...)`, `bbands(...)`, `macdext(...)`, `mavp(...)`, `stoch(...)`, `stochf(...)`, `stochrsi(...)` を通じて実行可能です。汎用 TA-Lib moving-average family では、`ma_type.mama` は上流 TA-Lib と同じく明示的な `length` を無視し、MAMA 既定値 `fast_limit=0.5` と `slow_limit=0.05` を使います。

現在の executable surface で尊重される TA-Lib 既定値:

- `max`, `min`, `sum` の既定 window は `30`
- `midpoint` と `midprice` の既定 window は `14`
- `wma`, `maxindex`, `minindex`, `minmax`, `minmaxindex` の既定 window は `30`
- `avgdev` の既定 window は `14`
- `stddev` と `var` の既定値は `length=5`
- `linearreg`, `linearreg_angle`, `linearreg_intercept`, `linearreg_slope`, `tsf` の既定値は `length=14`
- `beta` の既定値は `length=5` で、TA-Lib の return-based beta 計算を使う
- `correl` の既定値は `length=30`
- `apo` と `ppo` の既定値は `fast_length=12`, `slow_length=26`, `ma_type.sma`
- `macdfix` の既定値は `signal_length=9`
- `macdext` の既定値は `fast_length=12`, `slow_length=26`, `signal_length=9` で、三つの MA ロールすべてに `ma_type.sma` を使う
- `bbands` の既定値は `length=5`, `deviations_up=2`, `deviations_down=2`, `ma_type.sma`
- `accbands` の既定値は `length=20`
- `mavp` では `minimum_period`, `maximum_period`, `ma_type` の明示指定が必要
- `sar` の既定値は `acceleration=0.02`, `maximum=0.2`
- `sarext` の既定値は `start_value=0`, `offset_on_reverse=0`, `af_init_long=0.02`, `af_long=0.02`, `af_max_long=0.2`, `af_init_short=0.02`, `af_short=0.02`, `af_max_short=0.2`
- `aroon` と `aroonosc` の既定値は `length=14`
- `atr`, `natr`, `plus_dm`, `minus_dm`, `plus_di`, `minus_di`, `dx`, `adx`, `adxr`, `mfi`, `imi` の既定値は `length=14`
- `adosc` の既定値は `fast_length=3`, `slow_length=10`
- `cci` の既定値は `length=14`
- `cmo` の既定値は `length=14`
- `dema`, `tema`, `trima`, `kama`, `trix` の既定値は `length=30`
- `t3` の既定値は `length=5`, `volume_factor=0.7`
- `mama` の既定値は `fast_limit=0.5`, `slow_limit=0.05`
- `mom`, `roc`, `rocp`, `rocr`, `rocr100` の既定値は `length=10`
- `stoch` の既定値は `fast_k=5`, `slow_k=3`, `slow_d=3` で、両 smoothing stage に `ma_type.sma`
- `stochf` の既定値は `fast_k=5`, `fast_d=3`, `ma_type.sma`
- `stochrsi` の既定値は `time_period=14`, `fast_k=5`, `fast_d=3`, `ma_type.sma`
- `willr` の既定値は `length=14`

実装済み subset の oracle fixture 更新:

```bash
python3 tools/generate_talib_fixtures.py
cargo test --test ta_lib_parity
```

タプル返却の例:

```palm
interval 1m
source spot = binance.spot("BTCUSDT")

let (line, signal, hist) = macd(spot.close, 12, 26, 9)
plot(line)
plot(signal)
plot(hist)
```
