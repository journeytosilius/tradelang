# Cookbook: Fontes Ligadas A Exchanges

Use fontes nomeadas quando a estrategia precisar buscar candles historicos
diretamente de exchanges suportadas.

```palmscript
interval 1m

source bn = binance.spot("BTCUSDT")
source bb = bybit.usdt_perps("BTCUSDT")
use bb 1h

plot(bn.close)
plot(bb.1h.close)
```

PalmScript tambem suporta templates de source para Bybit e Gate:

- `bybit.spot("BTCUSDT")`
- `bybit.usdt_perps("BTCUSDT")`
- `gate.spot("BTC_USDT")`
- `gate.usdt_perps("BTC_USDT")`

Exemplos representativos incluidos no repositorio:

- `crates/palmscript/examples/strategies/binance_spot_btcusdt_weekly_trend.ps`
- `crates/palmscript/examples/strategies/binance_usdm_auxiliary_fields.ps`
- `crates/palmscript/examples/strategies/bybit_spot.ps`
- `crates/palmscript/examples/strategies/bybit_usdt_perps_backtest.ps`
- `crates/palmscript/examples/strategies/gate_spot.ps`
- `crates/palmscript/examples/strategies/gate_usdt_perps_backtest.ps`
- `crates/palmscript/examples/strategies/cross_exchange_bybit_gate_spread.ps`

## Teste No IDE Do Navegador

Abra [https://palmscript.dev/](https://palmscript.dev/), cole o exemplo
no editor e execute-o sobre o historico BTCUSDT disponivel na app.

## O Que Observar

- scripts source-aware precisam usar series de mercado qualificadas por fonte
- `use bb 1h` e obrigatorio antes de `bb.1h.close`
- o script ainda tem um unico `interval` base global
- o runtime resolve cada feed `(source, interval)` necessario antes da
  execucao
- `binance.usdm` tambem suporta os campos historicos `funding_rate`,
  `mark_price`, `index_price`, `premium_index` e `basis`
- Bybit espera simbolos nativos da venue como `BTCUSDT`
- Gate espera simbolos nativos da venue como `BTC_USDT`
- `run paper` agora inicializa esses campos auxiliares da Binance USD-M pelo
  mesmo caminho historico e os mantem nas sessoes paper armadas
- `run market`, `run backtest`, `run walk-forward`, `run walk-forward-sweep` e
  `run optimize` resolvem as mesmas declaracoes de source ligadas a exchanges

Referencia:

- [Intervalos e Fontes](../../reference/intervals-and-sources.md)
