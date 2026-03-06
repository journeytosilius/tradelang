# `.palmscript.json`

`.palmscript.json` is the workspace config file used by the IDE layer and the VS Code extension.

## Purpose

It lets editor tooling type-check files that depend on external series inputs provided by pipeline composition or host wiring.

## Schema

```json
{
  "version": 1,
  "documents": {
    "strategies/consumer.trl": {
      "compile_environment": {
        "external_inputs": [
          {
            "name": "trend",
            "ty": "SeriesBool",
            "kind": "ExportSeries"
          }
        ]
      }
    }
  }
}
```

## Rules

- document keys are workspace-relative file paths
- files without an entry use an empty compile environment
- `compile_environment` reuses the library's `CompileEnvironment` shape
- the VS Code extension watches this file and refreshes diagnostics on change
