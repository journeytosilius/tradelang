# Visao Geral Dos Indicadores

Esta secao define a superficie executavel de indicadores do PalmScript.

Use [Builtins](builtins.md) para regras compartilhadas de callables, helpers
builtin, `plot` e regras de desestruturacao de tuplas que se aplicam em toda a
linguagem.

## Familias De Indicadores

PalmScript atualmente documenta indicadores nestas familias:

- [Trend and Overlap](indicators-trend-and-overlap.md)
- [Momentum, Volume, and Volatility](indicators-momentum-volume-volatility.md)
- [Math, Price, and Statistics](indicators-math-price-statistics.md)

## Regras Compartilhadas Dos Indicadores

Regras:

- nomes de indicadores sao identificadores builtin, entao eles sao chamados
  diretamente, por exemplo `ema(spot.close, 20)`
- entradas de indicadores ainda precisam seguir as regras de series
  qualificadas por fonte em [Intervalos e Fontes](intervals-and-sources.md)
- argumentos opcionais de comprimento usam os defaults de TA-Lib documentados
  nas paginas de cada familia
- argumentos do tipo comprimento descritos como literais devem ser literais
  inteiros no codigo-fonte
- indicadores tuple-valued devem ser desestruturados com `let (...) = ...`
  antes de qualquer outro uso
- saidas de indicadores seguem o clock de atualizacao implicado por suas
  entradas de serie
- indicadores propagam `na` a menos que o contrato especifico diga o contrario

## Indicadores Tuple-Valued

Os indicadores tuple-valued atuais sao:

- `macd(series, fast_length, slow_length, signal_length)`
- `minmax(series[, length=30])`
- `minmaxindex(series[, length=30])`
- `aroon(high, low[, length=14])`
- `supertrend(high, low, close[, atr_length=10[, multiplier=3.0]])`
- `donchian(high, low[, length=20])`

Eles devem ser desestruturados imediatamente:

```palmscript
interval 1m
source spot = binance.spot("BTCUSDT")

let (line, signal, hist) = macd(spot.close, 12, 26, 9)
plot(line)
```

## Nomes TA-Lib Executaveis x Reservados

PalmScript reserva um catalogo TA-Lib mais amplo do que executa hoje.

- estas paginas de indicadores definem o subconjunto executavel
- [TA-Lib Surface](ta-lib.md) define a superficie mais ampla de nomes
  reservados e metadata
- um nome TA-Lib reservado mas ainda nao executavel produz um diagnostico de
  compilacao deterministico
