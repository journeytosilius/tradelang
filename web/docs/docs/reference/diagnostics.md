# Diagnostics

PalmScript surfaces diagnostics and errors from three public layers.

## 1. Compile Diagnostics

Compile diagnostics are source-level failures with spans.

Diagnostic classes:

- lexical errors
- parse errors
- type and name-resolution errors
- compile-time structural errors

Examples:

- missing or duplicate `interval`
- unsupported `source` template
- unknown source alias
- undeclared `use` interval reference
- lower-than-base interval reference
- duplicate bindings
- invalid function recursion
- invalid builtin arity or argument type

These diagnostics surface through:

- the browser IDE editor diagnostics panel
- backtest requests issued by the hosted app

## 2. Market Fetch Errors

After successful compilation, runtime preparation may fail while assembling the required historical feeds.

Examples:

- the requested time window is invalid
- the script has no `source` declarations
- an exchange request fails
- a venue response is malformed
- a required feed returns no data in the requested window
- a symbol cannot be resolved by the selected venue

Fetch failures now include as much request context as PalmScript has at the failing layer, such as the requested window and the paper-feed bootstrap stage that triggered the request.

## 3. Runtime Errors

Runtime errors occur after feed preparation begins or during execution.

Examples:

- feed alignment errors
- missing or duplicate runtime feeds
- instruction-budget exhaustion
- stack underflow
- type mismatch during execution
- invalid local or series slot
- history-capacity overflow
- output type mismatch during output collection

Paper-session manifests and snapshots also surface per-feed failure messages so `paper-status` and `paper-export` can show which feed failed, at which stage, and with which upstream error string.

## Layer Ownership

The owning layer for a failure is part of the contract:

- syntax and semantic validity belong to compilation
- exchange/network/response validity belong to market fetch
- feed consistency and execution validity belong to runtime

PalmScript fails explicitly instead of silently degrading semantics.
