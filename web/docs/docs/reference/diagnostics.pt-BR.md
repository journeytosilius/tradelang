# Diagnosticos

PalmScript expoe diagnosticos e erros a partir de tres camadas publicas.

## 1. Diagnosticos De Compilacao

Diagnosticos de compilacao sao falhas no nivel do codigo-fonte com spans.

Classes de diagnostico:

- erros lexicos
- erros de parsing
- erros de tipo e resolucao de nomes
- erros estruturais em tempo de compilacao

Exemplos:

- `interval` ausente ou duplicado
- template `source` nao suportado
- alias de source desconhecido
- referencia de intervalo `use` nao declarada
- referencia a um intervalo inferior ao intervalo base
- bindings duplicados
- recursao invalida de funcao
- aridade ou tipo de argumento builtin invalido

Esses diagnosticos aparecem por meio de:

- painel de diagnosticos do editor no IDE do navegador
- requisicoes de backtest emitidas pela app hospedada

## 2. Erros De Busca De Mercado

Depois de uma compilacao bem-sucedida, a preparacao do runtime pode falhar ao
montar os feeds historicos necessarios.

Exemplos:

- a janela de tempo solicitada e invalida
- o script nao possui declaracoes `source`
- uma requisicao ao exchange falha
- uma resposta do venue vem malformada
- um feed obrigatorio nao retorna dados na janela solicitada
- um simbolo nao pode ser resolvido pelo venue selecionado

Falhas de busca agora incluem tanto contexto da requisicao quanto o PalmScript conhece naquela camada, por exemplo a janela solicitada e a etapa de bootstrap do feed paper que disparou a requisicao.

## 3. Erros De Runtime

Erros de runtime acontecem depois que a preparacao de feeds comeca ou durante a
execucao.

Exemplos:

- erros de alinhamento de feed
- feeds de runtime ausentes ou duplicados
- esgotamento do budget de instrucoes
- stack underflow
- incompatibilidade de tipo durante a execucao
- slot local ou de serie invalido
- overflow de capacidade de historico
- incompatibilidade de tipo de saida durante a coleta de saidas

Manifests e snapshots de sessoes paper tambem expoem mensagens de falha por feed, para que `paper-status` e `paper-export` mostrem qual feed falhou, em que etapa e com qual erro upstream.

## Responsabilidade Por Camada

A camada responsavel pela falha faz parte do contrato:

- validade sintatica e semantica pertence a compilacao
- validade de exchange / rede / resposta pertence a busca de mercado
- consistencia de feeds e validade de execucao pertencem ao runtime

PalmScript falha explicitamente em vez de degradar a semantica silenciosamente.
