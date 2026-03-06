//! IDE-facing semantic analysis, formatting, and workspace configuration.
//!
//! This module exposes a stable, read-only API for editor tooling. It reuses
//! the compiler's parsing and semantic analysis passes instead of duplicating
//! language logic in the language server or editor extension.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ast::{Ast, Block, Expr, ExprKind, FunctionDecl, IntervalDecl, Stmt, StmtKind, UnaryOp};
use crate::builtins::BuiltinId;
use crate::compiler::{analyze_semantics, CompileEnvironment, ExprInfo, InferredType};
use crate::diagnostic::CompileError;
use crate::interval::{Interval, MarketField, INTERVAL_SPECS};
use crate::lexer;
use crate::parser;
use crate::span::Span;
use crate::types::Type;

const KEYWORD_COMPLETIONS: [(&str, &str); 11] = [
    ("interval", "Declare the strategy base interval"),
    ("use", "Declare an additional referenced interval"),
    ("fn", "Declare a top-level function"),
    ("let", "Bind a local value"),
    ("export", "Publish a named output series"),
    ("trigger", "Publish a named trigger series"),
    ("if", "Start a conditional block"),
    ("else", "Start an alternate conditional block"),
    ("and", "Logical conjunction"),
    ("or", "Logical disjunction"),
    ("na", "Missing value literal"),
];

const LITERAL_COMPLETIONS: [(&str, &str); 2] =
    [("true", "Boolean literal"), ("false", "Boolean literal")];

const PREDEFINED_SERIES: [(&str, &str); 6] = [
    ("open", "series<float> for the base-interval open"),
    ("high", "series<float> for the base-interval high"),
    ("low", "series<float> for the base-interval low"),
    ("close", "series<float> for the base-interval close"),
    ("volume", "series<float> for the base-interval volume"),
    (
        "time",
        "series<float> for the base-interval candle open time",
    ),
];

const MARKET_FIELDS: [(&str, &str); 6] = [
    ("open", "Open price"),
    ("high", "High price"),
    ("low", "Low price"),
    ("close", "Close price"),
    ("volume", "Volume"),
    ("time", "Candle open time"),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Interval,
    UseInterval,
    Function,
    Parameter,
    Let,
    Export,
    Trigger,
    ExternalInput,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub selection_span: Span,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionTarget {
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub selection_span: Span,
    pub detail: Option<String>,
    pub navigable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoverInfo {
    pub span: Span,
    pub contents: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSymbolInfo {
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub selection_span: Span,
    pub detail: Option<String>,
    pub children: Vec<DocumentSymbolInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompletionKind {
    Keyword,
    Builtin,
    Series,
    Interval,
    Field,
    Function,
    Variable,
    ExternalInput,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionEntry {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub version: u32,
    #[serde(default)]
    pub documents: BTreeMap<String, DocumentConfig>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentConfig {
    #[serde(default)]
    pub compile_environment: CompileEnvironment,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read `{path}`: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse `{path}`: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("unsupported TradeLang project config version `{version}`")]
    UnsupportedVersion { version: u32 },
}

#[derive(Clone, Debug)]
pub struct SemanticDocument {
    source: String,
    symbols: Vec<Symbol>,
    document_symbols: Vec<DocumentSymbolInfo>,
    definitions: Vec<DefinitionTarget>,
    references: Vec<Reference>,
}

#[derive(Clone, Debug)]
struct Reference {
    span: Span,
    definition_index: Option<usize>,
    hover: String,
}

#[derive(Clone, Debug)]
struct ResolutionContext<'a> {
    env: &'a CompileEnvironment,
    expr_info: &'a HashMap<u32, ExprInfo>,
    definitions: Vec<DefinitionTarget>,
    symbols: Vec<Symbol>,
    document_symbols: Vec<DocumentSymbolInfo>,
    references: Vec<Reference>,
    root_symbols: HashMap<String, usize>,
}

impl SemanticDocument {
    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    pub fn document_symbols(&self) -> &[DocumentSymbolInfo] {
        &self.document_symbols
    }

    pub fn definition_at(&self, offset: usize) -> Option<DefinitionTarget> {
        if let Some(reference) = self
            .references
            .iter()
            .find(|reference| span_contains(reference.span, offset))
        {
            return reference
                .definition_index
                .and_then(|index| self.definitions.get(index).cloned());
        }

        self.definitions
            .iter()
            .find(|definition| span_contains(definition.selection_span, offset))
            .cloned()
    }

    pub fn hover_at(&self, offset: usize) -> Option<HoverInfo> {
        if let Some(reference) = self
            .references
            .iter()
            .find(|reference| span_contains(reference.span, offset))
        {
            return Some(HoverInfo {
                span: reference.span,
                contents: reference.hover.clone(),
            });
        }

        self.definitions
            .iter()
            .find(|definition| span_contains(definition.selection_span, offset))
            .map(|definition| HoverInfo {
                span: definition.selection_span,
                contents: definition_hover(definition),
            })
    }

    pub fn completions_at(&self, offset: usize) -> Vec<CompletionEntry> {
        let mut items = BTreeMap::new();
        match completion_context(&self.source, offset) {
            CompletionContext::Field => {
                for (label, detail) in MARKET_FIELDS {
                    items.insert(
                        label.to_string(),
                        CompletionEntry {
                            label: label.to_string(),
                            kind: CompletionKind::Field,
                            detail: Some(detail.to_string()),
                        },
                    );
                }
            }
            CompletionContext::Interval => {
                for spec in INTERVAL_SPECS {
                    items.insert(
                        spec.text.to_string(),
                        CompletionEntry {
                            label: spec.text.to_string(),
                            kind: CompletionKind::Interval,
                            detail: Some("Binance-supported interval literal".to_string()),
                        },
                    );
                }
            }
            CompletionContext::General => {
                for (label, detail) in KEYWORD_COMPLETIONS {
                    items.insert(
                        label.to_string(),
                        CompletionEntry {
                            label: label.to_string(),
                            kind: CompletionKind::Keyword,
                            detail: Some(detail.to_string()),
                        },
                    );
                }
                for (label, detail) in LITERAL_COMPLETIONS {
                    items.insert(
                        label.to_string(),
                        CompletionEntry {
                            label: label.to_string(),
                            kind: CompletionKind::Keyword,
                            detail: Some(detail.to_string()),
                        },
                    );
                }
                for builtin in builtin_completions() {
                    items.insert(builtin.label.clone(), builtin);
                }
                for (label, detail) in PREDEFINED_SERIES {
                    items.insert(
                        label.to_string(),
                        CompletionEntry {
                            label: label.to_string(),
                            kind: CompletionKind::Series,
                            detail: Some(detail.to_string()),
                        },
                    );
                }
                for spec in INTERVAL_SPECS {
                    items
                        .entry(spec.text.to_string())
                        .or_insert(CompletionEntry {
                            label: spec.text.to_string(),
                            kind: CompletionKind::Interval,
                            detail: Some("Binance-supported interval literal".to_string()),
                        });
                }
                for definition in &self.definitions {
                    let kind = match definition.kind {
                        SymbolKind::Function => CompletionKind::Function,
                        SymbolKind::ExternalInput => CompletionKind::ExternalInput,
                        _ => CompletionKind::Variable,
                    };
                    items
                        .entry(definition.name.clone())
                        .or_insert(CompletionEntry {
                            label: definition.name.clone(),
                            kind,
                            detail: definition.detail.clone(),
                        });
                }
            }
        }

        items.into_values().collect()
    }
}

pub fn analyze_document(
    source: &str,
    env: &CompileEnvironment,
) -> Result<SemanticDocument, CompileError> {
    let (ast, analysis) = analyze_semantics(source, env)?;
    let mut context = ResolutionContext {
        env,
        expr_info: &analysis.expr_info,
        definitions: Vec::new(),
        symbols: Vec::new(),
        document_symbols: Vec::new(),
        references: Vec::new(),
        root_symbols: HashMap::new(),
    };
    build_semantic_document(&mut context, &ast);
    Ok(SemanticDocument {
        source: source.to_string(),
        symbols: context.symbols,
        document_symbols: context.document_symbols,
        definitions: context.definitions,
        references: context.references,
    })
}

pub fn format_document(source: &str) -> Result<String, CompileError> {
    let tokens = lexer::lex(source)?;
    let ast = parser::parse(&tokens)?;
    Ok(format_ast(&ast))
}

pub fn load_project_config(path: &Path) -> Result<ProjectConfig, ConfigError> {
    let raw = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let config: ProjectConfig =
        serde_json::from_str(&raw).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    if config.version != 1 {
        return Err(ConfigError::UnsupportedVersion {
            version: config.version,
        });
    }
    Ok(config)
}

impl ProjectConfig {
    pub fn compile_environment_for_document(
        &self,
        workspace_root: &Path,
        document_path: &Path,
    ) -> CompileEnvironment {
        let Ok(relative) = document_path.strip_prefix(workspace_root) else {
            return CompileEnvironment::default();
        };
        let key = normalize_relative_path(relative);
        self.documents
            .get(&key)
            .map(|config| config.compile_environment.clone())
            .unwrap_or_default()
    }
}

fn build_semantic_document(context: &mut ResolutionContext<'_>, ast: &Ast) {
    if let Some(base) = ast.strategy_intervals.base.first() {
        context.document_symbols.push(DocumentSymbolInfo {
            name: format!("interval {}", base.interval.as_str()),
            kind: SymbolKind::Interval,
            span: base.span,
            selection_span: base.span,
            detail: Some("Base execution interval".to_string()),
            children: Vec::new(),
        });
    }

    for use_decl in &ast.strategy_intervals.supplemental {
        context.document_symbols.push(document_interval_symbol(
            use_decl,
            SymbolKind::UseInterval,
            "Referenced interval",
        ));
    }

    for external in &context.env.external_inputs {
        let detail = format!(
            "external input: {} ({})",
            render_type(external.ty),
            external.kind.kind_name()
        );
        let index = push_definition(
            context,
            DefinitionTarget {
                name: external.name.clone(),
                kind: SymbolKind::ExternalInput,
                span: Span::default(),
                selection_span: Span::default(),
                detail: Some(detail),
                navigable: false,
            },
        );
        context.root_symbols.insert(external.name.clone(), index);
    }

    for function in &ast.functions {
        let detail = context
            .expr_info
            .get(&function.body.id)
            .map(render_expr_info)
            .unwrap_or_else(|| "unknown".to_string());
        let index = push_definition(
            context,
            DefinitionTarget {
                name: function.name.clone(),
                kind: SymbolKind::Function,
                span: function.span,
                selection_span: function.name_span,
                detail: Some(format!("fn {} -> {}", function.name, detail)),
                navigable: true,
            },
        );
        context.root_symbols.insert(function.name.clone(), index);
    }

    for function in &ast.functions {
        context
            .document_symbols
            .push(document_function_symbol(function, context));
    }

    for function in &ast.functions {
        resolve_function(context, function);
    }

    let mut root_scope = context.root_symbols.clone();
    for stmt in &ast.statements {
        resolve_stmt(context, stmt, &mut root_scope);
        maybe_push_top_level_symbol(context, stmt);
    }
}

fn resolve_function(context: &mut ResolutionContext<'_>, function: &FunctionDecl) {
    let mut scope = context.root_symbols.clone();
    for param in &function.params {
        let index = push_definition(
            context,
            DefinitionTarget {
                name: param.name.clone(),
                kind: SymbolKind::Parameter,
                span: param.span,
                selection_span: param.span,
                detail: Some("parameter".to_string()),
                navigable: true,
            },
        );
        scope.insert(param.name.clone(), index);
    }
    resolve_expr(context, &function.body, &scope);
}

fn resolve_stmt(
    context: &mut ResolutionContext<'_>,
    stmt: &Stmt,
    scope: &mut HashMap<String, usize>,
) {
    match &stmt.kind {
        StmtKind::Let {
            name,
            name_span,
            expr,
        } => {
            resolve_expr(context, expr, scope);
            let detail = context
                .expr_info
                .get(&expr.id)
                .map(render_expr_info)
                .unwrap_or_else(|| "unknown".to_string());
            let index = push_definition(
                context,
                DefinitionTarget {
                    name: name.clone(),
                    kind: SymbolKind::Let,
                    span: stmt.span,
                    selection_span: *name_span,
                    detail: Some(format!("let {}: {}", name, detail)),
                    navigable: true,
                },
            );
            scope.insert(name.clone(), index);
        }
        StmtKind::Export {
            name,
            name_span,
            expr,
        } => {
            resolve_expr(context, expr, scope);
            let detail = format!(
                "export {}: {}",
                name,
                render_output_type(expr, context, false)
            );
            let index = push_definition(
                context,
                DefinitionTarget {
                    name: name.clone(),
                    kind: SymbolKind::Export,
                    span: stmt.span,
                    selection_span: *name_span,
                    detail: Some(detail),
                    navigable: true,
                },
            );
            scope.insert(name.clone(), index);
        }
        StmtKind::Trigger {
            name,
            name_span,
            expr,
        } => {
            resolve_expr(context, expr, scope);
            let index = push_definition(
                context,
                DefinitionTarget {
                    name: name.clone(),
                    kind: SymbolKind::Trigger,
                    span: stmt.span,
                    selection_span: *name_span,
                    detail: Some("trigger series<bool>".to_string()),
                    navigable: true,
                },
            );
            scope.insert(name.clone(), index);
        }
        StmtKind::If {
            condition,
            then_block,
            else_block,
        } => {
            resolve_expr(context, condition, scope);
            let mut then_scope = scope.clone();
            resolve_block(context, then_block, &mut then_scope);
            let mut else_scope = scope.clone();
            resolve_block(context, else_block, &mut else_scope);
        }
        StmtKind::Expr(expr) => resolve_expr(context, expr, scope),
    }
}

fn resolve_block(
    context: &mut ResolutionContext<'_>,
    block: &Block,
    scope: &mut HashMap<String, usize>,
) {
    for stmt in &block.statements {
        resolve_stmt(context, stmt, scope);
    }
}

fn resolve_expr(context: &mut ResolutionContext<'_>, expr: &Expr, scope: &HashMap<String, usize>) {
    match &expr.kind {
        ExprKind::Ident(name) => {
            if let Some(index) = scope.get(name).copied() {
                let hover = definition_hover(&context.definitions[index]);
                context.references.push(Reference {
                    span: expr.span,
                    definition_index: Some(index),
                    hover,
                });
            } else if let Some((_label, detail)) =
                PREDEFINED_SERIES.iter().find(|(label, _)| *label == name)
            {
                context.references.push(Reference {
                    span: expr.span,
                    definition_index: None,
                    hover: format!("`{name}`\n\n{detail}"),
                });
            }
        }
        ExprKind::QualifiedSeries { interval, field } => {
            context.references.push(Reference {
                span: expr.span,
                definition_index: None,
                hover: format!(
                    "`{}.{}`\n\nseries<float> from the last fully closed {} candle {}.",
                    interval.as_str(),
                    render_market_field(*field),
                    interval.as_str(),
                    field_doc_suffix(*field)
                ),
            });
        }
        ExprKind::Unary { expr: inner, .. } => resolve_expr(context, inner, scope),
        ExprKind::Binary { left, right, .. } => {
            resolve_expr(context, left, scope);
            resolve_expr(context, right, scope);
        }
        ExprKind::Call {
            callee,
            callee_span,
            args,
        } => {
            let reference = if let Some(index) = scope.get(callee).copied() {
                Reference {
                    span: *callee_span,
                    definition_index: Some(index),
                    hover: definition_hover(&context.definitions[index]),
                }
            } else if let Some(builtin) = BuiltinId::from_name(callee) {
                Reference {
                    span: *callee_span,
                    definition_index: None,
                    hover: builtin_hover(builtin),
                }
            } else {
                Reference {
                    span: *callee_span,
                    definition_index: None,
                    hover: format!("`{callee}`"),
                }
            };
            context.references.push(reference);
            for arg in args {
                resolve_expr(context, arg, scope);
            }
        }
        ExprKind::Index { target, index } => {
            resolve_expr(context, target, scope);
            resolve_expr(context, index, scope);
        }
        ExprKind::Number(_) | ExprKind::Bool(_) | ExprKind::Na => {}
    }
}

fn maybe_push_top_level_symbol(context: &mut ResolutionContext<'_>, stmt: &Stmt) {
    match &stmt.kind {
        StmtKind::Let {
            name,
            name_span,
            expr,
        } => context.document_symbols.push(DocumentSymbolInfo {
            name: name.clone(),
            kind: SymbolKind::Let,
            span: stmt.span,
            selection_span: *name_span,
            detail: context.expr_info.get(&expr.id).map(render_expr_info),
            children: Vec::new(),
        }),
        StmtKind::Export {
            name,
            name_span,
            expr,
        } => context.document_symbols.push(DocumentSymbolInfo {
            name: name.clone(),
            kind: SymbolKind::Export,
            span: stmt.span,
            selection_span: *name_span,
            detail: Some(render_output_type(expr, context, false)),
            children: Vec::new(),
        }),
        StmtKind::Trigger {
            name, name_span, ..
        } => context.document_symbols.push(DocumentSymbolInfo {
            name: name.clone(),
            kind: SymbolKind::Trigger,
            span: stmt.span,
            selection_span: *name_span,
            detail: Some("series<bool>".to_string()),
            children: Vec::new(),
        }),
        StmtKind::If { .. } | StmtKind::Expr(_) => {}
    }
}

fn document_interval_symbol(
    decl: &IntervalDecl,
    kind: SymbolKind,
    detail: &str,
) -> DocumentSymbolInfo {
    DocumentSymbolInfo {
        name: decl.interval.as_str().to_string(),
        kind,
        span: decl.span,
        selection_span: decl.span,
        detail: Some(detail.to_string()),
        children: Vec::new(),
    }
}

fn document_function_symbol(
    function: &FunctionDecl,
    context: &ResolutionContext<'_>,
) -> DocumentSymbolInfo {
    let detail = context
        .expr_info
        .get(&function.body.id)
        .map(render_expr_info)
        .unwrap_or_else(|| "unknown".to_string());
    DocumentSymbolInfo {
        name: function.name.clone(),
        kind: SymbolKind::Function,
        span: function.span,
        selection_span: function.name_span,
        detail: Some(format!("fn -> {}", detail)),
        children: function
            .params
            .iter()
            .map(|param| DocumentSymbolInfo {
                name: param.name.clone(),
                kind: SymbolKind::Parameter,
                span: param.span,
                selection_span: param.span,
                detail: Some("parameter".to_string()),
                children: Vec::new(),
            })
            .collect(),
    }
}

fn push_definition(context: &mut ResolutionContext<'_>, definition: DefinitionTarget) -> usize {
    let index = context.definitions.len();
    context.symbols.push(Symbol {
        name: definition.name.clone(),
        kind: definition.kind.clone(),
        span: definition.span,
        selection_span: definition.selection_span,
        detail: definition.detail.clone(),
    });
    context.definitions.push(definition);
    index
}

fn render_output_type(expr: &Expr, context: &ResolutionContext<'_>, trigger: bool) -> String {
    if trigger {
        return "series<bool>".to_string();
    }
    context
        .expr_info
        .get(&expr.id)
        .map(|info| match info.ty {
            InferredType::Concrete(Type::Bool | Type::SeriesBool) => "series<bool>".to_string(),
            _ => "series<float>".to_string(),
        })
        .unwrap_or_else(|| "series<float>".to_string())
}

fn definition_hover(definition: &DefinitionTarget) -> String {
    match &definition.detail {
        Some(detail) => format!("`{}`\n\n{}", definition.name, detail),
        None => format!("`{}`", definition.name),
    }
}

fn builtin_hover(builtin: BuiltinId) -> String {
    match builtin {
        BuiltinId::Sma => "`sma(series, length)`\n\nSimple moving average.".to_string(),
        BuiltinId::Ema => "`ema(series, length)`\n\nExponential moving average.".to_string(),
        BuiltinId::Rsi => "`rsi(series, length)`\n\nRelative strength index.".to_string(),
        BuiltinId::Plot => "`plot(value)`\n\nEmit a plot output for the current bar.".to_string(),
        BuiltinId::Open => "`open`\n\nseries<float> for the base-interval open.".to_string(),
        BuiltinId::High => "`high`\n\nseries<float> for the base-interval high.".to_string(),
        BuiltinId::Low => "`low`\n\nseries<float> for the base-interval low.".to_string(),
        BuiltinId::Close => "`close`\n\nseries<float> for the base-interval close.".to_string(),
        BuiltinId::Volume => "`volume`\n\nseries<float> for the base-interval volume.".to_string(),
        BuiltinId::Time => {
            "`time`\n\nseries<float> for the base-interval candle open time.".to_string()
        }
    }
}

fn builtin_completions() -> Vec<CompletionEntry> {
    [
        ("sma", "sma(series, length)"),
        ("ema", "ema(series, length)"),
        ("rsi", "rsi(series, length)"),
        ("plot", "plot(value)"),
    ]
    .into_iter()
    .map(|(label, detail)| CompletionEntry {
        label: label.to_string(),
        kind: CompletionKind::Builtin,
        detail: Some(detail.to_string()),
    })
    .collect()
}

fn render_expr_info(info: &ExprInfo) -> String {
    match info.ty {
        InferredType::Concrete(ty) => render_type(ty).to_string(),
        InferredType::Na => "float".to_string(),
    }
}

fn render_type(ty: Type) -> &'static str {
    match ty {
        Type::F64 => "float",
        Type::Bool => "bool",
        Type::SeriesF64 => "series<float>",
        Type::SeriesBool => "series<bool>",
        Type::Void => "void",
    }
}

fn span_contains(span: Span, offset: usize) -> bool {
    span.start.offset <= offset && offset < span.end.offset.max(span.start.offset + 1)
}

fn field_doc_suffix(field: MarketField) -> &'static str {
    match field {
        MarketField::Open => "open value",
        MarketField::High => "high value",
        MarketField::Low => "low value",
        MarketField::Close => "close value",
        MarketField::Volume => "volume value",
        MarketField::Time => "open timestamp",
    }
}

fn normalize_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompletionContext {
    General,
    Interval,
    Field,
}

fn completion_context(source: &str, offset: usize) -> CompletionContext {
    let offset = offset.min(source.len());
    let before = &source[..offset];
    let token_start = before
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .map(|index| index + 1)
        .unwrap_or(0);
    let line_start = before.rfind('\n').map(|index| index + 1).unwrap_or(0);
    let line_before = &before[line_start..];
    let trimmed = line_before.trim_start();

    if token_start > 0 && before.as_bytes()[token_start - 1] == b'.' {
        let interval_start = before[..token_start - 1]
            .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .map(|index| index + 1)
            .unwrap_or(0);
        if Interval::parse(&before[interval_start..token_start - 1]).is_some() {
            return CompletionContext::Field;
        }
    }

    if trimmed.starts_with("interval") || trimmed.starts_with("use") {
        return CompletionContext::Interval;
    }

    CompletionContext::General
}

fn format_ast(ast: &Ast) -> String {
    let mut lines = Vec::new();
    if let Some(base) = ast.strategy_intervals.base.first() {
        lines.push(format!("interval {}", base.interval.as_str()));
    }
    for decl in &ast.strategy_intervals.supplemental {
        lines.push(format!("use {}", decl.interval.as_str()));
    }
    if !ast.strategy_intervals.base.is_empty() || !ast.strategy_intervals.supplemental.is_empty() {
        lines.push(String::new());
    }

    let mut first = true;
    for function in &ast.functions {
        if !first {
            lines.push(String::new());
        }
        first = false;
        let params = function
            .params
            .iter()
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "fn {}({}) = {}",
            function.name,
            params,
            format_expr(&function.body, 0)
        ));
    }

    if !ast.functions.is_empty() && !ast.statements.is_empty() {
        lines.push(String::new());
    }

    for (index, stmt) in ast.statements.iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        format_stmt(stmt, 0, &mut lines);
    }

    format!("{}\n", lines.join("\n"))
}

fn format_stmt(stmt: &Stmt, indent: usize, lines: &mut Vec<String>) {
    let prefix = "    ".repeat(indent);
    match &stmt.kind {
        StmtKind::Let { name, expr, .. } => {
            lines.push(format!("{prefix}let {name} = {}", format_expr(expr, 0)));
        }
        StmtKind::Export { name, expr, .. } => {
            lines.push(format!("{prefix}export {name} = {}", format_expr(expr, 0)));
        }
        StmtKind::Trigger { name, expr, .. } => {
            lines.push(format!("{prefix}trigger {name} = {}", format_expr(expr, 0)));
        }
        StmtKind::Expr(expr) => {
            lines.push(format!("{prefix}{}", format_expr(expr, 0)));
        }
        StmtKind::If {
            condition,
            then_block,
            else_block,
        } => format_if(condition, then_block, else_block, indent, lines),
    }
}

fn format_if(
    condition: &Expr,
    then_block: &Block,
    else_block: &Block,
    indent: usize,
    lines: &mut Vec<String>,
) {
    let prefix = "    ".repeat(indent);
    lines.push(format!("{prefix}if {} {{", format_expr(condition, 0)));
    format_block_body(then_block, indent + 1, lines);
    if let Some(nested_if) = else_if_stmt(else_block) {
        lines.push(format!(
            "{prefix}}} else if {} {{",
            format_expr(
                match &nested_if.kind {
                    StmtKind::If { condition, .. } => condition,
                    _ => unreachable!(),
                },
                0
            )
        ));
        if let StmtKind::If {
            then_block,
            else_block,
            ..
        } = &nested_if.kind
        {
            format_block_body(then_block, indent + 1, lines);
            format_else_tail(else_block, indent, lines);
        }
    } else {
        lines.push(format!("{prefix}}} else {{"));
        format_block_body(else_block, indent + 1, lines);
        lines.push(format!("{prefix}}}"));
    }
}

fn format_else_tail(block: &Block, indent: usize, lines: &mut Vec<String>) {
    let prefix = "    ".repeat(indent);
    if let Some(nested_if) = else_if_stmt(block) {
        lines.push(format!(
            "{prefix}}} else if {} {{",
            format_expr(
                match &nested_if.kind {
                    StmtKind::If { condition, .. } => condition,
                    _ => unreachable!(),
                },
                0
            )
        ));
        if let StmtKind::If {
            then_block,
            else_block,
            ..
        } = &nested_if.kind
        {
            format_block_body(then_block, indent + 1, lines);
            format_else_tail(else_block, indent, lines);
        }
    } else {
        lines.push(format!("{prefix}}} else {{"));
        format_block_body(block, indent + 1, lines);
        lines.push(format!("{prefix}}}"));
    }
}

fn format_block_body(block: &Block, indent: usize, lines: &mut Vec<String>) {
    for stmt in &block.statements {
        format_stmt(stmt, indent, lines);
    }
}

fn else_if_stmt(block: &Block) -> Option<&Stmt> {
    if block.statements.len() == 1 {
        match &block.statements[0].kind {
            StmtKind::If { .. } => Some(&block.statements[0]),
            _ => None,
        }
    } else {
        None
    }
}

fn format_expr(expr: &Expr, parent_bp: u8) -> String {
    match &expr.kind {
        ExprKind::Number(value) => trim_float(*value),
        ExprKind::Bool(value) => value.to_string(),
        ExprKind::Na => "na".to_string(),
        ExprKind::Ident(name) => name.clone(),
        ExprKind::QualifiedSeries { interval, field } => {
            format!("{}.{}", interval.as_str(), render_market_field(*field))
        }
        ExprKind::Unary { op, expr: inner } => {
            let operator = match op {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "!",
            };
            let rendered = format!("{operator}{}", format_expr(inner, 50));
            maybe_parenthesize(rendered, 50, parent_bp)
        }
        ExprKind::Binary { op, left, right } => {
            let (left_bp, right_bp, operator) = binary_precedence(op);
            let rendered = format!(
                "{} {} {}",
                format_expr(left, left_bp),
                operator,
                format_expr(right, right_bp)
            );
            maybe_parenthesize(rendered, left_bp, parent_bp)
        }
        ExprKind::Call { callee, args, .. } => {
            let args = args
                .iter()
                .map(|arg| format_expr(arg, 0))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{callee}({args})")
        }
        ExprKind::Index { target, index } => {
            format!("{}[{}]", format_expr(target, 60), format_expr(index, 0))
        }
    }
}

fn binary_precedence(op: &crate::ast::BinaryOp) -> (u8, u8, &'static str) {
    use crate::ast::BinaryOp;
    match op {
        BinaryOp::Or => (5, 6, "or"),
        BinaryOp::And => (7, 8, "and"),
        BinaryOp::Eq => (10, 11, "=="),
        BinaryOp::Ne => (10, 11, "!="),
        BinaryOp::Lt => (10, 11, "<"),
        BinaryOp::Le => (10, 11, "<="),
        BinaryOp::Gt => (10, 11, ">"),
        BinaryOp::Ge => (10, 11, ">="),
        BinaryOp::Add => (20, 21, "+"),
        BinaryOp::Sub => (20, 21, "-"),
        BinaryOp::Mul => (30, 31, "*"),
        BinaryOp::Div => (30, 31, "/"),
    }
}

fn maybe_parenthesize(rendered: String, bp: u8, parent_bp: u8) -> String {
    if bp < parent_bp {
        format!("({rendered})")
    } else {
        rendered
    }
}

fn trim_float(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

trait ExternalInputKindExt {
    fn kind_name(&self) -> &'static str;
}

impl ExternalInputKindExt for crate::ExternalInputKind {
    fn kind_name(&self) -> &'static str {
        match self {
            crate::ExternalInputKind::ExportSeries => "export series",
            crate::ExternalInputKind::TriggerSeries => "trigger series",
        }
    }
}

fn render_market_field(field: MarketField) -> &'static str {
    match field {
        MarketField::Open => "open",
        MarketField::High => "high",
        MarketField::Low => "low",
        MarketField::Close => "close",
        MarketField::Volume => "volume",
        MarketField::Time => "time",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{
        analyze_document, format_document, load_project_config, CompletionKind, ProjectConfig,
    };
    use crate::{CompileEnvironment, ExternalInputDecl, ExternalInputKind, Type};
    use tempfile::NamedTempFile;

    fn with_interval(source: &str) -> String {
        format!("interval 1m\n{source}")
    }

    #[test]
    fn semantic_document_contains_symbols_and_definitions() {
        let source = with_interval(
            "fn crossover(a, b) = a > b\nlet basis = ema(close, 5)\nexport trend = crossover(close, basis)\nif trend { plot(1) } else { plot(0) }",
        );
        let document = analyze_document(&source, &CompileEnvironment::default()).expect("semantic");
        assert!(document
            .symbols()
            .iter()
            .any(|symbol| symbol.name == "crossover"));
        let basis_offset = source.find("basis)").expect("basis ref");
        let definition = document.definition_at(basis_offset).expect("definition");
        assert_eq!(definition.name, "basis");
        let hover = document
            .hover_at(source.find("crossover(close").expect("call"))
            .expect("hover");
        assert!(hover.contents.contains("fn crossover"));
    }

    #[test]
    fn completions_include_keywords_builtins_and_fields() {
        let source = with_interval("plot(1w.)");
        let document = analyze_document(
            &source.replace("1w.", "close"),
            &CompileEnvironment::default(),
        )
        .expect("semantic");
        let general = document.completions_at(0);
        assert!(general.iter().any(|entry| entry.label == "interval"));
        assert!(general
            .iter()
            .any(|entry| entry.label == "ema" && entry.kind == CompletionKind::Builtin));
        let fields = document.completions_at(source.find('.').expect("dot") + 1);
        assert!(fields.iter().any(|entry| entry.label == "close"));
    }

    #[test]
    fn formatter_is_idempotent() {
        let source = "interval 1m\nfn crossover(a,b)=a>b\nif close>open{plot(1)}else{plot(0)}";
        let formatted = format_document(source).expect("formatted");
        let reformatted = format_document(&formatted).expect("reformatted");
        assert_eq!(formatted, reformatted);
    }

    #[test]
    fn project_config_loads_workspace_relative_envs() {
        let file = NamedTempFile::new().expect("temp file");
        fs::write(
            file.path(),
            r#"{
  "version": 1,
  "documents": {
    "strategies/consumer.trl": {
      "compile_environment": {
        "external_inputs": [
          { "name": "trend", "ty": "SeriesBool", "kind": "ExportSeries" }
        ]
      }
    }
  }
}"#,
        )
        .expect("write");
        let config = load_project_config(file.path()).expect("config");
        let env = config.compile_environment_for_document(
            Path::new("/workspace"),
            Path::new("/workspace/strategies/consumer.trl"),
        );
        assert_eq!(
            env,
            CompileEnvironment {
                external_inputs: vec![ExternalInputDecl {
                    name: "trend".to_string(),
                    ty: Type::SeriesBool,
                    kind: ExternalInputKind::ExportSeries,
                }],
            }
        );
        let _: ProjectConfig = config;
    }
}
