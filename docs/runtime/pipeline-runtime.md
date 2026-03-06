# Pipeline Runtime

PalmScript supports host-managed pipelines over multiple compiled programs.

## What The Pipeline Owns

The pipeline runtime is responsible for:

- node ordering
- edge validation
- external-input binding
- same-bar propagation of upstream outputs

## Current Constraints

The current runtime enforces:

- same base interval across all nodes
- acyclic graphs
- complete input wiring
- output/input kind and type compatibility

## Same-Bar Visibility

Downstream nodes can observe same-bar upstream outputs only when the upstream node runs earlier in the pipeline's validated topological order.

## What The Pipeline Does Not Change

The pipeline runtime does not create new language semantics. Each node still runs as an ordinary PalmScript program with the same deterministic VM and runtime rules.
