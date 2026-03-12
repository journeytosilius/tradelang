# Intervalos E Fontes

Esta pagina define as regras normativas de intervalos e fontes no PalmScript.

## Intervalos Suportados

PalmScript aceita os literais de intervalo listados na
[Tabela De Intervalos](intervals.md). Os intervalos diferenciam maiusculas de
minusculas.

## Intervalo Base

Todo script declara exatamente um intervalo base:

```palmscript
interval 1m
```

O intervalo base define o clock de execucao.

## Fontes Nomeadas

Scripts executaveis declaram uma ou mais fontes nomeadas ligadas a exchanges:

```palmscript
interval 1m
source bb = bybit.usdt_perps("BTCUSDT")
source bn = binance.spot("BTCUSDT")
use bb 1h

plot(bn.close - bb.1h.close)
```

Regras:

- pelo menos uma declaracao `source` e obrigatoria
- series de mercado precisam ser qualificadas por fonte
- cada fonte declarada contribui com um feed base no intervalo base do script
- `use <alias> <interval>` declara um intervalo adicional para aquela fonte
- `<alias>.<field>` referencia aquela fonte no intervalo base
- `<alias>.<interval>.<field>` referencia aquela fonte no intervalo nomeado
- referencias a intervalos inferiores ao intervalo base sao rejeitadas

## Templates De Source Suportados

PalmScript atualmente suporta estes templates de primeira classe:

- `binance.spot("<symbol>")`
- `binance.usdm("<symbol>")`
- `bybit.spot("<symbol>")`
- `bybit.usdt_perps("<symbol>")`
- `gate.spot("<symbol>")`
- `gate.usdt_perps("<symbol>")`

O suporte a intervalos depende do template:

- `binance.spot` aceita todos os intervalos PalmScript suportados
- `binance.usdm` aceita todos os intervalos PalmScript suportados
- `bybit.spot` aceita `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w` e `1M`
- `bybit.usdt_perps` aceita `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `4h`, `6h`, `12h`, `1d`, `1w` e `1M`
- `gate.spot` aceita `1s`, `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h`, `1d` e `1M`
- `gate.usdt_perps` aceita `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `8h` e `1d`

Restricoes operacionais de busca tambem dependem do template:

- Bybit usa simbolos nativos da venue como `BTCUSDT`
- Gate usa simbolos nativos da venue como `BTC_USDT`
- os klines REST da Bybit chegam em ordem decrescente e o PalmScript os reordena antes das verificacoes de alinhamento
- as APIs de candles da Gate usam Unix seconds e o PalmScript as normaliza para Unix milliseconds UTC
- a paginacao de Gate spot e futures usa janelas de tempo porque a API publica nao permite `limit` junto com `from` / `to`
- as requisicoes de Gate spot e futures sao limitadas a 1000 candles por chamada HTTP para evitar `400 Bad Request` causados por janelas amplas demais
- feeds da Binance, Bybit e Gate sao paginados internamente
- quando um venue rejeita uma busca, o PalmScript mostra o status HTTP junto com a URL da requisicao e um trecho truncado do corpo de resposta quando houver
- as URLs base podem ser sobrescritas com
  `PALMSCRIPT_BINANCE_SPOT_BASE_URL`, `PALMSCRIPT_BINANCE_USDM_BASE_URL`,
  `PALMSCRIPT_BYBIT_BASE_URL` e `PALMSCRIPT_GATE_BASE_URL`; no Gate, tanto a
  raiz do host, por exemplo `https://api.gateio.ws`, quanto a URL base completa
  `/api/v4` sao aceitas

## Conjunto De Campos De Source

Todos os templates de source sao normalizados para os mesmos campos canonicos
de mercado:

- `time`
- `open`
- `high`
- `low`
- `close`
- `volume`

Regras:

- `time` e o horario de abertura do candle em milissegundos Unix UTC
- campos de preco e volume sao numericos
- campos extras especificos do venue nao sao expostos na linguagem

## Intervalos Iguais, Superiores E Inferiores

PalmScript distingue tres casos para um intervalo referenciado em relacao ao
intervalo base:

- intervalo igual: valido
- intervalo superior: valido se declarado com `use <alias> <interval>`
- intervalo inferior: rejeitado

## Semantica De Runtime

No modo mercado:

- PalmScript busca diretamente dos venues os feeds `(source, interval)`
  necessarios
- a timeline de execucao base e a uniao dos tempos de abertura das barras de
  intervalo base de todas as fontes declaradas
- se uma fonte nao tiver barra base em um passo da timeline, ela contribui com
  `na` nesse passo
- intervalos de fonte mais lentos mantem o ultimo valor totalmente fechado ate
  o proximo limite de fechamento

## Garantia Sem Lookahead

PalmScript nao deve expor um candle de intervalo superior antes que ele esteja
totalmente fechado.

Isso se aplica a intervalos qualificados source-aware como `bb.1h.close`.

## Regras De Alinhamento Em Runtime

Feeds preparados precisam estar alinhados aos seus intervalos declarados.

O runtime rejeita feeds que estejam:

- desalinhados ao limite do intervalo
- fora de ordenacao
- duplicados no mesmo tempo de abertura do intervalo
