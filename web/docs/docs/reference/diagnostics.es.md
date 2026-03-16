# Diagnosticos

PalmScript expone diagnosticos y errores desde tres capas publicas.

## 1. Diagnosticos De Compilacion

Los diagnosticos de compilacion son fallos a nivel de codigo fuente con spans.

Clases de diagnostico:

- errores lexicos
- errores de parseo
- errores de tipos y resolucion de nombres
- errores estructurales en tiempo de compilacion

Ejemplos:

- `interval` ausente o duplicado
- template `source` no soportado
- alias de fuente desconocido
- referencia a un intervalo `use` no declarado
- referencia a un intervalo inferior al base
- bindings duplicados
- recursion de funciones invalida
- aridad o tipo de argumentos builtin invalidos

Estos diagnosticos aparecen a traves de:

- el panel de diagnosticos del editor en el IDE del navegador
- las solicitudes de backtest emitidas por la app alojada

## 2. Errores De Carga De Mercado

Despues de una compilacion exitosa, la preparacion del runtime puede fallar al
armar los feeds historicos requeridos.

Ejemplos:

- la ventana de tiempo solicitada es invalida
- el script no tiene declaraciones `source`
- falla una peticion al exchange
- la respuesta de una venue esta mal formada
- un feed requerido no devuelve datos en la ventana solicitada
- un simbolo no puede resolverse en la venue seleccionada

Los fallos de fetch ahora incluyen tanto contexto de la solicitud como PalmScript conoce en esa capa, por ejemplo la ventana solicitada y la etapa de bootstrap del feed paper que disparo la peticion.

## 3. Errores De Runtime

Los errores de runtime ocurren despues de que empieza la preparacion de feeds o
durante la ejecucion.

Ejemplos:

- errores de alineacion de feeds
- feeds de runtime ausentes o duplicados
- agotamiento del presupuesto de instrucciones
- stack underflow
- type mismatch durante la ejecucion
- slot local o de serie invalido
- overflow de capacidad de historial
- output type mismatch durante la recoleccion de salidas

Los manifests y snapshots de sesiones paper tambien exponen mensajes de fallo por feed para que `paper-status` y `paper-export` muestren que feed fallo, en que etapa y con que error upstream.

## Propiedad De Cada Capa

La capa duena de un fallo forma parte del contrato:

- la validez sintactica y semantica pertenece a compilacion
- la validez de exchange/red/respuesta pertenece a carga de mercado
- la consistencia de feeds y la validez de la ejecucion pertenecen al runtime

PalmScript falla de forma explicita en vez de degradar la semantica en
silencio.
