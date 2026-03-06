# Composition and External Inputs

PalmScript supports host-managed composition where one compiled strategy can feed another.

## External Inputs

Downstream strategies receive upstream outputs as typed external inputs injected by the host or pipeline runtime.

From the script's point of view, external inputs behave like predefined root-scope series identifiers.

## Compile Environments

External inputs are declared outside the source file through `CompileEnvironment`. The CLI `check` and `dump-bytecode` commands can load that from `.palmscript.json`-style JSON, and the language server uses the same model for editor diagnostics.

## Pipeline Rules

The current pipeline runtime enforces:

- same-base-interval execution across nodes
- DAG topology
- explicit edge wiring from upstream output names to downstream input names
- type matching between outputs and external inputs

Pipelines do not change the semantics of the language; they only supply additional series values at execution time.
