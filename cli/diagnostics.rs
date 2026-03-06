use std::path::Path;

use palmscript::{CompileError, DataPrepError, RuntimeError};

pub fn format_compile_error(path: &Path, err: &CompileError) -> String {
    let mut rendered = Vec::with_capacity(err.diagnostics.len() + 1);
    rendered.push(format!("compile failed for `{}`", path.display()));
    for diagnostic in &err.diagnostics {
        rendered.push(format!(
            "{}:{}:{}: {}: {}",
            path.display(),
            diagnostic.span.start.line,
            diagnostic.span.start.column,
            diagnostic_kind_label(diagnostic.kind.clone()),
            diagnostic.message
        ));
    }
    rendered.join("\n")
}

pub fn format_runtime_error(err: &RuntimeError) -> String {
    format!("runtime error: {err}")
}

pub fn format_data_prep_error(err: &DataPrepError) -> String {
    format!("CSV mode error: {err}")
}

fn diagnostic_kind_label(kind: palmscript::DiagnosticKind) -> &'static str {
    match kind {
        palmscript::DiagnosticKind::Lex => "lex",
        palmscript::DiagnosticKind::Parse => "parse",
        palmscript::DiagnosticKind::Type => "type",
        palmscript::DiagnosticKind::Compile => "compile",
    }
}
