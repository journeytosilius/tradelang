//! IDE-facing semantic analysis, formatting, and workspace configuration.
//!
//! This module exposes a stable, read-only API for editor tooling. It reuses
//! the compiler's parsing and semantic analysis passes instead of duplicating
//! language logic in the language server or editor extension.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::ast::{
    Ast, Block, Expr, ExprKind, FunctionDecl, InputOptimizationKind, RiskControlKind,
    SourceIntervalDecl, Stmt, StmtKind, UnaryOp,
};
use crate::builtins::BuiltinId;
use crate::compiler::{analyze_semantics, ExprInfo, InferredType};
use crate::diagnostic::CompileError;
use crate::interval::{Interval, MarketField, INTERVAL_SPECS};
use crate::lexer;
use crate::parser;
use crate::span::Span;
use crate::talib::{metadata_by_name as talib_metadata_by_name, TALIB_METADATA_SNAPSHOT};
use crate::token::{Token, TokenKind};
use crate::types::Type;

const KEYWORD_COMPLETIONS: [(&str, &str); 29] = [
    ("interval", "Declare the strategy base interval"),
    ("source", "Declare a named market source"),
    ("execution", "Declare a named execution venue"),
    ("use", "Declare an additional referenced interval"),
    ("fn", "Declare a top-level function"),
    ("let", "Bind a local value"),
    ("module", "Label an entry role for per-module attribution"),
    ("order_template", "Declare a reusable named order template"),
    ("arb_entry", "Declare an arbitrage basket entry trigger"),
    ("arb_exit", "Declare an arbitrage basket exit trigger"),
    ("arb_order", "Declare an arbitrage pair-order template"),
    ("transfer", "Declare a first-class inter-ledger transfer"),
    ("optimize", "Attach optimizer metadata to an input"),
    ("export", "Publish a named output series"),
    ("regime", "Declare a named persistent regime series"),
    ("trigger", "Publish a named trigger series"),
    (
        "cooldown",
        "Block same-side re-entry for a fixed number of bars after exit",
    ),
    (
        "max_bars_in_trade",
        "Force a same-side exit after a fixed number of bars",
    ),
    (
        "max_positions",
        "Limit total simultaneous open execution-source positions",
    ),
    (
        "max_long_positions",
        "Limit simultaneous long execution-source positions",
    ),
    (
        "max_short_positions",
        "Limit simultaneous short execution-source positions",
    ),
    (
        "max_gross_exposure_pct",
        "Limit portfolio gross notional exposure relative to equity",
    ),
    (
        "max_net_exposure_pct",
        "Limit portfolio net notional exposure relative to equity",
    ),
    (
        "portfolio_group",
        "Declare a named execution-source alias group for portfolio diagnostics",
    ),
    ("if", "Start a conditional block"),
    ("else", "Start an alternate conditional block"),
    ("and", "Logical conjunction"),
    ("or", "Logical disjunction"),
    ("na", "Missing value literal"),
];

const LITERAL_COMPLETIONS: [(&str, &str); 2] =
    [("true", "Boolean literal"), ("false", "Boolean literal")];

const MARKET_FIELDS: [(&str, &str); 6] = [
    ("open", "Open price"),
    ("high", "High price"),
    ("low", "Low price"),
    ("close", "Close price"),
    ("volume", "Volume"),
    ("time", "Candle open time"),
];

const BINANCE_USDM_AUXILIARY_FIELDS: [(&str, &str); 5] = [
    (
        "funding_rate",
        "Latest historical Binance USD-M funding rate",
    ),
    ("mark_price", "Binance USD-M mark-price close value"),
    ("index_price", "Binance USD-M index-price close value"),
    ("premium_index", "Binance USD-M premium-index close value"),
    ("basis", "Historical Binance USD-M basis snapshot"),
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Interval,
    Source,
    Execution,
    UseInterval,
    Function,
    Parameter,
    Let,
    Export,
    Trigger,
    OrderTemplate,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompletionKind {
    Keyword,
    Builtin,
    Series,
    Source,
    Interval,
    Field,
    Function,
    Variable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionInsertTextFormat {
    PlainText,
    Snippet,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionEntry {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: String,
    pub insert_text_format: CompletionInsertTextFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HighlightKind {
    Keyword,
    String,
    Number,
    Function,
    Variable,
    Parameter,
    Namespace,
    Type,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightToken {
    pub span: Span,
    pub kind: HighlightKind,
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
        completions_for_source(&self.source, offset, Some(&self.definitions))
    }
}

pub fn complete_document(source: &str, offset: usize) -> Vec<CompletionEntry> {
    match analyze_document(source) {
        Ok(semantic) => semantic.completions_at(offset),
        Err(_) => completions_for_source(source, offset, fallback_definitions(source).as_deref()),
    }
}

pub fn analyze_document(source: &str) -> Result<SemanticDocument, CompileError> {
    let (ast, analysis) = analyze_semantics(source)?;
    let mut context = ResolutionContext {
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

pub fn highlight_document(source: &str) -> Vec<HighlightToken> {
    let tokens = match lexer::lex(source) {
        Ok(tokens) => tokens,
        Err(_) => return Vec::new(),
    };
    let semantic = analyze_document(source).ok();

    tokens
        .into_iter()
        .filter_map(|token| {
            classify_highlight(&token, semantic.as_ref()).map(|kind| HighlightToken {
                span: token.span,
                kind,
            })
        })
        .collect()
}

pub fn format_document(source: &str) -> Result<String, CompileError> {
    let tokens = lexer::lex(source)?;
    let ast = parser::parse(&tokens)?;
    Ok(format_ast(&ast))
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

    for source in &ast.strategy_intervals.sources {
        let index = push_definition(
            context,
            DefinitionTarget {
                name: source.alias.clone(),
                kind: SymbolKind::Source,
                span: source.span,
                selection_span: source.alias_span,
                detail: Some(format!(
                    "{}(\"{}\")",
                    source.template.as_str(),
                    source.symbol
                )),
                navigable: true,
            },
        );
        context.root_symbols.insert(source.alias.clone(), index);
        context.document_symbols.push(DocumentSymbolInfo {
            name: source.alias.clone(),
            kind: SymbolKind::Source,
            span: source.span,
            selection_span: source.alias_span,
            detail: Some(format!(
                "{}(\"{}\")",
                source.template.as_str(),
                source.symbol
            )),
            children: Vec::new(),
        });
    }

    for execution in &ast.strategy_intervals.executions {
        let index = push_definition(
            context,
            DefinitionTarget {
                name: execution.alias.clone(),
                kind: SymbolKind::Execution,
                span: execution.span,
                selection_span: execution.alias_span,
                detail: Some(format!(
                    "{}(\"{}\")",
                    execution.template.as_str(),
                    execution.symbol
                )),
                navigable: true,
            },
        );
        context.root_symbols.insert(execution.alias.clone(), index);
        context.document_symbols.push(DocumentSymbolInfo {
            name: execution.alias.clone(),
            kind: SymbolKind::Execution,
            span: execution.span,
            selection_span: execution.alias_span,
            detail: Some(format!(
                "{}(\"{}\")",
                execution.template.as_str(),
                execution.symbol
            )),
            children: Vec::new(),
        });
    }

    for use_decl in &ast.strategy_intervals.supplemental {
        context
            .document_symbols
            .push(document_source_interval_symbol(
                use_decl,
                SymbolKind::UseInterval,
                "Referenced interval",
            ));
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

    for stmt in &ast.statements {
        let StmtKind::OrderTemplate {
            name,
            name_span,
            spec,
        } = &stmt.kind
        else {
            continue;
        };
        let index = push_definition(
            context,
            DefinitionTarget {
                name: name.clone(),
                kind: SymbolKind::OrderTemplate,
                span: stmt.span,
                selection_span: *name_span,
                detail: Some(format!(
                    "order_template {} = {}",
                    name,
                    format_order_spec(spec)
                )),
                navigable: true,
            },
        );
        context.root_symbols.insert(name.clone(), index);
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
        StmtKind::Const {
            name,
            name_span,
            expr,
        }
        | StmtKind::Input {
            name,
            name_span,
            expr,
            ..
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
                    detail: Some(detail),
                    navigable: true,
                },
            );
            scope.insert(name.clone(), index);
        }
        StmtKind::LetTuple { names, expr } => {
            resolve_expr(context, expr, scope);
            let detail = context
                .expr_info
                .get(&expr.id)
                .map(render_expr_info)
                .unwrap_or_else(|| "unknown".to_string());
            for binding in names {
                let index = push_definition(
                    context,
                    DefinitionTarget {
                        name: binding.name.clone(),
                        kind: SymbolKind::Let,
                        span: stmt.span,
                        selection_span: binding.span,
                        detail: Some(format!("let {}: {}", binding.name, detail)),
                        navigable: true,
                    },
                );
                scope.insert(binding.name.clone(), index);
            }
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
        StmtKind::Regime {
            name,
            name_span,
            expr,
        } => {
            resolve_expr(context, expr, scope);
            let index = push_definition(
                context,
                DefinitionTarget {
                    name: name.clone(),
                    kind: SymbolKind::Export,
                    span: stmt.span,
                    selection_span: *name_span,
                    detail: Some("regime series<bool>".to_string()),
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
        StmtKind::Signal { expr, .. } => {
            resolve_expr(context, expr, scope);
        }
        StmtKind::ArbSignal { expr, .. } => {
            resolve_expr(context, expr, scope);
        }
        StmtKind::OrderTemplate { spec, .. } => resolve_order_spec(context, spec, scope),
        StmtKind::Order { spec, .. } => {
            resolve_order_spec(context, spec, scope);
        }
        StmtKind::ArbOrder { spec, .. } => {
            resolve_arb_order_spec(context, spec, scope);
        }
        StmtKind::Transfer { spec, .. } => {
            resolve_expr(context, &spec.from, scope);
            resolve_expr(context, &spec.to, scope);
            resolve_expr(context, &spec.amount, scope);
            if let Some(expr) = &spec.fee {
                resolve_expr(context, expr, scope);
            }
            if let Some(expr) = &spec.delay_bars {
                resolve_expr(context, expr, scope);
            }
        }
        StmtKind::OrderSize { expr, .. } => {
            resolve_expr(context, expr, scope);
        }
        StmtKind::RiskControl { expr, .. } => {
            resolve_expr(context, expr, scope);
        }
        StmtKind::PortfolioControl { expr, .. } => {
            resolve_expr(context, expr, scope);
        }
        StmtKind::PortfolioGroup { .. } | StmtKind::Module { .. } => {}
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
            }
        }
        ExprKind::SourceSeries {
            source,
            source_span,
            interval,
            field,
        } => {
            let definition_index = scope.get(source).copied();
            context.references.push(Reference {
                span: *source_span,
                definition_index,
                hover: definition_index
                    .map(|index| definition_hover(&context.definitions[index]))
                    .unwrap_or_else(|| format!("`{source}`")),
            });
            let label = interval
                .map(|interval| {
                    format!(
                        "{source}.{}.{}",
                        interval.as_str(),
                        render_market_field(*field)
                    )
                })
                .unwrap_or_else(|| format!("{source}.{}", render_market_field(*field)));
            context.references.push(Reference {
                span: expr.span,
                definition_index: None,
                hover: format!("`{label}`\n\nseries<float> for the declared source market field."),
            });
        }
        ExprKind::EnumVariant {
            namespace, variant, ..
        } => {
            context.references.push(Reference {
                span: expr.span,
                definition_index: None,
                hover: format!("`{}.{}`\n\nTyped enum literal.", namespace, variant),
            });
        }
        ExprKind::PositionField { field, .. } => {
            context.references.push(Reference {
                span: expr.span,
                definition_index: None,
                hover: format!(
                    "`position.{}`\n\nAttached-exit position field.",
                    field.as_str()
                ),
            });
        }
        ExprKind::PositionEventField { field, .. } => {
            context.references.push(Reference {
                span: expr.span,
                definition_index: None,
                hover: format!(
                    "`position_event.{}`\n\nBacktest-driven position fill event.",
                    field.as_str()
                ),
            });
        }
        ExprKind::LastExitField { scope, field, .. } => {
            context.references.push(Reference {
                span: expr.span,
                definition_index: None,
                hover: format!(
                    "`{}.{}`\n\nBacktest-driven latest closed-trade field.",
                    scope.namespace(),
                    field.as_str()
                ),
            });
        }
        ExprKind::LedgerField {
            execution_alias,
            field,
            ..
        } => {
            context.references.push(Reference {
                span: expr.span,
                definition_index: None,
                hover: format!(
                    "`ledger({}).{}`\n\nBacktest-driven execution-ledger field.",
                    execution_alias,
                    field.as_str()
                ),
            });
        }
        ExprKind::Unary { expr: inner, .. } => resolve_expr(context, inner, scope),
        ExprKind::Binary { left, right, .. } => {
            resolve_expr(context, left, scope);
            resolve_expr(context, right, scope);
        }
        ExprKind::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            resolve_expr(context, condition, scope);
            resolve_expr(context, when_true, scope);
            resolve_expr(context, when_false, scope);
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
            } else if let Some(metadata) = talib_metadata_by_name(callee) {
                Reference {
                    span: *callee_span,
                    definition_index: None,
                    hover: talib_hover(metadata),
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
        ExprKind::Number(_) | ExprKind::Bool(_) | ExprKind::Na | ExprKind::String(_) => {}
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
        StmtKind::Const {
            name,
            name_span,
            expr,
        }
        | StmtKind::Input {
            name,
            name_span,
            expr,
            ..
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
        StmtKind::Regime {
            name, name_span, ..
        } => context.document_symbols.push(DocumentSymbolInfo {
            name: name.clone(),
            kind: SymbolKind::Export,
            span: stmt.span,
            selection_span: *name_span,
            detail: Some("series<bool>".to_string()),
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
        StmtKind::OrderTemplate {
            name,
            name_span,
            spec,
        } => context.document_symbols.push(DocumentSymbolInfo {
            name: name.clone(),
            kind: SymbolKind::OrderTemplate,
            span: stmt.span,
            selection_span: *name_span,
            detail: Some(format_order_spec(spec)),
            children: Vec::new(),
        }),
        StmtKind::Signal { .. } => {}
        StmtKind::ArbSignal { kind, .. } => context.document_symbols.push(DocumentSymbolInfo {
            name: match kind {
                crate::ast::ArbSignalKind::Entry => "arb_entry".to_string(),
                crate::ast::ArbSignalKind::Exit => "arb_exit".to_string(),
            },
            kind: SymbolKind::Trigger,
            span: stmt.span,
            selection_span: stmt.span,
            detail: Some("series<bool>".to_string()),
            children: Vec::new(),
        }),
        StmtKind::Order { .. } => {}
        StmtKind::ArbOrder { .. } => {}
        StmtKind::Transfer { .. } => {}
        StmtKind::OrderSize { .. } => {}
        StmtKind::RiskControl { .. } => {}
        StmtKind::PortfolioControl { .. } => {}
        StmtKind::PortfolioGroup { .. } => {}
        StmtKind::Module { module } => context.document_symbols.push(DocumentSymbolInfo {
            name: module.name.clone(),
            kind: SymbolKind::Let,
            span: stmt.span,
            selection_span: module.name_span,
            detail: Some(format!(
                "module {}",
                format_signal_role_surface(module.role)
            )),
            children: Vec::new(),
        }),
        StmtKind::LetTuple { names, expr } => {
            let detail = context.expr_info.get(&expr.id).map(render_expr_info);
            for binding in names {
                context.document_symbols.push(DocumentSymbolInfo {
                    name: binding.name.clone(),
                    kind: SymbolKind::Let,
                    span: stmt.span,
                    selection_span: binding.span,
                    detail: detail.clone(),
                    children: Vec::new(),
                });
            }
        }
        StmtKind::If { .. } | StmtKind::Expr(_) => {}
    }
}

fn document_source_interval_symbol(
    decl: &SourceIntervalDecl,
    kind: SymbolKind,
    detail: &str,
) -> DocumentSymbolInfo {
    DocumentSymbolInfo {
        name: if decl.source.is_empty() {
            decl.interval.as_str().to_string()
        } else {
            format!("{} {}", decl.source, decl.interval.as_str())
        },
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
            InferredType::Tuple2(_) | InferredType::Tuple3(_) => "tuple".to_string(),
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
    format!("`{}`\n\n{}", builtin.signature(), builtin.summary())
}

fn builtin_completions() -> Vec<CompletionEntry> {
    let mut entries: BTreeMap<String, CompletionEntry> = BuiltinId::CALLABLE
        .into_iter()
        .map(|builtin| CompletionEntry {
            label: builtin.as_str().to_string(),
            kind: CompletionKind::Builtin,
            detail: Some(builtin.signature().to_string()),
            documentation: Some(builtin_hover(builtin)),
            insert_text: completion_insert_text(
                builtin.as_str(),
                CompletionKind::Builtin,
                Some(builtin.signature()),
            ),
            insert_text_format: completion_insert_text_format(
                CompletionKind::Builtin,
                Some(builtin.signature()),
            ),
        })
        .map(|entry| (entry.label.clone(), entry))
        .collect();
    for metadata in TALIB_METADATA_SNAPSHOT {
        entries
            .entry(metadata.name.to_string())
            .or_insert(CompletionEntry {
                label: metadata.name.to_string(),
                kind: CompletionKind::Builtin,
                detail: Some(metadata.signature.to_string()),
                documentation: Some(talib_hover(metadata)),
                insert_text: completion_insert_text(
                    metadata.name,
                    CompletionKind::Builtin,
                    Some(metadata.signature),
                ),
                insert_text_format: completion_insert_text_format(
                    CompletionKind::Builtin,
                    Some(metadata.signature),
                ),
            });
    }
    entries.into_values().collect()
}

fn completions_for_source(
    source: &str,
    offset: usize,
    definitions: Option<&[DefinitionTarget]>,
) -> Vec<CompletionEntry> {
    let mut items = BTreeMap::new();
    match completion_context_for(source, offset, definitions) {
        CompletionContext::Field { source_alias } => {
            for (label, detail) in market_fields_for_alias(source, source_alias, definitions) {
                items.insert(
                    label.to_string(),
                    plain_completion(label, CompletionKind::Field, detail),
                );
            }
        }
        CompletionContext::Interval => {
            for spec in INTERVAL_SPECS {
                items.insert(spec.text.to_string(), interval_completion(spec.text));
            }
        }
        CompletionContext::General => {
            for (label, detail) in KEYWORD_COMPLETIONS {
                items.insert(
                    label.to_string(),
                    plain_completion(label, CompletionKind::Keyword, detail),
                );
            }
            for (label, detail) in LITERAL_COMPLETIONS {
                items.insert(
                    label.to_string(),
                    plain_completion(label, CompletionKind::Keyword, detail),
                );
            }
            for builtin in builtin_completions() {
                items.insert(builtin.label.clone(), builtin);
            }
            for spec in INTERVAL_SPECS {
                items
                    .entry(spec.text.to_string())
                    .or_insert_with(|| interval_completion(spec.text));
            }
            if let Some(definitions) = definitions {
                for definition in definitions {
                    let kind = match definition.kind {
                        SymbolKind::Function => CompletionKind::Function,
                        SymbolKind::Source | SymbolKind::Execution => CompletionKind::Source,
                        _ => CompletionKind::Variable,
                    };
                    items
                        .entry(definition.name.clone())
                        .or_insert(CompletionEntry {
                            label: definition.name.clone(),
                            kind,
                            detail: definition.detail.clone(),
                            documentation: Some(definition_hover(definition)),
                            insert_text: completion_insert_text(
                                &definition.name,
                                kind,
                                definition.detail.as_deref(),
                            ),
                            insert_text_format: completion_insert_text_format(
                                kind,
                                definition.detail.as_deref(),
                            ),
                        });
                }
            }
        }
    }

    items.into_values().collect()
}

fn fallback_definitions(source: &str) -> Option<Vec<DefinitionTarget>> {
    let tokens = lexer::lex(source).ok()?;
    let ast = parser::parse(&tokens).ok()?;
    Some(collect_definition_fallbacks(&ast))
}

fn collect_definition_fallbacks(ast: &Ast) -> Vec<DefinitionTarget> {
    let mut definitions = Vec::new();

    for source in &ast.strategy_intervals.sources {
        definitions.push(DefinitionTarget {
            name: source.alias.clone(),
            kind: SymbolKind::Source,
            span: source.span,
            selection_span: source.alias_span,
            detail: Some(format!(
                "{}(\"{}\")",
                source.template.as_str(),
                source.symbol
            )),
            navigable: true,
        });
    }
    for execution in &ast.strategy_intervals.executions {
        definitions.push(DefinitionTarget {
            name: execution.alias.clone(),
            kind: SymbolKind::Execution,
            span: execution.span,
            selection_span: execution.alias_span,
            detail: Some(format!(
                "{}(\"{}\")",
                execution.template.as_str(),
                execution.symbol
            )),
            navigable: true,
        });
    }

    for function in &ast.functions {
        let params = function
            .params
            .iter()
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        definitions.push(DefinitionTarget {
            name: function.name.clone(),
            kind: SymbolKind::Function,
            span: function.span,
            selection_span: function.name_span,
            detail: Some(format!("fn {}({})", function.name, params)),
            navigable: true,
        });
    }

    for stmt in &ast.statements {
        match &stmt.kind {
            StmtKind::Let {
                name, name_span, ..
            }
            | StmtKind::Const {
                name, name_span, ..
            }
            | StmtKind::Input {
                name, name_span, ..
            } => definitions.push(DefinitionTarget {
                name: name.clone(),
                kind: SymbolKind::Let,
                span: stmt.span,
                selection_span: *name_span,
                detail: None,
                navigable: true,
            }),
            StmtKind::LetTuple { names, .. } => {
                for binding in names {
                    definitions.push(DefinitionTarget {
                        name: binding.name.clone(),
                        kind: SymbolKind::Let,
                        span: stmt.span,
                        selection_span: binding.span,
                        detail: None,
                        navigable: true,
                    });
                }
            }
            StmtKind::Export {
                name, name_span, ..
            } => definitions.push(DefinitionTarget {
                name: name.clone(),
                kind: SymbolKind::Export,
                span: stmt.span,
                selection_span: *name_span,
                detail: Some(format!("export {}", name)),
                navigable: true,
            }),
            StmtKind::Regime {
                name, name_span, ..
            } => definitions.push(DefinitionTarget {
                name: name.clone(),
                kind: SymbolKind::Export,
                span: stmt.span,
                selection_span: *name_span,
                detail: Some("regime series<bool>".to_string()),
                navigable: true,
            }),
            StmtKind::Trigger {
                name, name_span, ..
            } => definitions.push(DefinitionTarget {
                name: name.clone(),
                kind: SymbolKind::Trigger,
                span: stmt.span,
                selection_span: *name_span,
                detail: Some("trigger series<bool>".to_string()),
                navigable: true,
            }),
            _ => {}
        }
    }

    definitions
}

fn plain_completion(label: &str, kind: CompletionKind, detail: &str) -> CompletionEntry {
    CompletionEntry {
        label: label.to_string(),
        kind,
        detail: Some(detail.to_string()),
        documentation: Some(detail.to_string()),
        insert_text: label.to_string(),
        insert_text_format: CompletionInsertTextFormat::PlainText,
    }
}

fn interval_completion(label: &str) -> CompletionEntry {
    CompletionEntry {
        label: label.to_string(),
        kind: CompletionKind::Interval,
        detail: Some("Binance-supported interval literal".to_string()),
        documentation: Some(format!(
            "`{}`\n\nBinance-supported interval literal.",
            label
        )),
        insert_text: label.to_string(),
        insert_text_format: CompletionInsertTextFormat::PlainText,
    }
}

fn completion_insert_text(label: &str, kind: CompletionKind, detail: Option<&str>) -> String {
    completion_snippet(label, kind, detail).unwrap_or_else(|| label.to_string())
}

fn completion_insert_text_format(
    kind: CompletionKind,
    detail: Option<&str>,
) -> CompletionInsertTextFormat {
    if completion_snippet_supported(kind, detail) {
        CompletionInsertTextFormat::Snippet
    } else {
        CompletionInsertTextFormat::PlainText
    }
}

fn completion_snippet_supported(kind: CompletionKind, detail: Option<&str>) -> bool {
    match kind {
        CompletionKind::Builtin => detail.and_then(signature_snippet_name).is_some(),
        CompletionKind::Function => true,
        _ => false,
    }
}

fn completion_snippet(label: &str, kind: CompletionKind, detail: Option<&str>) -> Option<String> {
    match kind {
        CompletionKind::Builtin => detail.and_then(|signature| signature_snippet(label, signature)),
        CompletionKind::Function => Some(format!("{label}($0)")),
        _ => None,
    }
}

fn signature_snippet(label: &str, signature: &str) -> Option<String> {
    if label.is_empty() {
        return signature_snippet_name(signature);
    }
    let (name, args) = parse_signature(signature)?;
    if name != label {
        return None;
    }
    Some(build_signature_snippet(name, args))
}

fn signature_snippet_name(signature: &str) -> Option<String> {
    let (name, args) = parse_signature(signature)?;
    Some(build_signature_snippet(name, args))
}

fn parse_signature(signature: &str) -> Option<(&str, Vec<&str>)> {
    let open = signature.find('(')?;
    let close = signature.rfind(')')?;
    let name = signature[..open].trim();
    let args = signature[open + 1..close]
        .split(',')
        .map(str::trim)
        .filter(|arg| !arg.is_empty())
        .collect::<Vec<_>>();
    Some((name, args))
}

fn build_signature_snippet(name: &str, args: Vec<&str>) -> String {
    if args.is_empty() {
        return format!("{name}($0)");
    }

    let placeholders = args
        .into_iter()
        .enumerate()
        .map(|(index, arg)| {
            let cleaned = arg.trim_matches(|ch| ch == '[' || ch == ']').trim();
            let placeholder = cleaned
                .split_once('=')
                .map(|(_, value)| value.trim())
                .unwrap_or(cleaned);
            format!("${{{}:{}}}", index + 1, placeholder)
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{name}({placeholders})")
}

fn talib_hover(metadata: &crate::talib::TalibFunctionMetadata) -> String {
    format!("`{}`\n\n{}", metadata.signature, metadata.summary)
}

pub(crate) fn classify_highlight(
    token: &Token,
    semantic: Option<&SemanticDocument>,
) -> Option<HighlightKind> {
    match &token.kind {
        TokenKind::Fn
        | TokenKind::Let
        | TokenKind::Const
        | TokenKind::Input
        | TokenKind::Order
        | TokenKind::Module
        | TokenKind::OrderTemplate
        | TokenKind::IntervalKw
        | TokenKind::Source
        | TokenKind::Execution
        | TokenKind::Use
        | TokenKind::Export
        | TokenKind::Regime
        | TokenKind::Trigger
        | TokenKind::Entry
        | TokenKind::Exit
        | TokenKind::Protect
        | TokenKind::Target
        | TokenKind::Size
        | TokenKind::Long
        | TokenKind::Short
        | TokenKind::If
        | TokenKind::Else
        | TokenKind::And
        | TokenKind::Or
        | TokenKind::Optimize
        | TokenKind::Cooldown
        | TokenKind::MaxBarsInTrade
        | TokenKind::True
        | TokenKind::False
        | TokenKind::Na => Some(HighlightKind::Keyword),
        TokenKind::String(_) => Some(HighlightKind::String),
        TokenKind::Number(_) => Some(HighlightKind::Number),
        TokenKind::Interval(_) => Some(HighlightKind::Type),
        TokenKind::Ident(text) => {
            if BuiltinId::from_name(text).is_some() || talib_metadata_by_name(text).is_some() {
                return Some(HighlightKind::Function);
            }

            semantic
                .and_then(|semantic| semantic.definition_at(token.span.start.offset))
                .map(|definition| match definition.kind {
                    SymbolKind::Function => HighlightKind::Function,
                    SymbolKind::Parameter => HighlightKind::Parameter,
                    SymbolKind::Source
                    | SymbolKind::Execution
                    | SymbolKind::UseInterval
                    | SymbolKind::Interval => HighlightKind::Namespace,
                    SymbolKind::Let
                    | SymbolKind::Export
                    | SymbolKind::Trigger
                    | SymbolKind::OrderTemplate => HighlightKind::Variable,
                })
                .or(Some(HighlightKind::Variable))
        }
        _ => None,
    }
}

fn render_expr_info(info: &ExprInfo) -> String {
    match info.ty {
        InferredType::Concrete(ty) => render_type(ty).to_string(),
        InferredType::Tuple2(items) => {
            format!("({}, {})", render_type(items[0]), render_type(items[1]))
        }
        InferredType::Tuple3(items) => format!(
            "({}, {}, {})",
            render_type(items[0]),
            render_type(items[1]),
            render_type(items[2])
        ),
        InferredType::Na => "float".to_string(),
    }
}

fn render_type(ty: Type) -> &'static str {
    match ty {
        Type::F64 => "float",
        Type::Bool => "bool",
        Type::MaType => "ma_type",
        Type::TimeInForce => "tif",
        Type::TriggerReference => "trigger_ref",
        Type::ExecutionAlias => "execution_alias",
        Type::PositionSide => "position_side",
        Type::ExitKind => "exit_kind",
        Type::SeriesF64 => "series<float>",
        Type::SeriesBool => "series<bool>",
        Type::Void => "void",
    }
}

fn span_contains(span: Span, offset: usize) -> bool {
    span.start.offset <= offset && offset < span.end.offset.max(span.start.offset + 1)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CompletionContext {
    General,
    Interval,
    Field { source_alias: Option<String> },
}

fn completion_context_for(
    source: &str,
    offset: usize,
    definitions: Option<&[DefinitionTarget]>,
) -> CompletionContext {
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
        let segment = &before[interval_start..token_start - 1];
        if Interval::parse(segment).is_some() || source_alias_segment(source, segment, definitions)
        {
            let alias = source_alias_for_field_context(before, token_start - 1);
            return CompletionContext::Field {
                source_alias: alias,
            };
        }
    }

    if trimmed.starts_with("interval") || trimmed.starts_with("use") {
        return CompletionContext::Interval;
    }

    CompletionContext::General
}

fn market_fields_for_alias(
    source: &str,
    source_alias: Option<String>,
    definitions: Option<&[DefinitionTarget]>,
) -> Vec<(&'static str, &'static str)> {
    let mut fields = MARKET_FIELDS.to_vec();
    if source_alias
        .as_deref()
        .is_some_and(|alias| source_alias_uses_binance_usdm(source, alias, definitions))
    {
        fields.extend(BINANCE_USDM_AUXILIARY_FIELDS);
    }
    fields
}

fn source_alias_for_field_context(before: &str, dot_index: usize) -> Option<String> {
    let before_dot = &before[..dot_index];
    let segment_start = before_dot
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .map(|index| index + 1)
        .unwrap_or(0);
    let segment = &before_dot[segment_start..];
    if Interval::parse(segment).is_some() {
        let alias_before_interval = &before_dot[..segment_start.saturating_sub(1)];
        let alias_end = alias_before_interval.len();
        let alias_start = alias_before_interval
            .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .map(|index| index + 1)
            .unwrap_or(0);
        let alias = &alias_before_interval[alias_start..alias_end];
        (!alias.is_empty()).then(|| alias.to_string())
    } else {
        (!segment.is_empty()).then(|| segment.to_string())
    }
}

fn source_alias_uses_binance_usdm(
    source: &str,
    alias: &str,
    definitions: Option<&[DefinitionTarget]>,
) -> bool {
    definitions
        .and_then(|definitions| {
            definitions.iter().find(|definition| {
                definition.name == alias && matches!(definition.kind, SymbolKind::Source)
            })
        })
        .and_then(|definition| definition.detail.as_deref())
        .is_some_and(|detail| detail.starts_with("binance.usdm("))
        || source.lines().any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with(&format!("source {alias} = binance.usdm("))
        })
}

fn source_alias_segment(
    source: &str,
    segment: &str,
    definitions: Option<&[DefinitionTarget]>,
) -> bool {
    definitions.is_some_and(|definitions| {
        definitions.iter().any(|definition| {
            definition.name == segment && matches!(definition.kind, SymbolKind::Source)
        })
    }) || source_declares_alias(source, segment)
}

fn source_declares_alias(source: &str, alias: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("source ") {
            return false;
        }
        let rest = &trimmed["source ".len()..];
        let Some((declared_alias, _)) = rest.split_once('=') else {
            return false;
        };
        declared_alias.trim() == alias
    })
}

fn format_ast(ast: &Ast) -> String {
    let mut lines = Vec::new();
    if let Some(base) = ast.strategy_intervals.base.first() {
        lines.push(format!("interval {}", base.interval.as_str()));
    }
    for source in &ast.strategy_intervals.sources {
        lines.push(format!(
            "source {} = {}(\"{}\")",
            source.alias,
            source.template.as_str(),
            source.symbol.replace('\\', "\\\\").replace('"', "\\\"")
        ));
    }
    for execution in &ast.strategy_intervals.executions {
        lines.push(format!(
            "execution {} = {}(\"{}\")",
            execution.alias,
            execution.template.as_str(),
            execution.symbol.replace('\\', "\\\\").replace('"', "\\\"")
        ));
    }
    for decl in &ast.strategy_intervals.supplemental {
        if decl.source.is_empty() {
            lines.push(format!("use {}", decl.interval.as_str()));
        } else {
            lines.push(format!("use {} {}", decl.source, decl.interval.as_str()));
        }
    }
    if !ast.strategy_intervals.base.is_empty()
        || !ast.strategy_intervals.sources.is_empty()
        || !ast.strategy_intervals.executions.is_empty()
        || !ast.strategy_intervals.supplemental.is_empty()
    {
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
        StmtKind::Const { name, expr, .. } => {
            lines.push(format!("{prefix}const {name} = {}", format_expr(expr, 0)));
        }
        StmtKind::Input {
            name,
            expr,
            optimization,
            ..
        } => {
            let optimization = optimization
                .as_ref()
                .map(format_input_optimization)
                .unwrap_or_default();
            lines.push(format!(
                "{prefix}input {name} = {}{optimization}",
                format_expr(expr, 0)
            ));
        }
        StmtKind::LetTuple { names, expr } => {
            let bindings = names
                .iter()
                .map(|binding| binding.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "{prefix}let ({bindings}) = {}",
                format_expr(expr, 0)
            ));
        }
        StmtKind::Export { name, expr, .. } => {
            lines.push(format!("{prefix}export {name} = {}", format_expr(expr, 0)));
        }
        StmtKind::Regime { name, expr, .. } => {
            lines.push(format!("{prefix}regime {name} = {}", format_expr(expr, 0)));
        }
        StmtKind::Trigger { name, expr, .. } => {
            lines.push(format!("{prefix}trigger {name} = {}", format_expr(expr, 0)));
        }
        StmtKind::ArbSignal { kind, expr } => {
            let keyword = match kind {
                crate::ast::ArbSignalKind::Entry => "arb_entry",
                crate::ast::ArbSignalKind::Exit => "arb_exit",
            };
            lines.push(format!("{prefix}{keyword} = {}", format_expr(expr, 0)));
        }
        StmtKind::OrderTemplate { name, spec, .. } => {
            lines.push(format!(
                "{prefix}order_template {name} = {}",
                format_order_spec(spec)
            ));
        }
        StmtKind::Signal { role, expr } => {
            let header = match role {
                crate::ast::SignalRole::LongEntry => "entry long",
                crate::ast::SignalRole::LongEntry2 => "entry2 long",
                crate::ast::SignalRole::LongEntry3 => "entry3 long",
                crate::ast::SignalRole::LongExit => "exit long",
                crate::ast::SignalRole::ShortEntry => "entry short",
                crate::ast::SignalRole::ShortEntry2 => "entry2 short",
                crate::ast::SignalRole::ShortEntry3 => "entry3 short",
                crate::ast::SignalRole::ShortExit => "exit short",
                crate::ast::SignalRole::ProtectLong => {
                    unreachable!("attached exits use order declarations")
                }
                crate::ast::SignalRole::ProtectAfterTarget1Long
                | crate::ast::SignalRole::ProtectAfterTarget2Long
                | crate::ast::SignalRole::ProtectAfterTarget3Long => {
                    unreachable!("attached exits use order declarations")
                }
                crate::ast::SignalRole::ProtectShort => {
                    unreachable!("attached exits use order declarations")
                }
                crate::ast::SignalRole::ProtectAfterTarget1Short
                | crate::ast::SignalRole::ProtectAfterTarget2Short
                | crate::ast::SignalRole::ProtectAfterTarget3Short => {
                    unreachable!("attached exits use order declarations")
                }
                crate::ast::SignalRole::TargetLong => {
                    unreachable!("attached exits use order declarations")
                }
                crate::ast::SignalRole::TargetLong2 | crate::ast::SignalRole::TargetLong3 => {
                    unreachable!("attached exits use order declarations")
                }
                crate::ast::SignalRole::TargetShort => {
                    unreachable!("attached exits use order declarations")
                }
                crate::ast::SignalRole::TargetShort2 | crate::ast::SignalRole::TargetShort3 => {
                    unreachable!("attached exits use order declarations")
                }
            };
            lines.push(format!("{prefix}{header} = {}", format_expr(expr, 0)));
        }
        StmtKind::Order { role, spec } => {
            let header = match role {
                crate::ast::SignalRole::LongEntry => "order entry long",
                crate::ast::SignalRole::LongEntry2 => "order entry2 long",
                crate::ast::SignalRole::LongEntry3 => "order entry3 long",
                crate::ast::SignalRole::LongExit => "order exit long",
                crate::ast::SignalRole::ShortEntry => "order entry short",
                crate::ast::SignalRole::ShortEntry2 => "order entry2 short",
                crate::ast::SignalRole::ShortEntry3 => "order entry3 short",
                crate::ast::SignalRole::ShortExit => "order exit short",
                crate::ast::SignalRole::ProtectLong => "protect long",
                crate::ast::SignalRole::ProtectAfterTarget1Long => "protect_after_target1 long",
                crate::ast::SignalRole::ProtectAfterTarget2Long => "protect_after_target2 long",
                crate::ast::SignalRole::ProtectAfterTarget3Long => "protect_after_target3 long",
                crate::ast::SignalRole::ProtectShort => "protect short",
                crate::ast::SignalRole::ProtectAfterTarget1Short => "protect_after_target1 short",
                crate::ast::SignalRole::ProtectAfterTarget2Short => "protect_after_target2 short",
                crate::ast::SignalRole::ProtectAfterTarget3Short => "protect_after_target3 short",
                crate::ast::SignalRole::TargetLong => "target long",
                crate::ast::SignalRole::TargetLong2 => "target2 long",
                crate::ast::SignalRole::TargetLong3 => "target3 long",
                crate::ast::SignalRole::TargetShort => "target short",
                crate::ast::SignalRole::TargetShort2 => "target2 short",
                crate::ast::SignalRole::TargetShort3 => "target3 short",
            };
            lines.push(format!("{prefix}{header} = {}", format_order_spec(spec)));
        }
        StmtKind::ArbOrder { kind, spec } => {
            let header = match kind {
                crate::ast::ArbSignalKind::Entry => "arb_order entry",
                crate::ast::ArbSignalKind::Exit => "arb_order exit",
            };
            lines.push(format!(
                "{prefix}{header} = {}",
                format_arb_order_spec(spec)
            ));
        }
        StmtKind::Transfer { asset_kind, spec } => {
            let asset = match asset_kind {
                crate::ast::TransferAssetKind::Quote => "quote",
                crate::ast::TransferAssetKind::Base => "base",
            };
            lines.push(format!(
                "{prefix}transfer {asset} = {}",
                format_transfer_spec(*asset_kind, spec)
            ));
        }
        StmtKind::OrderSize { target, expr } => {
            let header = match target {
                crate::ast::OrderSizeTarget::Module(binding) => {
                    lines.push(format!(
                        "{prefix}size module {} = {}",
                        binding.name,
                        format_expr(expr, 0)
                    ));
                    return;
                }
                crate::ast::OrderSizeTarget::Role(role) => match role {
                    crate::ast::SignalRole::LongEntry => "size entry long",
                    crate::ast::SignalRole::LongEntry2 => "size entry2 long",
                    crate::ast::SignalRole::LongEntry3 => "size entry3 long",
                    crate::ast::SignalRole::ShortEntry => "size entry short",
                    crate::ast::SignalRole::ShortEntry2 => "size entry2 short",
                    crate::ast::SignalRole::ShortEntry3 => "size entry3 short",
                    crate::ast::SignalRole::TargetLong => "size target long",
                    crate::ast::SignalRole::TargetLong2 => "size target2 long",
                    crate::ast::SignalRole::TargetLong3 => "size target3 long",
                    crate::ast::SignalRole::TargetShort => "size target short",
                    crate::ast::SignalRole::TargetShort2 => "size target2 short",
                    crate::ast::SignalRole::TargetShort3 => "size target3 short",
                    crate::ast::SignalRole::LongExit
                    | crate::ast::SignalRole::ShortExit
                    | crate::ast::SignalRole::ProtectLong
                    | crate::ast::SignalRole::ProtectAfterTarget1Long
                    | crate::ast::SignalRole::ProtectAfterTarget2Long
                    | crate::ast::SignalRole::ProtectAfterTarget3Long
                    | crate::ast::SignalRole::ProtectShort
                    | crate::ast::SignalRole::ProtectAfterTarget1Short
                    | crate::ast::SignalRole::ProtectAfterTarget2Short
                    | crate::ast::SignalRole::ProtectAfterTarget3Short => {
                        unreachable!("order sizing is only supported for entries and targets")
                    }
                },
            };
            lines.push(format!("{prefix}{header} = {}", format_expr(expr, 0)));
        }
        StmtKind::RiskControl { kind, side, expr } => {
            let keyword = match kind {
                RiskControlKind::Cooldown => "cooldown",
                RiskControlKind::MaxBarsInTrade => "max_bars_in_trade",
            };
            let side = match side {
                crate::PositionSide::Long => "long",
                crate::PositionSide::Short => "short",
            };
            lines.push(format!(
                "{prefix}{keyword} {side} = {}",
                format_expr(expr, 0)
            ));
        }
        StmtKind::PortfolioControl { kind, expr } => {
            let keyword = match kind {
                crate::ast::PortfolioControlKind::MaxPositions => "max_positions",
                crate::ast::PortfolioControlKind::MaxLongPositions => "max_long_positions",
                crate::ast::PortfolioControlKind::MaxShortPositions => "max_short_positions",
                crate::ast::PortfolioControlKind::MaxGrossExposurePct => "max_gross_exposure_pct",
                crate::ast::PortfolioControlKind::MaxNetExposurePct => "max_net_exposure_pct",
            };
            lines.push(format!("{prefix}{keyword} = {}", format_expr(expr, 0)));
        }
        StmtKind::PortfolioGroup { group } => {
            let aliases = group
                .aliases
                .iter()
                .map(|alias| alias.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "{prefix}portfolio_group \"{}\" = [{aliases}]",
                group.name
            ));
        }
        StmtKind::Module { module } => {
            lines.push(format!(
                "{prefix}module {} = {}",
                module.name,
                format_signal_role_surface(module.role)
            ));
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

fn format_input_optimization(optimization: &crate::ast::InputOptimization) -> String {
    match &optimization.kind {
        InputOptimizationKind::IntegerRange { low, high, step } => {
            format!(" optimize(int, {low}, {high}, {step})")
        }
        InputOptimizationKind::FloatRange { low, high, step } => match step {
            Some(step) => format!(" optimize(float, {low}, {high}, {step})"),
            None => format!(" optimize(float, {low}, {high})"),
        },
        InputOptimizationKind::Choice { values } => {
            let values = values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!(" optimize(choice, {values})")
        }
    }
}

fn format_signal_role_surface(role: crate::ast::SignalRole) -> &'static str {
    match role {
        crate::ast::SignalRole::LongEntry => "entry long",
        crate::ast::SignalRole::LongEntry2 => "entry2 long",
        crate::ast::SignalRole::LongEntry3 => "entry3 long",
        crate::ast::SignalRole::LongExit => "exit long",
        crate::ast::SignalRole::ShortEntry => "entry short",
        crate::ast::SignalRole::ShortEntry2 => "entry2 short",
        crate::ast::SignalRole::ShortEntry3 => "entry3 short",
        crate::ast::SignalRole::ShortExit => "exit short",
        crate::ast::SignalRole::ProtectLong => "protect long",
        crate::ast::SignalRole::ProtectAfterTarget1Long => "protect_after_target1 long",
        crate::ast::SignalRole::ProtectAfterTarget2Long => "protect_after_target2 long",
        crate::ast::SignalRole::ProtectAfterTarget3Long => "protect_after_target3 long",
        crate::ast::SignalRole::ProtectShort => "protect short",
        crate::ast::SignalRole::ProtectAfterTarget1Short => "protect_after_target1 short",
        crate::ast::SignalRole::ProtectAfterTarget2Short => "protect_after_target2 short",
        crate::ast::SignalRole::ProtectAfterTarget3Short => "protect_after_target3 short",
        crate::ast::SignalRole::TargetLong => "target long",
        crate::ast::SignalRole::TargetLong2 => "target2 long",
        crate::ast::SignalRole::TargetLong3 => "target3 long",
        crate::ast::SignalRole::TargetShort => "target short",
        crate::ast::SignalRole::TargetShort2 => "target2 short",
        crate::ast::SignalRole::TargetShort3 => "target3 short",
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

fn resolve_order_spec(
    context: &mut ResolutionContext<'_>,
    spec: &crate::ast::OrderSpec,
    scope: &HashMap<String, usize>,
) {
    if let crate::ast::OrderSpecKind::TemplateRef(binding) = &spec.kind {
        let definition_index = scope.get(&binding.name).copied();
        context.references.push(Reference {
            span: binding.span,
            definition_index,
            hover: definition_index
                .map(|index| definition_hover(&context.definitions[index]))
                .unwrap_or_else(|| format!("`{}`", binding.name)),
        });
        return;
    }
    if let Some(execution) = &spec.execution {
        match execution {
            crate::ast::OrderExecutionBinding::Static(execution) => {
                if let Some(definition_index) = scope.get(&execution.name).copied() {
                    let hover = context
                        .definitions
                        .get(definition_index)
                        .map(definition_hover)
                        .unwrap_or_else(|| format!("`{}`", execution.name));
                    context.references.push(Reference {
                        span: execution.span,
                        definition_index: Some(definition_index),
                        hover,
                    });
                }
            }
            crate::ast::OrderExecutionBinding::Expr(expr) => resolve_expr(context, expr, scope),
        }
    }
    match &spec.kind {
        crate::ast::OrderSpecKind::TemplateRef(_) => unreachable!("handled above"),
        crate::ast::OrderSpecKind::Market => {}
        crate::ast::OrderSpecKind::Limit {
            price,
            tif,
            post_only,
        } => {
            resolve_expr(context, price, scope);
            resolve_expr(context, tif, scope);
            resolve_expr(context, post_only, scope);
        }
        crate::ast::OrderSpecKind::StopMarket {
            trigger_price,
            trigger_ref,
        }
        | crate::ast::OrderSpecKind::TakeProfitMarket {
            trigger_price,
            trigger_ref,
        } => {
            resolve_expr(context, trigger_price, scope);
            resolve_expr(context, trigger_ref, scope);
        }
        crate::ast::OrderSpecKind::StopLimit {
            trigger_price,
            limit_price,
            tif,
            post_only,
            trigger_ref,
            expire_time_ms,
        }
        | crate::ast::OrderSpecKind::TakeProfitLimit {
            trigger_price,
            limit_price,
            tif,
            post_only,
            trigger_ref,
            expire_time_ms,
        } => {
            resolve_expr(context, trigger_price, scope);
            resolve_expr(context, limit_price, scope);
            resolve_expr(context, tif, scope);
            resolve_expr(context, post_only, scope);
            resolve_expr(context, trigger_ref, scope);
            resolve_expr(context, expire_time_ms, scope);
        }
    }
}

fn resolve_arb_order_spec(
    context: &mut ResolutionContext<'_>,
    spec: &crate::ast::ArbOrderSpec,
    scope: &HashMap<String, usize>,
) {
    let crate::ast::ArbOrderSpecKind::Pair {
        buy_venue,
        sell_venue,
        size,
        buy_price,
        sell_price,
        tif,
        post_only,
        abort_on_partial,
        max_leg_delay_bars,
        max_leg_price_drift_bps,
        ..
    } = &spec.kind;

    resolve_expr(context, buy_venue, scope);
    resolve_expr(context, sell_venue, scope);
    resolve_expr(context, size, scope);
    if let Some(expr) = buy_price {
        resolve_expr(context, expr, scope);
    }
    if let Some(expr) = sell_price {
        resolve_expr(context, expr, scope);
    }
    if let Some(expr) = tif {
        resolve_expr(context, expr, scope);
    }
    if let Some(expr) = post_only {
        resolve_expr(context, expr, scope);
    }
    if let Some(expr) = abort_on_partial {
        resolve_expr(context, expr, scope);
    }
    if let Some(expr) = max_leg_delay_bars {
        resolve_expr(context, expr, scope);
    }
    if let Some(expr) = max_leg_price_drift_bps {
        resolve_expr(context, expr, scope);
    }
}

fn format_order_spec(spec: &crate::ast::OrderSpec) -> String {
    if let crate::ast::OrderSpecKind::TemplateRef(binding) = &spec.kind {
        return binding.name.clone();
    }
    let execution_arg = spec.execution.as_ref().map(|execution| match execution {
        crate::ast::OrderExecutionBinding::Static(execution) => {
            format!("venue = {}", execution.name)
        }
        crate::ast::OrderExecutionBinding::Expr(expr) => {
            format!("venue = {}", format_expr(expr, 0))
        }
    });
    let mut args = match &spec.kind {
        crate::ast::OrderSpecKind::TemplateRef(_) => unreachable!("handled above"),
        crate::ast::OrderSpecKind::Market => Vec::new(),
        crate::ast::OrderSpecKind::Limit {
            price,
            tif,
            post_only,
        } => vec![
            format!("price = {}", format_expr(price, 0)),
            format!("tif = {}", format_expr(tif, 0)),
            format!("post_only = {}", format_expr(post_only, 0)),
        ],
        crate::ast::OrderSpecKind::StopMarket {
            trigger_price,
            trigger_ref,
        } => vec![
            format!("trigger_price = {}", format_expr(trigger_price, 0)),
            format!("trigger_ref = {}", format_expr(trigger_ref, 0)),
        ],
        crate::ast::OrderSpecKind::TakeProfitMarket {
            trigger_price,
            trigger_ref,
        } => vec![
            format!("trigger_price = {}", format_expr(trigger_price, 0)),
            format!("trigger_ref = {}", format_expr(trigger_ref, 0)),
        ],
        crate::ast::OrderSpecKind::StopLimit {
            trigger_price,
            limit_price,
            tif,
            post_only,
            trigger_ref,
            expire_time_ms,
        } => vec![
            format!("trigger_price = {}", format_expr(trigger_price, 0)),
            format!("limit_price = {}", format_expr(limit_price, 0)),
            format!("tif = {}", format_expr(tif, 0)),
            format!("post_only = {}", format_expr(post_only, 0)),
            format!("trigger_ref = {}", format_expr(trigger_ref, 0)),
            format!("expire_time_ms = {}", format_expr(expire_time_ms, 0)),
        ],
        crate::ast::OrderSpecKind::TakeProfitLimit {
            trigger_price,
            limit_price,
            tif,
            post_only,
            trigger_ref,
            expire_time_ms,
        } => vec![
            format!("trigger_price = {}", format_expr(trigger_price, 0)),
            format!("limit_price = {}", format_expr(limit_price, 0)),
            format!("tif = {}", format_expr(tif, 0)),
            format!("post_only = {}", format_expr(post_only, 0)),
            format!("trigger_ref = {}", format_expr(trigger_ref, 0)),
            format!("expire_time_ms = {}", format_expr(expire_time_ms, 0)),
        ],
    };
    if let Some(execution_arg) = execution_arg {
        args.push(execution_arg);
    }
    let callee = match &spec.kind {
        crate::ast::OrderSpecKind::Market => "market",
        crate::ast::OrderSpecKind::Limit { .. } => "limit",
        crate::ast::OrderSpecKind::StopMarket { .. } => "stop_market",
        crate::ast::OrderSpecKind::StopLimit { .. } => "stop_limit",
        crate::ast::OrderSpecKind::TakeProfitMarket { .. } => "take_profit_market",
        crate::ast::OrderSpecKind::TakeProfitLimit { .. } => "take_profit_limit",
        crate::ast::OrderSpecKind::TemplateRef(_) => unreachable!(),
    };
    if args.is_empty() {
        format!("{callee}()")
    } else {
        format!("{callee}({})", args.join(", "))
    }
}

fn format_arb_order_spec(spec: &crate::ast::ArbOrderSpec) -> String {
    let crate::ast::ArbOrderSpecKind::Pair {
        constructor,
        buy_venue,
        sell_venue,
        size,
        buy_price,
        sell_price,
        tif,
        post_only,
        abort_on_partial,
        max_leg_delay_bars,
        max_leg_price_drift_bps,
    } = &spec.kind;

    let mut args = vec![
        format!("buy_venue = {}", format_expr(buy_venue, 0)),
        format!("sell_venue = {}", format_expr(sell_venue, 0)),
        format!("size = {}", format_expr(size, 0)),
    ];
    if let Some(expr) = buy_price {
        args.push(format!("buy_price = {}", format_expr(expr, 0)));
    }
    if let Some(expr) = sell_price {
        args.push(format!("sell_price = {}", format_expr(expr, 0)));
    }
    if let Some(expr) = tif {
        args.push(format!("tif = {}", format_expr(expr, 0)));
    }
    if let Some(expr) = post_only {
        args.push(format!("post_only = {}", format_expr(expr, 0)));
    }
    if let Some(expr) = abort_on_partial {
        args.push(format!("abort_on_partial = {}", format_expr(expr, 0)));
    }
    if let Some(expr) = max_leg_delay_bars {
        args.push(format!("max_leg_delay_bars = {}", format_expr(expr, 0)));
    }
    if let Some(expr) = max_leg_price_drift_bps {
        args.push(format!(
            "max_leg_price_drift_bps = {}",
            format_expr(expr, 0)
        ));
    }
    let callee = match constructor {
        crate::ast::ArbPairConstructor::MarketPair => "market_pair",
        crate::ast::ArbPairConstructor::LimitPair => "limit_pair",
        crate::ast::ArbPairConstructor::MixedPair => "mixed_pair",
    };
    format!("{callee}({})", args.join(", "))
}

fn format_transfer_spec(
    asset_kind: crate::ast::TransferAssetKind,
    spec: &crate::ast::TransferSpec,
) -> String {
    let mut args = vec![
        format!("from = {}", format_expr(&spec.from, 0)),
        format!("to = {}", format_expr(&spec.to, 0)),
        format!("amount = {}", format_expr(&spec.amount, 0)),
    ];
    if let Some(expr) = &spec.fee {
        args.push(format!("fee = {}", format_expr(expr, 0)));
    }
    if let Some(expr) = &spec.delay_bars {
        args.push(format!("delay_bars = {}", format_expr(expr, 0)));
    }
    let callee = match asset_kind {
        crate::ast::TransferAssetKind::Quote => "quote_transfer",
        crate::ast::TransferAssetKind::Base => "base_transfer",
    };
    format!("{callee}({})", args.join(", "))
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
        ExprKind::String(value) => format!("{:?}", value),
        ExprKind::Ident(name) => name.clone(),
        ExprKind::EnumVariant {
            namespace, variant, ..
        } => format!("{namespace}.{variant}"),
        ExprKind::PositionField { field, .. } => format!("position.{}", field.as_str()),
        ExprKind::PositionEventField { field, .. } => {
            format!("position_event.{}", field.as_str())
        }
        ExprKind::LastExitField { scope, field, .. } => {
            format!("{}.{}", scope.namespace(), field.as_str())
        }
        ExprKind::LedgerField {
            execution_alias,
            field,
            ..
        } => format!("ledger({}).{}", execution_alias, field.as_str()),
        ExprKind::SourceSeries {
            source,
            interval,
            field,
            ..
        } => interval
            .map(|interval| {
                format!(
                    "{}.{}.{}",
                    source,
                    interval.as_str(),
                    render_market_field(*field)
                )
            })
            .unwrap_or_else(|| format!("{}.{}", source, render_market_field(*field))),
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
        ExprKind::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            let rendered = format!(
                "{} ? {} : {}",
                format_expr(condition, 4),
                format_expr(when_true, 0),
                format_expr(when_false, 4)
            );
            maybe_parenthesize(rendered, 4, parent_bp)
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

fn render_market_field(field: MarketField) -> &'static str {
    match field {
        MarketField::Open => "open",
        MarketField::High => "high",
        MarketField::Low => "low",
        MarketField::Close => "close",
        MarketField::Volume => "volume",
        MarketField::Time => "time",
        MarketField::FundingRate => "funding_rate",
        MarketField::MarkPrice => "mark_price",
        MarketField::IndexPrice => "index_price",
        MarketField::PremiumIndex => "premium_index",
        MarketField::Basis => "basis",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        analyze_document, complete_document, format_document, highlight_document, talib_hover,
        CompletionInsertTextFormat, CompletionKind, HighlightKind,
    };
    use crate::talib::metadata_by_name as talib_metadata_by_name;

    fn with_interval(source: &str) -> String {
        format!("interval 1m\nsource src = binance.spot(\"BTCUSDT\")\n{source}")
    }

    #[test]
    fn semantic_document_contains_symbols_and_definitions() {
        let source = with_interval(
            "fn cross_signal(a, b) = a > b\nlet basis = ema(src.close, 5)\nexport trend = cross_signal(src.close, basis)\nif trend { plot(1) } else { plot(0) }",
        );
        let document = analyze_document(&source).expect("semantic");
        assert!(document
            .symbols()
            .iter()
            .any(|symbol| symbol.name == "cross_signal"));
        let basis_offset = source.find("basis)").expect("basis ref");
        let definition = document.definition_at(basis_offset).expect("definition");
        assert_eq!(definition.name, "basis");
        let hover = document
            .hover_at(source.find("cross_signal(src.close").expect("call"))
            .expect("hover");
        assert!(hover.contents.contains("fn cross_signal"));
    }

    #[test]
    fn completions_include_keywords_builtins_and_fields() {
        let source =
            "interval 1m\nsource src = binance.spot(\"BTCUSDT\")\nuse src 1w\nplot(src.1w.close)";
        let document = analyze_document(source).expect("semantic");
        let general = document.completions_at(0);
        assert!(general.iter().any(|entry| entry.label == "interval"));
        assert!(general
            .iter()
            .any(|entry| entry.label == "ema" && entry.kind == CompletionKind::Builtin));
        assert!(general
            .iter()
            .any(|entry| entry.label == "valuewhen" && entry.kind == CompletionKind::Builtin));
        assert!(general
            .iter()
            .any(|entry| entry.label == "ht_sine" && entry.kind == CompletionKind::Builtin));
        assert!(general
            .iter()
            .any(|entry| entry.label == "cdlhammer" && entry.kind == CompletionKind::Builtin));
        let fields = document.completions_at(source.find("close").expect("field"));
        assert!(fields.iter().any(|entry| entry.label == "close"));
        let crossover = general
            .iter()
            .find(|entry| entry.label == "crossover")
            .expect("builtin completion");
        assert_eq!(crossover.detail.as_deref(), Some("crossover(a, b)"));
        assert!(crossover
            .documentation
            .as_deref()
            .expect("builtin docs")
            .contains("crosses above"));
        assert_eq!(crossover.insert_text, "crossover(${1:a}, ${2:b})");
        assert_eq!(
            crossover.insert_text_format,
            CompletionInsertTextFormat::Snippet
        );
    }

    #[test]
    fn auxiliary_field_completions_are_scoped_to_binance_usdm_sources() {
        let usdm = "interval 1h\nsource perp = binance.usdm(\"BTCUSDT\")\nplot(perp.mark_price)";
        let usdm_fields = analyze_document(usdm)
            .expect("semantic")
            .completions_at(usdm.find("mark_price").expect("field"));
        assert!(usdm_fields
            .iter()
            .any(|entry| entry.label == "funding_rate"));

        let spot = "interval 1h\nsource spot = binance.spot(\"BTCUSDT\")\nplot(spot.close)";
        let spot_fields = analyze_document(spot)
            .expect("semantic")
            .completions_at(spot.find("close").expect("field"));
        assert!(!spot_fields
            .iter()
            .any(|entry| entry.label == "funding_rate"));
    }

    #[test]
    fn completions_fall_back_to_builtins_for_incomplete_assignments() {
        let source = r#"interval 1m
source spot = binance.spot("BTCUSDT")

let sar_fast = sar
"#;
        let items = complete_document(source, source.len());
        let sar = items
            .iter()
            .find(|entry| entry.label == "sar")
            .expect("sar builtin completion");
        assert!(sar
            .detail
            .as_deref()
            .expect("sar detail")
            .starts_with("sar("));
        assert_eq!(
            sar.insert_text,
            "sar(${1:high}, ${2:low}, ${3:0.02}, ${4:0.2})"
        );
        assert_eq!(sar.insert_text_format, CompletionInsertTextFormat::Snippet);
    }

    #[test]
    fn completions_fall_back_to_source_aliases_for_semantic_errors() {
        let source = r#"interval 1m
source spot = binance.spot("BTCUSDT")

let basis = spo
"#;
        let items = complete_document(source, source.len());
        let spot = items
            .iter()
            .find(|entry| entry.label == "spot")
            .expect("spot source completion");
        assert_eq!(spot.kind, CompletionKind::Source);
    }

    #[test]
    fn completions_offer_market_fields_after_source_dot() {
        let source = r#"interval 1m
source spot = binance.spot("BTCUSDT")

let basis = spot.
"#;
        let offset = source.find("spot.").expect("spot field access") + "spot.".len();
        let items = complete_document(source, offset);
        assert!(items.iter().any(|entry| entry.label == "close"));
        assert!(items.iter().any(|entry| entry.label == "high"));
        assert!(items.iter().any(|entry| entry.label == "low"));
    }

    #[test]
    fn completions_do_not_offer_market_fields_after_series_variables() {
        let source = r#"interval 1m
source spot = binance.spot("BTCUSDT")

let fast = ema(spot.close, 13)
let basis = fast.
"#;
        let offset = source.find("fast.").expect("series variable access") + "fast.".len();
        let items = complete_document(source, offset);
        assert!(!items.iter().any(|entry| entry.label == "close"));
        assert!(!items.iter().any(|entry| entry.label == "high"));
        assert!(!items.iter().any(|entry| entry.label == "low"));
    }

    #[test]
    fn builtin_hover_uses_registry_metadata() {
        let source = with_interval("if crossover(src.close, 100) { plot(1) } else { plot(0) }");
        let document = analyze_document(&source).expect("semantic");
        let hover = document
            .hover_at(source.find("crossover(").expect("call"))
            .expect("hover");
        assert!(hover.contents.contains("`crossover(a, b)`"));
        assert!(hover.contents.contains("crosses above"));
    }

    #[test]
    fn talib_metadata_hover_is_available_for_unimplemented_functions() {
        let hover = talib_hover(talib_metadata_by_name("ht_sine").expect("metadata"));
        assert!(hover.contains("`ht_sine(real)`"));
        assert!(hover.contains("Hilbert Transform"));
    }

    #[test]
    fn formatter_is_idempotent() {
        let source = "interval 1m\nsource src = binance.spot(\"BTCUSDT\")\nfn cross_signal(a,b)=a>b\nif src.close>src.open{plot(1)}else{plot(0)}";
        let formatted = format_document(source).expect("formatted");
        let reformatted = format_document(&formatted).expect("reformatted");
        assert_eq!(formatted, reformatted);
    }

    #[test]
    fn formatter_preserves_input_optimization_metadata() {
        let source = "interval 1m\nsource src = binance.spot(\"BTCUSDT\")\ninput fast = 21 optimize(int, 8, 34, 1)\nplot(src.close)";
        let formatted = format_document(source).expect("formatted");
        assert!(formatted.contains("optimize(int, 8, 34, 1)"));
    }

    #[test]
    fn highlight_document_classifies_keywords_builtins_and_variables() {
        let source = with_interval("let basis = ema(src.close, 5)\nexport trend = basis");
        let highlights = highlight_document(&source);

        assert!(highlights
            .iter()
            .any(|token| token.kind == HighlightKind::Keyword));
        assert!(highlights
            .iter()
            .any(|token| token.kind == HighlightKind::Function));
        assert!(highlights
            .iter()
            .any(|token| token.kind == HighlightKind::Namespace));
        assert!(highlights
            .iter()
            .any(|token| token.kind == HighlightKind::Variable));
    }

    #[test]
    fn highlight_document_retains_lexical_color_on_compile_errors() {
        let source = "interval 1m\nsource src = binance.spot(\"BTCUSDT\")\nplot(src.foo)";
        let highlights = highlight_document(source);

        assert!(highlights
            .iter()
            .any(|token| token.kind == HighlightKind::Keyword));
    }
}
