//! Source-to-bytecode compilation for PalmScript programs.
//!
//! This module drives lexing and parsing, performs semantic analysis and type
//! inference, resolves locals and builtins, and emits deterministic bytecode.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ast::{
    Ast, BinaryOp, Block, Expr, ExprKind, FunctionDecl, InputOptimizationKind, NodeId, OrderSpec,
    OrderSpecKind, PortfolioControlKind as AstPortfolioControlKind,
    RiskControlKind as AstRiskControlKind, SignalRole as AstSignalRole, Stmt, StmtKind, UnaryOp,
};
use crate::builtins::{BuiltinArity, BuiltinId, BuiltinKind};
use crate::bytecode::{
    Constant, InputDecl, InputOptimizationDecl, InputOptimizationDeclKind, Instruction,
    LastExitFieldDecl, LocalInfo, OpCode, OrderDecl, OrderFieldDecl, OutputDecl, OutputKind,
    PortfolioControlDecl, PortfolioControlKind as CompiledPortfolioControlKind,
    PortfolioGroupDecl as CompiledPortfolioGroupDecl, PositionEventFieldDecl, PositionFieldDecl,
    Program, RiskControlDecl, RiskControlKind as CompiledRiskControlKind,
    SignalRole as CompiledSignalRole,
};
use crate::diagnostic::{CompileError, Diagnostic, DiagnosticKind};
use crate::interval::{
    DeclaredExecutionTarget, DeclaredMarketSource, Interval, MarketBinding, MarketField,
    MarketSource, SourceIntervalRef,
};
use crate::lexer;
use crate::order::{OrderFieldKind, OrderKind, SizeMode, TimeInForce, TriggerReference};
use crate::parser;
use crate::position::{
    ExitKind, LastExitField, LastExitScope, PositionEventField, PositionField, PositionSide,
};
use crate::span::Span;
use crate::talib::{metadata_by_name as talib_metadata_by_name, MaType, TalibFunctionMetadata};
use crate::types::{SlotKind, Type, Value};

const BASE_UPDATE_MASK: u32 = 1;
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CompiledProgram {
    pub program: Program,
    pub source: String,
}

pub fn compile(source: &str) -> Result<CompiledProgram, CompileError> {
    compile_with_input_overrides(source, &BTreeMap::new())
}

pub fn compile_with_input_overrides(
    source: &str,
    overrides: &BTreeMap<String, f64>,
) -> Result<CompiledProgram, CompileError> {
    let tokens = lexer::lex(source)?;
    let mut ast = parser::parse(&tokens)?;
    apply_input_overrides(&mut ast, overrides)?;
    Compiler::new(source, &ast).compile()
}

fn apply_input_overrides(
    ast: &mut Ast,
    overrides: &BTreeMap<String, f64>,
) -> Result<(), CompileError> {
    if overrides.is_empty() {
        return Ok(());
    }

    let mut seen = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for stmt in &mut ast.statements {
        if let StmtKind::Input { name, expr, .. } = &mut stmt.kind {
            if let Some(value) = overrides.get(name) {
                if !value.is_finite() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Compile,
                        format!("input override `{name}` must be finite"),
                        expr.span,
                    ));
                    continue;
                }
                seen.insert(name.clone());
                expr.kind = ExprKind::Number(*value);
            }
        }
    }
    for name in overrides.keys() {
        if seen.contains(name) {
            continue;
        }
        diagnostics.push(Diagnostic::new(
            DiagnosticKind::Compile,
            format!("unknown input override `{name}`"),
            Span::default(),
        ));
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(CompileError::new(diagnostics))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum InferredType {
    Concrete(Type),
    #[allow(dead_code)]
    Tuple2([Type; 2]),
    Tuple3([Type; 3]),
    Na,
}

impl InferredType {
    fn concrete(self) -> Option<Type> {
        match self {
            Self::Concrete(ty) => Some(ty),
            Self::Na => None,
            Self::Tuple2(_) | Self::Tuple3(_) => None,
        }
    }

    fn allow_bool(self) -> bool {
        matches!(
            self,
            Self::Concrete(Type::Bool | Type::SeriesBool) | Self::Na
        )
    }

    fn is_numeric_like(self) -> bool {
        matches!(self, Self::Concrete(Type::F64 | Type::SeriesF64) | Self::Na)
    }

    fn is_scalar_numeric(self) -> bool {
        matches!(self, Self::Concrete(Type::F64) | Self::Na)
    }

    fn is_series_numeric(self) -> bool {
        matches!(self, Self::Concrete(Type::SeriesF64))
    }

    fn is_series_bool(self) -> bool {
        matches!(self, Self::Concrete(Type::SeriesBool))
    }

    fn tuple_len(self) -> Option<usize> {
        match self {
            Self::Tuple2(_) => Some(2),
            Self::Tuple3(_) => Some(3),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ExprInfo {
    pub(crate) ty: InferredType,
    pub(crate) update_mask: u32,
}

impl ExprInfo {
    const fn scalar(ty: Type) -> Self {
        Self {
            ty: InferredType::Concrete(ty),
            update_mask: 0,
        }
    }

    const fn series(mask: u32) -> Self {
        Self {
            ty: InferredType::Concrete(Type::SeriesF64),
            update_mask: mask,
        }
    }

    fn concrete(self) -> Option<Type> {
        self.ty.concrete()
    }
}

#[derive(Clone, Copy, Debug)]
struct AnalyzerSymbol {
    info: ExprInfo,
}

#[derive(Clone, Copy, Debug)]
struct CompilerSymbol {
    slot: u16,
    ty: Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FunctionArgShape {
    ty: InferredType,
    update_mask: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FunctionSpecializationKey {
    function_id: NodeId,
    arg_shapes: Vec<FunctionArgShape>,
}

#[derive(Clone, Copy, Debug)]
struct FunctionParamBinding {
    ty: Type,
    kind: SlotKind,
    update_mask: u32,
}

#[derive(Clone, Debug)]
struct FunctionSpecialization {
    expr_info: HashMap<NodeId, ExprInfo>,
    user_function_calls: HashMap<NodeId, FunctionSpecializationKey>,
    return_info: ExprInfo,
    param_bindings: Vec<FunctionParamBinding>,
}

#[derive(Default)]
struct Analysis {
    base_interval: Option<Interval>,
    declared_sources: Vec<DeclaredMarketSource>,
    declared_executions: Vec<DeclaredExecutionTarget>,
    source_intervals: Vec<SourceIntervalRef>,
    expr_info: HashMap<NodeId, ExprInfo>,
    user_function_calls: HashMap<NodeId, FunctionSpecializationKey>,
    resolved_let_slots: HashMap<NodeId, u16>,
    resolved_let_tuple_slots: HashMap<NodeId, Vec<u16>>,
    resolved_output_slots: HashMap<NodeId, u16>,
    resolved_order_field_slots: HashMap<NodeId, ResolvedOrderFieldSlots>,
    order_size_decls: HashMap<CompiledSignalRole, ResolvedOrderSizeDecl>,
    immutable_slots: HashMap<NodeId, u16>,
    immutable_bindings: HashMap<String, ExprInfo>,
    immutable_binding_slots: HashMap<String, u16>,
    immutable_values: HashMap<String, Value>,
    locals: Vec<LocalInfo>,
    outputs: Vec<OutputDecl>,
    order_fields: Vec<OrderFieldDecl>,
    position_fields: Vec<PositionFieldDecl>,
    position_field_slots: HashMap<PositionField, u16>,
    position_event_fields: Vec<PositionEventFieldDecl>,
    position_event_field_slots: HashMap<PositionEventField, u16>,
    last_exit_fields: Vec<LastExitFieldDecl>,
    last_exit_field_slots: HashMap<(LastExitScope, LastExitField), u16>,
    orders: Vec<OrderDecl>,
    risk_controls: Vec<RiskControlDecl>,
    portfolio_controls: Vec<PortfolioControlDecl>,
    portfolio_groups: Vec<CompiledPortfolioGroupDecl>,
    source_slots: HashMap<(u16, Option<Interval>, MarketField), u16>,
    function_specializations: HashMap<FunctionSpecializationKey, FunctionSpecialization>,
}

#[derive(Clone, Copy, Debug, Default)]
struct ResolvedOrderFieldSlots {
    price_slot: Option<u16>,
    trigger_price_slot: Option<u16>,
    expire_time_slot: Option<u16>,
    size_slot: Option<u16>,
    risk_stop_slot: Option<u16>,
}

#[derive(Clone, Copy, Debug)]
struct ResolvedOrderSizeDecl {
    mode: SizeMode,
    size_field_id: u16,
    risk_stop_field_id: Option<u16>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AnalysisSnapshot {
    pub(crate) expr_info: HashMap<NodeId, ExprInfo>,
}

pub(crate) fn analyze_semantics(source: &str) -> Result<(Ast, AnalysisSnapshot), CompileError> {
    let tokens = lexer::lex(source)?;
    let ast = parser::parse(&tokens)?;
    let analysis = Analyzer::new(&ast).analyze(&ast)?;
    Ok((
        ast,
        AnalysisSnapshot {
            expr_info: analysis.expr_info,
        },
    ))
}

struct Analyzer<'a> {
    diagnostics: Vec<Diagnostic>,
    scopes: Vec<HashMap<String, AnalyzerSymbol>>,
    analysis: Analysis,
    functions_by_name: HashMap<String, &'a FunctionDecl>,
    functions_by_id: HashMap<NodeId, &'a FunctionDecl>,
    active_specializations: HashSet<FunctionSpecializationKey>,
    active_attached_role: Option<CompiledSignalRole>,
}

impl<'a> Analyzer<'a> {
    fn new(ast: &'a Ast) -> Self {
        let mut analyzer = Self {
            diagnostics: Vec::new(),
            scopes: vec![HashMap::new()],
            analysis: Analysis::default(),
            functions_by_name: HashMap::new(),
            functions_by_id: HashMap::new(),
            active_specializations: HashSet::new(),
            active_attached_role: None,
        };

        analyzer.validate_strategy_intervals(ast);
        analyzer.collect_functions(ast);
        analyzer.collect_source_series(ast);
        analyzer.collect_immutable_bindings(ast);
        analyzer.validate_function_bodies();
        analyzer.validate_function_cycles();
        analyzer
    }

    fn analyze(mut self, ast: &Ast) -> Result<Analysis, CompileError> {
        for stmt in &ast.statements {
            self.analyze_stmt(stmt);
        }
        for role in self.analysis.order_size_decls.keys().copied() {
            if self.analysis.orders.iter().any(|order| order.role == role) {
                continue;
            }
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "size declaration for `{}` requires a matching order declaration",
                    role.canonical_name()
                ),
                Span::default(),
            ));
        }
        if self.diagnostics.is_empty() {
            Ok(self.analysis)
        } else {
            Err(CompileError::new(self.diagnostics))
        }
    }

    fn validate_strategy_intervals(&mut self, ast: &Ast) {
        match ast.strategy_intervals.base.as_slice() {
            [] => self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "strategy must declare exactly one `interval <...>` directive",
                Span::default(),
            )),
            [decl] => {
                self.analysis.base_interval = Some(decl.interval);
            }
            [first, second, ..] => {
                self.analysis.base_interval = Some(first.interval);
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "strategy must declare exactly one `interval <...>` directive",
                    second.span,
                ));
            }
        }

        let Some(base_interval) = self.analysis.base_interval else {
            return;
        };

        if ast.strategy_intervals.sources.is_empty() {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "strategy must declare at least one `source <alias> = <exchange>.<market>(\"...\")` directive",
                Span::default(),
            ));
            return;
        }

        let mut source_aliases = BTreeSet::new();
        for source in &ast.strategy_intervals.sources {
            if !source_aliases.insert(source.alias.as_str()) {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("duplicate source alias `{}`", source.alias),
                    source.alias_span,
                ));
                continue;
            }
            let id = self.analysis.declared_sources.len() as u16;
            self.analysis.declared_sources.push(DeclaredMarketSource {
                id,
                alias: source.alias.clone(),
                template: source.template,
                symbol: source.symbol.clone(),
            });
        }

        let mut execution_aliases = BTreeSet::new();
        for execution in &ast.strategy_intervals.executions {
            let conflicts_with_source = self
                .analysis
                .declared_sources
                .iter()
                .find(|source| source.alias == execution.alias)
                .is_some_and(|source| {
                    source.template != execution.template || source.symbol != execution.symbol
                });
            if conflicts_with_source || !execution_aliases.insert(execution.alias.as_str()) {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("duplicate execution alias `{}`", execution.alias),
                    execution.alias_span,
                ));
                continue;
            }
            let id = (self.analysis.declared_sources.len()
                + self.analysis.declared_executions.len()) as u16;
            self.analysis
                .declared_executions
                .push(DeclaredExecutionTarget {
                    id,
                    alias: execution.alias.clone(),
                    template: execution.template,
                    symbol: execution.symbol.clone(),
                });
        }

        let mut uses = BTreeSet::new();
        for decl in &ast.strategy_intervals.supplemental {
            let Some(source_id) = self
                .analysis
                .declared_sources
                .iter()
                .find(|source| source.alias == decl.source)
                .map(|source| source.id)
            else {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("unknown source alias `{}`", decl.source),
                    decl.source_span,
                ));
                continue;
            };
            if decl.interval < base_interval {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!(
                        "lower interval reference `{}` is not allowed with base interval `{}`",
                        decl.interval.as_str(),
                        base_interval.as_str()
                    ),
                    decl.span,
                ));
                continue;
            }
            if !uses.insert((source_id, decl.interval)) {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!(
                        "duplicate `use {} {}` declaration",
                        decl.source,
                        decl.interval.as_str()
                    ),
                    decl.span,
                ));
            }
        }
        self.analysis.source_intervals = uses
            .iter()
            .map(|(source_id, interval)| SourceIntervalRef {
                source_id: *source_id,
                interval: *interval,
            })
            .collect();

        let mut refs = BTreeSet::new();
        for function in &ast.functions {
            collect_source_series_refs(&function.body, &mut refs);
        }
        for stmt in &ast.statements {
            collect_source_series_stmt(stmt, &mut refs);
        }
        for (source, interval, _field) in refs {
            let Some(source_id) = self
                .analysis
                .declared_sources
                .iter()
                .find(|decl| decl.alias == source)
                .map(|decl| decl.id)
            else {
                let span = source_ref_span(ast, &source, interval).unwrap_or_default();
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("unknown source alias `{source}`"),
                    span,
                ));
                continue;
            };
            if let Some(interval) = interval {
                if interval < base_interval {
                    let span = source_ref_span(ast, &source, Some(interval)).unwrap_or_default();
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "lower interval reference `{}` is not allowed with base interval `{}`",
                            interval.as_str(),
                            base_interval.as_str()
                        ),
                        span,
                    ));
                    continue;
                }
                if interval != base_interval && !uses.contains(&(source_id, interval)) {
                    let span = source_ref_span(ast, &source, Some(interval)).unwrap_or_default();
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "source interval `{}` for `{}` must be declared with `use {} {}`",
                            interval.as_str(),
                            source,
                            source,
                            interval.as_str()
                        ),
                        span,
                    ));
                }
            }
        }
    }

    fn collect_functions(&mut self, ast: &'a Ast) {
        for function in &ast.functions {
            if BuiltinId::from_name(&function.name).is_some()
                || talib_metadata_by_name(&function.name).is_some()
            {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("function name `{}` collides with a builtin", function.name),
                    function.span,
                ));
                continue;
            }
            if self.lookup_symbol(&function.name).is_some() {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!(
                        "function name `{}` collides with a predefined binding",
                        function.name
                    ),
                    function.span,
                ));
                continue;
            }
            if self.functions_by_name.contains_key(&function.name) {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("duplicate function `{}`", function.name),
                    function.span,
                ));
                continue;
            }

            let mut seen = HashSet::new();
            for param in &function.params {
                if !seen.insert(param.name.as_str()) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "duplicate parameter `{}` in function `{}`",
                            param.name, function.name
                        ),
                        param.span,
                    ));
                }
            }

            self.functions_by_name
                .insert(function.name.clone(), function);
            self.functions_by_id.insert(function.id, function);
        }
    }

    fn collect_source_series(&mut self, ast: &Ast) {
        let alias_to_id: HashMap<&str, u16> = self
            .analysis
            .declared_sources
            .iter()
            .map(|source| (source.alias.as_str(), source.id))
            .collect();
        let mut refs = BTreeSet::new();
        for function in &ast.functions {
            collect_source_series_refs(&function.body, &mut refs);
        }
        for stmt in &ast.statements {
            collect_source_series_stmt(stmt, &mut refs);
        }

        for (source, interval, field) in refs {
            let Some(&source_id) = alias_to_id.get(source.as_str()) else {
                continue;
            };
            let update_mask = interval.map_or(BASE_UPDATE_MASK, Interval::mask);
            let slot = self.analysis.locals.len() as u16;
            self.analysis.locals.push(LocalInfo::series(
                None,
                Type::SeriesF64,
                true,
                update_mask,
                Some(MarketBinding {
                    source: MarketSource::Named {
                        source_id,
                        interval,
                    },
                    field,
                }),
            ));
            self.analysis
                .source_slots
                .insert((source_id, interval, field), slot);
        }
    }

    fn collect_immutable_bindings(&mut self, ast: &Ast) {
        for stmt in &ast.statements {
            match &stmt.kind {
                StmtKind::Const { name, expr, .. } => {
                    self.analyze_immutable_stmt(stmt.id, name, expr, true, None, stmt.span);
                }
                StmtKind::Input {
                    name,
                    expr,
                    optimization,
                    ..
                } => {
                    self.analyze_immutable_stmt(
                        stmt.id,
                        name,
                        expr,
                        false,
                        optimization.as_ref(),
                        stmt.span,
                    );
                }
                _ => {}
            }
        }
    }

    fn analyze_immutable_stmt(
        &mut self,
        stmt_id: NodeId,
        name: &str,
        expr: &Expr,
        is_const: bool,
        optimization: Option<&crate::ast::InputOptimization>,
        span: Span,
    ) {
        if self.scopes[0].contains_key(name) {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!("duplicate binding `{name}` in the same scope"),
                span,
            ));
            return;
        }

        let info = self.analyze_immutable_expr(expr, is_const);
        let Some(ty) = info.concrete() else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                if is_const {
                    "`const` expressions must resolve to a scalar value"
                } else {
                    "`input` expressions must resolve to a scalar value"
                },
                expr.span,
            ));
            return;
        };
        if ty.is_series() || matches!(ty, Type::Void) {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                if is_const {
                    "`const` expressions must resolve to a scalar value"
                } else {
                    "`input` expressions must resolve to a scalar value"
                },
                expr.span,
            ));
            return;
        }

        let slot = self.define_symbol(name.to_string(), info, false, None);
        self.analysis.immutable_slots.insert(stmt_id, slot);
        self.analysis
            .immutable_bindings
            .insert(name.to_string(), info);
        self.analysis
            .immutable_binding_slots
            .insert(name.to_string(), slot);
        if let Some(value) = eval_immutable_expr(expr, &self.analysis.immutable_values) {
            if let Some(metadata) = optimization {
                self.validate_input_optimization(name, metadata, ty, &value);
            }
            self.analysis
                .immutable_values
                .insert(name.to_string(), value);
        } else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                if is_const {
                    "failed to evaluate `const` expression at compile time"
                } else {
                    "failed to evaluate `input` expression at compile time"
                },
                expr.span,
            ));
        }
    }

    fn validate_input_optimization(
        &mut self,
        name: &str,
        optimization: &crate::ast::InputOptimization,
        ty: Type,
        value: &Value,
    ) {
        if ty != Type::F64 {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "input optimization metadata is only supported on numeric `input` bindings; `{name}` is `{}`",
                    ty.type_name()
                ),
                optimization.span,
            ));
            return;
        }
        let Value::F64(default_value) = value else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "input optimization metadata requires a concrete numeric default value",
                optimization.span,
            ));
            return;
        };
        if !default_value.is_finite() {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "input optimization metadata requires a finite numeric default value",
                optimization.span,
            ));
            return;
        }
        match &optimization.kind {
            InputOptimizationKind::IntegerRange { low, high, step } => {
                if low > high {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "input `{name}` optimize int metadata requires low <= high, found {low} > {high}"
                        ),
                        optimization.span,
                    ));
                }
                if *step <= 0 {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("input `{name}` optimize int step must be > 0"),
                        optimization.span,
                    ));
                }
                if (*default_value).fract() != 0.0 {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "input `{name}` optimize int metadata requires an integer default value"
                        ),
                        optimization.span,
                    ));
                }
                let default_value = *default_value as i64;
                if default_value < *low || default_value > *high {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "input `{name}` default value {default_value} must lie inside optimize int range {low}..={high}"
                        ),
                        optimization.span,
                    ));
                }
                if default_value >= *low && (*step > 0) && (default_value - *low) % *step != 0 {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "input `{name}` default value {default_value} must align to optimize int step {step} from low {low}"
                        ),
                        optimization.span,
                    ));
                }
            }
            InputOptimizationKind::FloatRange { low, high, step } => {
                if !low.is_finite() || !high.is_finite() || low > high {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "input `{name}` optimize float metadata requires finite low/high with low <= high"
                        ),
                        optimization.span,
                    ));
                }
                if *default_value < *low || *default_value > *high {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "input `{name}` default value {:.6} must lie inside optimize float range {:.6}..={:.6}",
                            default_value, low, high
                        ),
                        optimization.span,
                    ));
                }
                if let Some(step) = step {
                    if !step.is_finite() || *step <= 0.0 {
                        self.diagnostics.push(Diagnostic::new(
                            DiagnosticKind::Type,
                            format!(
                                "input `{name}` optimize float step must be a finite value > 0"
                            ),
                            optimization.span,
                        ));
                    } else if *default_value >= *low {
                        let delta = (*default_value - *low) / *step;
                        if (delta.round() - delta).abs() > 1.0e-9 {
                            self.diagnostics.push(Diagnostic::new(
                                DiagnosticKind::Type,
                                format!(
                                    "input `{name}` default value {:.6} must align to optimize float step {:.6} from low {:.6}",
                                    default_value, step, low
                                ),
                                optimization.span,
                            ));
                        }
                    }
                }
            }
            InputOptimizationKind::Choice { values } => {
                if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "input `{name}` optimize choice metadata requires one or more finite numeric choices"
                        ),
                        optimization.span,
                    ));
                } else if !values
                    .iter()
                    .any(|value| (*value - *default_value).abs() <= 1.0e-9)
                {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "input `{name}` default value {:.6} must be present in optimize choice metadata",
                            default_value
                        ),
                        optimization.span,
                    ));
                }
            }
        }
    }

    fn analyze_immutable_expr(&mut self, expr: &Expr, is_const: bool) -> ExprInfo {
        match &expr.kind {
            ExprKind::Number(_) => ExprInfo::scalar(Type::F64),
            ExprKind::Bool(_) => ExprInfo::scalar(Type::Bool),
            ExprKind::Na => ExprInfo {
                ty: InferredType::Na,
                update_mask: 0,
            },
            ExprKind::EnumVariant {
                namespace,
                variant,
                variant_span,
                ..
            } => match resolve_enum_variant(namespace, variant) {
                Some(value) => ExprInfo::scalar(scalar_type_for_value(&value)),
                None => {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("unknown enum variant `{}.{}`", namespace, variant),
                        *variant_span,
                    ));
                    ExprInfo::scalar(Type::MaType)
                }
            },
            ExprKind::Ident(name) => match self.analysis.immutable_bindings.get(name).copied() {
                Some(info) => info,
                None => {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        if is_const {
                            format!("`const` expressions may only reference previously declared `const` or `input` bindings; found `{name}`")
                        } else {
                            format!("`input` expressions may only use scalar literals or enum literals; found `{name}`")
                        },
                        expr.span,
                    ));
                    ExprInfo::scalar(Type::F64)
                }
            },
            ExprKind::Unary { op, expr: inner } => {
                if !is_const {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "`input` expressions may only use scalar literals or enum literals",
                        expr.span,
                    ));
                }
                let inner_info = self.analyze_immutable_expr(inner, is_const);
                ExprInfo {
                    ty: infer_unary(*op, inner_info.ty, inner.span, &mut self.diagnostics),
                    update_mask: 0,
                }
            }
            ExprKind::Binary { op, left, right } => {
                if !is_const {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "`input` expressions may only use scalar literals or enum literals",
                        expr.span,
                    ));
                }
                let left_info = self.analyze_immutable_expr(left, is_const);
                let right_info = self.analyze_immutable_expr(right, is_const);
                ExprInfo {
                    ty: infer_binary(
                        *op,
                        left_info.ty,
                        right_info.ty,
                        left.span.merge(right.span),
                        &mut self.diagnostics,
                    ),
                    update_mask: 0,
                }
            }
            ExprKind::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                if !is_const {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "`input` expressions may only use scalar literals or enum literals",
                        expr.span,
                    ));
                }
                let condition_info = self.analyze_immutable_expr(condition, is_const);
                if !condition_info.ty.allow_bool() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "conditional expression condition must be bool, series<bool>, or na",
                        condition.span,
                    ));
                }
                let true_info = self.analyze_immutable_expr(when_true, is_const);
                let false_info = self.analyze_immutable_expr(when_false, is_const);
                ExprInfo {
                    ty: infer_conditional(
                        true_info.ty,
                        false_info.ty,
                        expr.span,
                        &mut self.diagnostics,
                    ),
                    update_mask: 0,
                }
            }
            ExprKind::Call { callee, args, .. } => {
                if !is_const {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "`input` expressions may only use scalar literals or enum literals",
                        expr.span,
                    ));
                    for arg in args {
                        self.analyze_immutable_expr(arg, is_const);
                    }
                    return ExprInfo::scalar(Type::F64);
                }
                let Some(builtin) = BuiltinId::from_name(callee) else {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("`const` expressions may only call pure scalar builtins; found `{callee}`"),
                        expr.span,
                    ));
                    for arg in args {
                        self.analyze_immutable_expr(arg, is_const);
                    }
                    return ExprInfo::scalar(Type::F64);
                };
                if !builtin_allowed_in_const(builtin) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("`const` expressions may only call pure scalar builtins; found `{callee}`"),
                        expr.span,
                    ));
                }
                let arg_info: Vec<ExprInfo> = args
                    .iter()
                    .map(|arg| self.analyze_immutable_expr(arg, is_const))
                    .collect();
                let info = analyze_helper_builtin(
                    builtin,
                    callee,
                    args,
                    &arg_info,
                    &self.analysis.immutable_values,
                    expr.span,
                    &mut self.diagnostics,
                );
                if info.update_mask != 0 || info.concrete().is_some_and(Type::is_series) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "`const` expressions must resolve to a scalar value",
                        expr.span,
                    ));
                }
                ExprInfo {
                    ty: info.ty,
                    update_mask: 0,
                }
            }
            ExprKind::String(_)
            | ExprKind::SourceSeries { .. }
            | ExprKind::PositionField { .. }
            | ExprKind::PositionEventField { .. }
            | ExprKind::LastExitField { .. }
            | ExprKind::Index { .. } => {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    if is_const {
                        "`const` expressions may only use scalar literals, enum literals, immutable bindings, and pure scalar builtins"
                    } else {
                        "`input` expressions may only use scalar literals or enum literals"
                    },
                    expr.span,
                ));
                ExprInfo::scalar(Type::F64)
            }
        }
    }

    fn validate_function_cycles(&mut self) {
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut reported = HashSet::new();
        let functions: Vec<&FunctionDecl> = self.functions_by_id.values().copied().collect();
        for function in functions {
            self.visit_function_cycle(function, &mut visiting, &mut visited, &mut reported);
        }
    }

    fn validate_function_bodies(&mut self) {
        let functions: Vec<&FunctionDecl> = self.functions_by_id.values().copied().collect();
        for function in functions {
            let params: HashSet<&str> = function
                .params
                .iter()
                .map(|param| param.name.as_str())
                .collect();
            self.validate_function_expr(&function.body, &params);
        }
    }

    fn validate_function_expr(&mut self, expr: &Expr, params: &HashSet<&str>) {
        match &expr.kind {
            ExprKind::Ident(name) => {
                if !params.contains(name.as_str()) && !self.is_function_visible_name(name) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "function bodies may only reference parameters or declared source series; found `{name}`"
                        ),
                        expr.span,
                    ));
                }
            }
            ExprKind::PositionField { field_span, .. } => {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "`position.*` is only available inside `protect` and `target` declarations",
                    *field_span,
                ));
            }
            ExprKind::PositionEventField { .. }
            | ExprKind::LastExitField { .. }
            | ExprKind::SourceSeries { .. }
            | ExprKind::EnumVariant { .. } => {}
            ExprKind::Unary { expr, .. } => self.validate_function_expr(expr, params),
            ExprKind::Binary { left, right, .. } => {
                self.validate_function_expr(left, params);
                self.validate_function_expr(right, params);
            }
            ExprKind::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                self.validate_function_expr(condition, params);
                self.validate_function_expr(when_true, params);
                self.validate_function_expr(when_false, params);
            }
            ExprKind::Call { callee, args, .. } => {
                match BuiltinId::from_name(callee) {
                    Some(BuiltinId::Plot) => {
                        self.diagnostics.push(Diagnostic::new(
                            DiagnosticKind::Type,
                            "function bodies may not call `plot`",
                            expr.span,
                        ));
                    }
                    Some(
                        BuiltinId::Open
                        | BuiltinId::High
                        | BuiltinId::Low
                        | BuiltinId::Close
                        | BuiltinId::Volume
                        | BuiltinId::Time,
                    ) => {
                        self.diagnostics.push(Diagnostic::new(
                            DiagnosticKind::Type,
                            "market data builtins are identifiers, not callable functions",
                            expr.span,
                        ));
                    }
                    Some(builtin) => {
                        let arity = builtin.arity();
                        if !arity.accepts(args.len()) {
                            self.diagnostics.push(Diagnostic::new(
                                DiagnosticKind::Type,
                                expected_arity_message(callee, arity),
                                expr.span,
                            ));
                        }
                    }
                    None if talib_metadata_by_name(callee).is_some() => {
                        self.diagnostics.push(Diagnostic::new(
                            DiagnosticKind::Type,
                            format!(
                                "builtin `{callee}` is reserved by the TA-Lib catalog but is not implemented yet"
                            ),
                            expr.span,
                        ));
                    }
                    None => match self.functions_by_name.get(callee).copied() {
                        Some(target) if target.params.len() != args.len() => {
                            self.diagnostics.push(Diagnostic::new(
                                DiagnosticKind::Type,
                                format!(
                                    "function `{callee}` expects {} argument(s), found {}",
                                    target.params.len(),
                                    args.len()
                                ),
                                expr.span,
                            ));
                        }
                        Some(_) => {}
                        None => {
                            self.diagnostics.push(Diagnostic::new(
                                DiagnosticKind::Type,
                                format!("unknown function `{callee}`"),
                                expr.span,
                            ));
                        }
                    },
                }
                for arg in args {
                    self.validate_function_expr(arg, params);
                }
            }
            ExprKind::Index { target, index } => {
                self.validate_function_expr(target, params);
                self.validate_function_expr(index, params);
            }
            ExprKind::Number(_) | ExprKind::Bool(_) | ExprKind::Na | ExprKind::String(_) => {}
        }
    }

    fn visit_function_cycle(
        &mut self,
        function: &'a FunctionDecl,
        visiting: &mut HashSet<NodeId>,
        visited: &mut HashSet<NodeId>,
        reported: &mut HashSet<NodeId>,
    ) {
        if visited.contains(&function.id) {
            return;
        }
        visiting.insert(function.id);
        let callees: Vec<NodeId> = called_user_functions(&function.body, &self.functions_by_name)
            .into_iter()
            .filter_map(|callee| {
                self.functions_by_name
                    .get(callee)
                    .map(|function| function.id)
            })
            .collect();
        for callee_id in callees {
            let Some(target) = self.functions_by_id.get(&callee_id).copied() else {
                continue;
            };
            if visiting.contains(&target.id) {
                if reported.insert(function.id) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "recursive and cyclic function definitions are not allowed",
                        function.span,
                    ));
                }
                continue;
            }
            self.visit_function_cycle(target, visiting, visited, reported);
        }
        visiting.remove(&function.id);
        visited.insert(function.id);
    }

    fn analyze_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, expr, .. } => {
                let expr_info = self.analyze_expr(expr);
                if expr_info.ty.tuple_len().is_some() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "tuple-valued expressions must be destructured with `let (...) = ...`",
                        expr.span,
                    ));
                }
                let concrete = expr_info.ty.concrete().unwrap_or(Type::F64);
                if self.scopes.last().unwrap().contains_key(name) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("duplicate binding `{name}` in the same scope"),
                        stmt.span,
                    ));
                    return;
                }
                let slot = self.define_symbol(
                    name.clone(),
                    ExprInfo {
                        ty: InferredType::Concrete(concrete),
                        update_mask: expr_info.update_mask,
                    },
                    false,
                    None,
                );
                self.analysis.resolved_let_slots.insert(stmt.id, slot);
            }
            StmtKind::LetTuple { names, expr } => {
                let expr_info = self.analyze_expr(expr);
                let expected = names.len();
                if expr_info.ty.tuple_len() != Some(expected) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "tuple binding expects {expected} value(s), found {}",
                            expr_info.ty.tuple_len().unwrap_or(1)
                        ),
                        expr.span,
                    ));
                    return;
                }
                let item_types = match expr_info.ty {
                    InferredType::Tuple2(types) => types.to_vec(),
                    InferredType::Tuple3(types) => types.to_vec(),
                    _ => unreachable!(),
                };
                let mut slots = Vec::with_capacity(names.len());
                for (binding, ty) in names.iter().zip(item_types) {
                    if self.scopes.last().unwrap().contains_key(&binding.name) {
                        self.diagnostics.push(Diagnostic::new(
                            DiagnosticKind::Type,
                            format!("duplicate binding `{}` in the same scope", binding.name),
                            binding.span,
                        ));
                        return;
                    }
                    let slot = self.define_symbol(
                        binding.name.clone(),
                        ExprInfo {
                            ty: InferredType::Concrete(ty),
                            update_mask: expr_info.update_mask,
                        },
                        false,
                        None,
                    );
                    slots.push(slot);
                }
                self.analysis
                    .resolved_let_tuple_slots
                    .insert(stmt.id, slots);
            }
            StmtKind::Const { name, expr, .. } | StmtKind::Input { name, expr, .. } => {
                let info = self.analyze_expr(expr);
                let Some(slot) = self.analysis.immutable_slots.get(&stmt.id).copied() else {
                    return;
                };
                let Some(expected) = self.analysis.immutable_bindings.get(name).copied() else {
                    return;
                };
                if info.ty != expected.ty {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("binding `{name}` changed type during semantic analysis"),
                        expr.span,
                    ));
                    return;
                }
                self.analysis.resolved_let_slots.insert(stmt.id, slot);
            }
            StmtKind::Export { name, expr, .. } => {
                self.analyze_output_stmt(stmt, name, expr, OutputKind::ExportSeries, None);
            }
            StmtKind::Regime { name, expr, .. } => {
                self.analyze_regime_stmt(stmt, name, expr);
            }
            StmtKind::Trigger { name, expr, .. } => {
                self.analyze_output_stmt(stmt, name, expr, OutputKind::Trigger, None);
            }
            StmtKind::Signal { role, expr } => {
                self.analyze_signal_stmt(stmt, *role, expr);
            }
            StmtKind::Order { role, spec } => {
                self.analyze_order_stmt(stmt, *role, spec);
            }
            StmtKind::OrderSize { role, expr } => {
                self.analyze_order_size_stmt(stmt, *role, expr);
            }
            StmtKind::RiskControl { kind, side, expr } => {
                self.analyze_risk_control_stmt(stmt, *kind, *side, expr);
            }
            StmtKind::PortfolioControl { kind, expr } => {
                self.analyze_portfolio_control_stmt(stmt, *kind, expr);
            }
            StmtKind::PortfolioGroup { group } => {
                self.analyze_portfolio_group_stmt(group);
            }
            StmtKind::If {
                condition,
                then_block,
                else_block,
            } => {
                let info = self.analyze_expr(condition);
                if !info.ty.allow_bool() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "if condition must be bool, series<bool>, or na",
                        condition.span,
                    ));
                }
                self.push_scope();
                self.analyze_block(then_block);
                self.pop_scope();
                self.push_scope();
                self.analyze_block(else_block);
                self.pop_scope();
            }
            StmtKind::Expr(expr) => {
                self.analyze_expr(expr);
            }
        }
    }

    fn analyze_signal_stmt(&mut self, stmt: &Stmt, role: AstSignalRole, expr: &Expr) {
        let compiled_role = compiled_signal_role(role);
        let canonical = compiled_role.canonical_name();
        self.analyze_output_stmt(
            stmt,
            canonical,
            expr,
            OutputKind::Trigger,
            Some(compiled_role),
        );
    }

    fn analyze_regime_stmt(&mut self, stmt: &Stmt, name: &str, expr: &Expr) {
        let expr_info = self.analyze_expr(expr);
        if !expr_info.ty.allow_bool() {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "regime requires bool, series<bool>, or na",
                expr.span,
            ));
            return;
        }
        self.analyze_output_stmt(stmt, name, expr, OutputKind::ExportSeries, None);
    }

    fn analyze_output_stmt(
        &mut self,
        stmt: &Stmt,
        name: &str,
        expr: &Expr,
        kind: OutputKind,
        signal_role: Option<CompiledSignalRole>,
    ) {
        let expr_info = self.analyze_expr(expr);
        match kind {
            OutputKind::ExportSeries => {
                if matches!(expr_info.ty, InferredType::Concrete(Type::Void)) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "export requires a numeric, bool, series numeric, series bool, or na value",
                        expr.span,
                    ));
                    return;
                }
            }
            OutputKind::Trigger => {
                if !expr_info.ty.allow_bool() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        if signal_role.is_some() {
                            "signal declarations require bool, series<bool>, or na"
                        } else {
                            "trigger requires bool, series<bool>, or na"
                        },
                        expr.span,
                    ));
                    return;
                }
            }
        }

        if self.scopes.last().unwrap().contains_key(name) {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!("duplicate binding `{name}` in the same scope"),
                stmt.span,
            ));
            return;
        }

        let ty = output_series_type(expr_info.ty, kind, expr.span, &mut self.diagnostics);
        let slot = self.define_symbol(
            name.to_string(),
            ExprInfo {
                ty: InferredType::Concrete(ty),
                update_mask: BASE_UPDATE_MASK,
            },
            false,
            None,
        );
        self.analysis.resolved_output_slots.insert(stmt.id, slot);
        self.analysis.outputs.push(OutputDecl {
            name: name.to_string(),
            kind,
            signal_role,
            ty,
            slot,
        });
    }

    fn analyze_order_stmt(&mut self, stmt: &Stmt, role: AstSignalRole, spec: &OrderSpec) {
        let role = compiled_signal_role(role);
        if self.analysis.orders.iter().any(|decl| decl.role == role) {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "duplicate order declaration for `{}`",
                    role.canonical_name()
                ),
                stmt.span,
            ));
            return;
        }

        let mut resolved = ResolvedOrderFieldSlots::default();
        let size_decl = self.analysis.order_size_decls.get(&role).copied();
        let mut order = OrderDecl {
            role,
            execution_alias: None,
            kind: OrderKind::Market,
            tif: None,
            post_only: false,
            trigger_ref: None,
            size_mode: size_decl.map(|decl| decl.mode),
            price_field_id: None,
            trigger_price_field_id: None,
            expire_time_field_id: None,
            size_field_id: size_decl.map(|decl| decl.size_field_id),
            risk_stop_field_id: size_decl.and_then(|decl| decl.risk_stop_field_id),
        };

        if let Some(binding) = &spec.execution {
            let exists = self
                .analysis
                .declared_executions
                .iter()
                .any(|execution| execution.alias == binding.name);
            if !exists {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("unknown execution alias `{}`", binding.name),
                    binding.span,
                ));
            } else {
                order.execution_alias = Some(binding.name.clone());
            }
        }

        match &spec.kind {
            OrderSpecKind::Market => {}
            OrderSpecKind::Limit {
                price,
                tif,
                post_only,
            } => {
                order.kind = OrderKind::Limit;
                if let Some((field_id, slot)) =
                    self.analyze_order_numeric_field(role, OrderFieldKind::Price, price)
                {
                    order.price_field_id = Some(field_id);
                    resolved.price_slot = Some(slot);
                }
                self.diagnose_unknown_enum_literal(tif);
                order.tif = literal_time_in_force(tif, &self.analysis.immutable_values);
                if order.tif.is_none() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "limit requires `tif.<variant>` as the second argument",
                        tif.span,
                    ));
                }
                match literal_bool(post_only, &self.analysis.immutable_values) {
                    Some(value) => order.post_only = value,
                    None => self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "limit requires a bool literal or immutable bool binding as the third argument",
                        post_only.span,
                    )),
                }
            }
            OrderSpecKind::StopMarket {
                trigger_price,
                trigger_ref,
            } => {
                order.kind = OrderKind::StopMarket;
                if let Some((field_id, slot)) = self.analyze_order_numeric_field(
                    role,
                    OrderFieldKind::TriggerPrice,
                    trigger_price,
                ) {
                    order.trigger_price_field_id = Some(field_id);
                    resolved.trigger_price_slot = Some(slot);
                }
                self.diagnose_unknown_enum_literal(trigger_ref);
                order.trigger_ref =
                    literal_trigger_reference(trigger_ref, &self.analysis.immutable_values);
                if order.trigger_ref.is_none() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "stop_market requires `trigger_ref.<variant>` as the second argument",
                        trigger_ref.span,
                    ));
                }
            }
            OrderSpecKind::StopLimit {
                trigger_price,
                limit_price,
                tif,
                post_only,
                trigger_ref,
                expire_time_ms,
            } => {
                order.kind = OrderKind::StopLimit;
                if let Some((field_id, slot)) = self.analyze_order_numeric_field(
                    role,
                    OrderFieldKind::TriggerPrice,
                    trigger_price,
                ) {
                    order.trigger_price_field_id = Some(field_id);
                    resolved.trigger_price_slot = Some(slot);
                }
                if let Some((field_id, slot)) =
                    self.analyze_order_numeric_field(role, OrderFieldKind::Price, limit_price)
                {
                    order.price_field_id = Some(field_id);
                    resolved.price_slot = Some(slot);
                }
                if let Some((field_id, slot)) = self.analyze_order_numeric_field(
                    role,
                    OrderFieldKind::ExpireTime,
                    expire_time_ms,
                ) {
                    order.expire_time_field_id = Some(field_id);
                    resolved.expire_time_slot = Some(slot);
                }
                self.diagnose_unknown_enum_literal(tif);
                order.tif = literal_time_in_force(tif, &self.analysis.immutable_values);
                if order.tif.is_none() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "stop_limit requires `tif.<variant>` as the third argument",
                        tif.span,
                    ));
                }
                match literal_bool(post_only, &self.analysis.immutable_values) {
                    Some(value) => order.post_only = value,
                    None => self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "stop_limit requires a bool literal or immutable bool binding as the fourth argument",
                        post_only.span,
                    )),
                }
                self.diagnose_unknown_enum_literal(trigger_ref);
                order.trigger_ref =
                    literal_trigger_reference(trigger_ref, &self.analysis.immutable_values);
                if order.trigger_ref.is_none() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "stop_limit requires `trigger_ref.<variant>` as the fifth argument",
                        trigger_ref.span,
                    ));
                }
            }
            OrderSpecKind::TakeProfitMarket {
                trigger_price,
                trigger_ref,
            } => {
                order.kind = OrderKind::TakeProfitMarket;
                if let Some((field_id, slot)) = self.analyze_order_numeric_field(
                    role,
                    OrderFieldKind::TriggerPrice,
                    trigger_price,
                ) {
                    order.trigger_price_field_id = Some(field_id);
                    resolved.trigger_price_slot = Some(slot);
                }
                self.diagnose_unknown_enum_literal(trigger_ref);
                order.trigger_ref =
                    literal_trigger_reference(trigger_ref, &self.analysis.immutable_values);
                if order.trigger_ref.is_none() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "take_profit_market requires `trigger_ref.<variant>` as the second argument",
                        trigger_ref.span,
                    ));
                }
            }
            OrderSpecKind::TakeProfitLimit {
                trigger_price,
                limit_price,
                tif,
                post_only,
                trigger_ref,
                expire_time_ms,
            } => {
                order.kind = OrderKind::TakeProfitLimit;
                if let Some((field_id, slot)) = self.analyze_order_numeric_field(
                    role,
                    OrderFieldKind::TriggerPrice,
                    trigger_price,
                ) {
                    order.trigger_price_field_id = Some(field_id);
                    resolved.trigger_price_slot = Some(slot);
                }
                if let Some((field_id, slot)) =
                    self.analyze_order_numeric_field(role, OrderFieldKind::Price, limit_price)
                {
                    order.price_field_id = Some(field_id);
                    resolved.price_slot = Some(slot);
                }
                if let Some((field_id, slot)) = self.analyze_order_numeric_field(
                    role,
                    OrderFieldKind::ExpireTime,
                    expire_time_ms,
                ) {
                    order.expire_time_field_id = Some(field_id);
                    resolved.expire_time_slot = Some(slot);
                }
                self.diagnose_unknown_enum_literal(tif);
                order.tif = literal_time_in_force(tif, &self.analysis.immutable_values);
                if order.tif.is_none() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "take_profit_limit requires `tif.<variant>` as the third argument",
                        tif.span,
                    ));
                }
                match literal_bool(post_only, &self.analysis.immutable_values) {
                    Some(value) => order.post_only = value,
                    None => self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "take_profit_limit requires a bool literal or immutable bool binding as the fourth argument",
                        post_only.span,
                    )),
                }
                self.diagnose_unknown_enum_literal(trigger_ref);
                order.trigger_ref =
                    literal_trigger_reference(trigger_ref, &self.analysis.immutable_values);
                if order.trigger_ref.is_none() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "take_profit_limit requires `trigger_ref.<variant>` as the fifth argument",
                        trigger_ref.span,
                    ));
                }
            }
        }

        self.analysis
            .resolved_order_field_slots
            .insert(stmt.id, resolved);
        self.analysis.orders.push(order);
    }

    fn analyze_order_size_stmt(&mut self, stmt: &Stmt, role: AstSignalRole, expr: &Expr) {
        let role = compiled_signal_role(role);
        if !matches!(
            role,
            CompiledSignalRole::LongEntry
                | CompiledSignalRole::LongEntry2
                | CompiledSignalRole::LongEntry3
                | CompiledSignalRole::ShortEntry
                | CompiledSignalRole::ShortEntry2
                | CompiledSignalRole::ShortEntry3
                | CompiledSignalRole::TargetLong
                | CompiledSignalRole::TargetLong2
                | CompiledSignalRole::TargetLong3
                | CompiledSignalRole::TargetShort
                | CompiledSignalRole::TargetShort2
                | CompiledSignalRole::TargetShort3
        ) {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "only `size entry long|short`, `size entry1..3 long|short`, `size target long|short`, and `size target1..3 long|short` are supported in v1",
                stmt.span,
            ));
            return;
        }
        if self.analysis.order_size_decls.contains_key(&role) {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!("duplicate size declaration for `{}`", role.canonical_name()),
                stmt.span,
            ));
            return;
        }

        let mut resolved = ResolvedOrderFieldSlots::default();
        let size_decl = match classify_order_size_expr(expr) {
            OrderSizeExpr::CapitalFraction(size_expr) => {
                let Some((field_id, slot)) =
                    self.analyze_order_numeric_field(role, OrderFieldKind::SizeFraction, size_expr)
                else {
                    return;
                };
                resolved.size_slot = Some(slot);
                Some(ResolvedOrderSizeDecl {
                    mode: SizeMode::CapitalFraction,
                    size_field_id: field_id,
                    risk_stop_field_id: None,
                })
            }
            OrderSizeExpr::RiskPct { pct, stop_price } => {
                if !role.is_entry() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "`risk_pct(...)` is only supported on staged entry size declarations in v1",
                        expr.span,
                    ));
                    return;
                }
                let Some((size_field_id, size_slot)) =
                    self.analyze_order_numeric_field(role, OrderFieldKind::SizeFraction, pct)
                else {
                    return;
                };
                let Some((risk_stop_field_id, risk_stop_slot)) = self.analyze_order_numeric_field(
                    role,
                    OrderFieldKind::RiskStopPrice,
                    stop_price,
                ) else {
                    return;
                };
                resolved.size_slot = Some(size_slot);
                resolved.risk_stop_slot = Some(risk_stop_slot);
                Some(ResolvedOrderSizeDecl {
                    mode: SizeMode::RiskPct,
                    size_field_id,
                    risk_stop_field_id: Some(risk_stop_field_id),
                })
            }
            OrderSizeExpr::InvalidCapitalFractionArity => {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "`capital_fraction(...)` expects exactly one argument",
                    expr.span,
                ));
                None
            }
            OrderSizeExpr::InvalidRiskPctArity => {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "`risk_pct(...)` expects exactly two arguments: risk_pct(pct, stop_price)",
                    expr.span,
                ));
                None
            }
        };
        if let Some(size_decl) = size_decl {
            self.analysis.order_size_decls.insert(role, size_decl);
            if let Some(order) = self
                .analysis
                .orders
                .iter_mut()
                .find(|decl| decl.role == role)
            {
                order.size_mode = Some(size_decl.mode);
                order.size_field_id = Some(size_decl.size_field_id);
                order.risk_stop_field_id = size_decl.risk_stop_field_id;
            }
        }
        self.analysis
            .resolved_order_field_slots
            .insert(stmt.id, resolved);
    }

    fn analyze_risk_control_stmt(
        &mut self,
        stmt: &Stmt,
        kind: AstRiskControlKind,
        side: PositionSide,
        expr: &Expr,
    ) {
        let compiled_kind = compiled_risk_control_kind(kind);
        if self
            .analysis
            .risk_controls
            .iter()
            .any(|decl| decl.kind == compiled_kind && decl.side == side)
        {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "duplicate {} declaration for `{}`",
                    risk_control_name(compiled_kind),
                    side_name(side)
                ),
                stmt.span,
            ));
            return;
        }

        let info = self.analyze_expr(expr);
        let Some(ty) = info.concrete() else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "{} requires a compile-time numeric scalar expression",
                    risk_control_name(compiled_kind)
                ),
                expr.span,
            ));
            return;
        };
        if ty != Type::F64 {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "{} requires a compile-time numeric scalar expression",
                    risk_control_name(compiled_kind)
                ),
                expr.span,
            ));
            return;
        }
        let Some(value) = eval_immutable_expr(expr, &self.analysis.immutable_values) else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "{} requires a compile-time numeric scalar expression",
                    risk_control_name(compiled_kind)
                ),
                expr.span,
            ));
            return;
        };
        let Value::F64(value) = value else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "{} requires a compile-time numeric scalar expression",
                    risk_control_name(compiled_kind)
                ),
                expr.span,
            ));
            return;
        };
        if !value.is_finite() || value < 0.0 || value.fract() != 0.0 {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "{} requires a non-negative whole number of bars",
                    risk_control_name(compiled_kind)
                ),
                expr.span,
            ));
            return;
        }

        self.analysis.risk_controls.push(RiskControlDecl {
            side,
            kind: compiled_kind,
            bars: value as usize,
        });
    }

    fn analyze_portfolio_control_stmt(
        &mut self,
        stmt: &Stmt,
        kind: AstPortfolioControlKind,
        expr: &Expr,
    ) {
        let compiled_kind = compiled_portfolio_control_kind(kind);
        if self
            .analysis
            .portfolio_controls
            .iter()
            .any(|decl| decl.kind == compiled_kind)
        {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "duplicate {} declaration",
                    portfolio_control_name(compiled_kind)
                ),
                stmt.span,
            ));
            return;
        }

        let info = self.analyze_expr(expr);
        let Some(ty) = info.concrete() else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "{} requires a compile-time numeric scalar expression",
                    portfolio_control_name(compiled_kind)
                ),
                expr.span,
            ));
            return;
        };
        if ty != Type::F64 {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "{} requires a compile-time numeric scalar expression",
                    portfolio_control_name(compiled_kind)
                ),
                expr.span,
            ));
            return;
        }
        let Some(value) = eval_immutable_expr(expr, &self.analysis.immutable_values) else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "{} requires a compile-time numeric scalar expression",
                    portfolio_control_name(compiled_kind)
                ),
                expr.span,
            ));
            return;
        };
        let Value::F64(value) = value else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "{} requires a compile-time numeric scalar expression",
                    portfolio_control_name(compiled_kind)
                ),
                expr.span,
            ));
            return;
        };
        if !value.is_finite() || value < 0.0 {
            let message = match compiled_kind {
                CompiledPortfolioControlKind::MaxPositions
                | CompiledPortfolioControlKind::MaxLongPositions
                | CompiledPortfolioControlKind::MaxShortPositions => format!(
                    "{} requires a non-negative whole number",
                    portfolio_control_name(compiled_kind)
                ),
                CompiledPortfolioControlKind::MaxGrossExposurePct
                | CompiledPortfolioControlKind::MaxNetExposurePct => format!(
                    "{} requires a finite non-negative exposure fraction",
                    portfolio_control_name(compiled_kind)
                ),
            };
            self.diagnostics
                .push(Diagnostic::new(DiagnosticKind::Type, message, expr.span));
            return;
        }
        if matches!(
            compiled_kind,
            CompiledPortfolioControlKind::MaxPositions
                | CompiledPortfolioControlKind::MaxLongPositions
                | CompiledPortfolioControlKind::MaxShortPositions
        ) && value.fract() != 0.0
        {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "{} requires a non-negative whole number",
                    portfolio_control_name(compiled_kind)
                ),
                expr.span,
            ));
            return;
        }
        self.analysis.portfolio_controls.push(PortfolioControlDecl {
            kind: compiled_kind,
            value,
        });
    }

    fn analyze_portfolio_group_stmt(&mut self, group: &crate::ast::PortfolioGroupDecl) {
        if self
            .analysis
            .portfolio_groups
            .iter()
            .any(|decl| decl.name == group.name)
        {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!("duplicate portfolio group `{}`", group.name),
                group.name_span,
            ));
            return;
        }
        if group.aliases.is_empty() {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "portfolio group `{}` must include at least one source alias",
                    group.name
                ),
                group.span,
            ));
            return;
        }
        let declared_aliases = self
            .analysis
            .declared_sources
            .iter()
            .map(|source| source.alias.as_str())
            .collect::<HashSet<_>>();
        let mut seen = HashSet::new();
        let mut aliases = Vec::with_capacity(group.aliases.len());
        for alias in &group.aliases {
            if !seen.insert(alias.name.clone()) {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!(
                        "portfolio group `{}` contains duplicate alias `{}`",
                        group.name, alias.name
                    ),
                    alias.span,
                ));
                continue;
            }
            if !declared_aliases.contains(alias.name.as_str()) {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!(
                        "portfolio group `{}` references unknown source alias `{}`",
                        group.name, alias.name
                    ),
                    alias.span,
                ));
                continue;
            }
            aliases.push(alias.name.clone());
        }
        self.analysis
            .portfolio_groups
            .push(CompiledPortfolioGroupDecl {
                name: group.name.clone(),
                aliases,
            });
    }

    fn diagnose_unknown_enum_literal(&mut self, expr: &Expr) {
        if let ExprKind::EnumVariant {
            namespace,
            variant,
            variant_span,
            ..
        } = &expr.kind
        {
            if resolve_enum_variant(namespace, variant).is_none() {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("unknown enum variant `{}.{}`", namespace, variant),
                    *variant_span,
                ));
            }
        }
    }

    fn analyze_order_numeric_field(
        &mut self,
        role: CompiledSignalRole,
        kind: OrderFieldKind,
        expr: &Expr,
    ) -> Option<(u16, u16)> {
        let info = if role.is_attached_exit() {
            let previous = self.active_attached_role.replace(role);
            let info = self.analyze_expr(expr);
            self.active_attached_role = previous;
            info
        } else {
            self.analyze_expr(expr)
        };
        if !info.ty.is_numeric_like() {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "order field `{}` requires numeric, series<float>, or na",
                    kind.as_str()
                ),
                expr.span,
            ));
            return None;
        }
        let hidden_name = format!("__order.{}.{}", role.canonical_name(), kind.as_str());
        let slot = self.define_symbol(
            hidden_name.clone(),
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: info.update_mask,
            },
            true,
            None,
        );
        let field_id = self.analysis.order_fields.len() as u16;
        self.analysis.order_fields.push(OrderFieldDecl {
            name: hidden_name,
            role,
            kind,
            slot,
        });
        Some((field_id, slot))
    }

    fn position_field_slot(&mut self, field: PositionField) -> u16 {
        if let Some(slot) = self.analysis.position_field_slots.get(&field).copied() {
            return slot;
        }
        let name = format!("__position.{}", field.as_str());
        let slot = self.define_symbol(
            name,
            ExprInfo::scalar(position_field_type(field)),
            true,
            None,
        );
        self.analysis
            .position_fields
            .push(PositionFieldDecl { field, slot });
        self.analysis.position_field_slots.insert(field, slot);
        slot
    }

    fn position_event_field_slot(&mut self, field: PositionEventField) -> u16 {
        if let Some(slot) = self
            .analysis
            .position_event_field_slots
            .get(&field)
            .copied()
        {
            return slot;
        }
        let name = format!("__position_event.{}", field.as_str());
        let slot = self.define_symbol(
            name,
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesBool),
                update_mask: BASE_UPDATE_MASK,
            },
            true,
            None,
        );
        self.analysis
            .position_event_fields
            .push(PositionEventFieldDecl { field, slot });
        self.analysis.position_event_field_slots.insert(field, slot);
        slot
    }

    fn last_exit_field_slot(&mut self, scope: LastExitScope, field: LastExitField) -> u16 {
        if let Some(slot) = self
            .analysis
            .last_exit_field_slots
            .get(&(scope, field))
            .copied()
        {
            return slot;
        }
        let name = format!("__{}.{}", scope.namespace(), field.as_str());
        let slot = self.define_symbol(
            name,
            ExprInfo::scalar(last_exit_field_type(field)),
            true,
            None,
        );
        self.analysis
            .last_exit_fields
            .push(LastExitFieldDecl { scope, field, slot });
        self.analysis
            .last_exit_field_slots
            .insert((scope, field), slot);
        slot
    }

    fn analyze_block(&mut self, block: &Block) {
        for stmt in &block.statements {
            self.analyze_stmt(stmt);
        }
    }

    fn analyze_expr(&mut self, expr: &Expr) -> ExprInfo {
        let info = match &expr.kind {
            ExprKind::Number(_) => ExprInfo::scalar(Type::F64),
            ExprKind::Bool(_) => ExprInfo::scalar(Type::Bool),
            ExprKind::Na => ExprInfo {
                ty: InferredType::Na,
                update_mask: 0,
            },
            ExprKind::String(_) => {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "string literals are only allowed in source declarations",
                    expr.span,
                ));
                ExprInfo::scalar(Type::F64)
            }
            ExprKind::EnumVariant {
                namespace,
                variant,
                variant_span,
                ..
            } => match resolve_enum_variant(namespace, variant) {
                Some(value) => {
                    let ty = scalar_type_for_value(&value);
                    self.analysis
                        .expr_info
                        .insert(expr.id, ExprInfo::scalar(ty));
                    ExprInfo::scalar(ty)
                }
                None => {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("unknown enum variant `{}.{}`", namespace, variant),
                        *variant_span,
                    ));
                    ExprInfo::scalar(Type::MaType)
                }
            },
            ExprKind::PositionField { field, field_span } => {
                if self.active_attached_role.is_none() {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "`position.*` is only available inside `protect` and `target` declarations",
                        *field_span,
                    ));
                }
                let ty = position_field_type(*field);
                self.position_field_slot(*field);
                ExprInfo::scalar(ty)
            }
            ExprKind::PositionEventField { field, .. } => {
                self.position_event_field_slot(*field);
                ExprInfo {
                    ty: InferredType::Concrete(Type::SeriesBool),
                    update_mask: BASE_UPDATE_MASK,
                }
            }
            ExprKind::LastExitField { scope, field, .. } => {
                self.last_exit_field_slot(*scope, *field);
                ExprInfo::scalar(last_exit_field_type(*field))
            }
            ExprKind::Ident(name) => {
                let Some(symbol) = self.lookup_symbol(name) else {
                    if is_predefined_series_name(name) {
                        self.diagnostics.push(Diagnostic::new(
                            DiagnosticKind::Type,
                            format!(
                                "scripts require source-qualified market series; found `{name}`"
                            ),
                            expr.span,
                        ));
                        return ExprInfo::series(BASE_UPDATE_MASK);
                    }
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("unknown identifier `{name}`"),
                        expr.span,
                    ));
                    return ExprInfo::scalar(Type::F64);
                };
                symbol.info
            }
            ExprKind::SourceSeries { interval, .. } => {
                ExprInfo::series(interval.map_or(BASE_UPDATE_MASK, Interval::mask))
            }
            ExprKind::Unary { op, expr: inner } => self.analyze_unary(*op, inner),
            ExprKind::Binary { op, left, right } => self.analyze_binary(*op, left, right),
            ExprKind::Conditional {
                condition,
                when_true,
                when_false,
            } => self.analyze_conditional(expr.span, condition, when_true, when_false),
            ExprKind::Call { callee, args, .. } => self.analyze_call(expr, callee, args),
            ExprKind::Index { target, index } => self.analyze_index(target, index, expr.span),
        };
        self.analysis.expr_info.insert(expr.id, info);
        info
    }

    fn analyze_unary(&mut self, op: UnaryOp, inner: &Expr) -> ExprInfo {
        let inner_info = self.analyze_expr(inner);
        let ty = infer_unary(op, inner_info.ty, inner.span, &mut self.diagnostics);
        ExprInfo {
            ty,
            update_mask: inner_info.update_mask,
        }
    }

    fn analyze_binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr) -> ExprInfo {
        let left_info = self.analyze_expr(left);
        let right_info = self.analyze_expr(right);
        let ty = infer_binary(
            op,
            left_info.ty,
            right_info.ty,
            left.span.merge(right.span),
            &mut self.diagnostics,
        );
        ExprInfo {
            ty,
            update_mask: left_info.update_mask | right_info.update_mask,
        }
    }

    fn analyze_conditional(
        &mut self,
        span: Span,
        condition: &Expr,
        when_true: &Expr,
        when_false: &Expr,
    ) -> ExprInfo {
        let condition_info = self.analyze_expr(condition);
        if !condition_info.ty.allow_bool() {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "conditional expression condition must be bool, series<bool>, or na",
                condition.span,
            ));
        }
        let true_info = self.analyze_expr(when_true);
        let false_info = self.analyze_expr(when_false);
        ExprInfo {
            ty: infer_conditional(true_info.ty, false_info.ty, span, &mut self.diagnostics),
            update_mask: condition_info.update_mask
                | true_info.update_mask
                | false_info.update_mask,
        }
    }

    fn analyze_call(&mut self, expr: &Expr, callee: &str, args: &[Expr]) -> ExprInfo {
        if let Some(builtin) = BuiltinId::from_name(callee) {
            return self.analyze_builtin_call(builtin, callee, args, expr.span, false);
        }
        if let Some(metadata) = talib_metadata_by_name(callee) {
            let arg_info: Vec<ExprInfo> = args.iter().map(|arg| self.analyze_expr(arg)).collect();
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "builtin `{callee}` is reserved by the TA-Lib catalog but is not implemented yet"
                ),
                expr.span,
            ));
            return fallback_expr_info_for_talib(metadata, &arg_info);
        }

        let arg_info: Vec<ExprInfo> = args.iter().map(|arg| self.analyze_expr(arg)).collect();
        let Some(function) = self.functions_by_name.get(callee).copied() else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!("unknown function `{callee}`"),
                expr.span,
            ));
            return ExprInfo::scalar(Type::F64);
        };

        if args.len() != function.params.len() {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "function `{callee}` expects {} argument(s), found {}",
                    function.params.len(),
                    args.len()
                ),
                expr.span,
            ));
            return ExprInfo::scalar(Type::F64);
        }

        let key = FunctionSpecializationKey {
            function_id: function.id,
            arg_shapes: arg_info
                .iter()
                .map(|info| FunctionArgShape {
                    ty: info.ty,
                    update_mask: info.update_mask,
                })
                .collect(),
        };
        self.analysis
            .user_function_calls
            .insert(expr.id, key.clone());
        self.ensure_function_specialization(&key, expr.span)
    }

    fn analyze_builtin_call(
        &mut self,
        builtin: BuiltinId,
        callee: &str,
        args: &[Expr],
        span: Span,
        in_function_body: bool,
    ) -> ExprInfo {
        match builtin {
            BuiltinId::Plot => {
                if in_function_body {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "function bodies may not call `plot`",
                        span,
                    ));
                }
                if args.len() != 1 {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "plot expects exactly one argument",
                        span,
                    ));
                    return ExprInfo::scalar(Type::Void);
                }
                let arg_info = self.analyze_expr(&args[0]);
                if !matches!(
                    arg_info.ty,
                    InferredType::Concrete(Type::F64 | Type::SeriesF64) | InferredType::Na
                ) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "plot expects a numeric or series numeric value",
                        args[0].span,
                    ));
                }
                ExprInfo::scalar(Type::Void)
            }
            BuiltinId::Open
            | BuiltinId::High
            | BuiltinId::Low
            | BuiltinId::Close
            | BuiltinId::Volume
            | BuiltinId::Time => {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "market data builtins are identifiers, not callable functions",
                    span,
                ));
                for arg in args {
                    self.analyze_expr(arg);
                }
                ExprInfo::series(0)
            }
            _ => {
                let arg_info: Vec<ExprInfo> =
                    args.iter().map(|arg| self.analyze_expr(arg)).collect();
                analyze_helper_builtin(
                    builtin,
                    callee,
                    args,
                    &arg_info,
                    &self.analysis.immutable_values,
                    span,
                    &mut self.diagnostics,
                )
            }
        }
    }

    fn analyze_index(&mut self, target: &Expr, index: &Expr, span: Span) -> ExprInfo {
        let target_info = self.analyze_expr(target);
        let Some(_) = literal_window(index, &self.analysis.immutable_values) else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "series indexing requires a non-negative integer literal",
                index.span,
            ));
            return ExprInfo::scalar(Type::F64);
        };
        let ty = match target_info.ty {
            InferredType::Concrete(Type::SeriesF64) => InferredType::Concrete(Type::F64),
            InferredType::Concrete(Type::SeriesBool) => InferredType::Concrete(Type::Bool),
            InferredType::Na => InferredType::Na,
            _ => {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "only series values can be indexed",
                    span,
                ));
                InferredType::Concrete(Type::F64)
            }
        };
        ExprInfo {
            ty,
            update_mask: target_info.update_mask,
        }
    }

    fn ensure_function_specialization(
        &mut self,
        key: &FunctionSpecializationKey,
        span: Span,
    ) -> ExprInfo {
        if let Some(spec) = self.analysis.function_specializations.get(key) {
            return spec.return_info;
        }
        if !self.active_specializations.insert(key.clone()) {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "recursive and cyclic function definitions are not allowed",
                span,
            ));
            return ExprInfo::scalar(Type::F64);
        }

        let return_info = match self.functions_by_id.get(&key.function_id).copied() {
            Some(function) => {
                let spec = FunctionAnalyzer::new(self, function, key.arg_shapes.clone()).analyze();
                let return_info = spec.return_info;
                self.analysis
                    .function_specializations
                    .insert(key.clone(), spec);
                return_info
            }
            None => {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "unknown function specialization target",
                    span,
                ));
                ExprInfo::scalar(Type::F64)
            }
        };

        self.active_specializations.remove(key);
        return_info
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_symbol(
        &mut self,
        name: String,
        info: ExprInfo,
        hidden: bool,
        market_binding: Option<MarketBinding>,
    ) -> u16 {
        let slot = self.analysis.locals.len() as u16;
        let concrete = info.ty.concrete().unwrap_or(Type::F64);
        let local = if concrete.is_series() {
            LocalInfo::series(
                if hidden { None } else { Some(name.clone()) },
                concrete,
                hidden,
                info.update_mask,
                market_binding,
            )
        } else {
            LocalInfo::scalar(
                if hidden { None } else { Some(name.clone()) },
                concrete,
                hidden,
            )
        };
        self.analysis.locals.push(local);
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name, AnalyzerSymbol { info });
        slot
    }

    fn lookup_symbol(&self, name: &str) -> Option<AnalyzerSymbol> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn is_function_visible_name(&self, name: &str) -> bool {
        self.analysis.immutable_bindings.contains_key(name)
    }
}

struct FunctionAnalyzer<'a, 'b> {
    parent: &'b mut Analyzer<'a>,
    function: &'a FunctionDecl,
    scopes: Vec<HashMap<String, AnalyzerSymbol>>,
    expr_info: HashMap<NodeId, ExprInfo>,
    user_function_calls: HashMap<NodeId, FunctionSpecializationKey>,
    param_bindings: Vec<FunctionParamBinding>,
}

impl<'a, 'b> FunctionAnalyzer<'a, 'b> {
    fn new(
        parent: &'b mut Analyzer<'a>,
        function: &'a FunctionDecl,
        arg_shapes: Vec<FunctionArgShape>,
    ) -> Self {
        let mut root: HashMap<String, AnalyzerSymbol> = parent
            .analysis
            .immutable_bindings
            .iter()
            .map(|(name, info)| (name.clone(), AnalyzerSymbol { info: *info }))
            .collect();

        let mut param_bindings = Vec::with_capacity(function.params.len());
        for (param, arg_shape) in function.params.iter().zip(arg_shapes) {
            let info = ExprInfo {
                ty: arg_shape.ty,
                update_mask: arg_shape.update_mask,
            };
            root.insert(param.name.clone(), AnalyzerSymbol { info });
            param_bindings.push(param_binding(arg_shape));
        }

        Self {
            parent,
            function,
            scopes: vec![root],
            expr_info: HashMap::new(),
            user_function_calls: HashMap::new(),
            param_bindings,
        }
    }

    fn analyze(mut self) -> FunctionSpecialization {
        let return_info = self.analyze_expr(&self.function.body);
        if matches!(return_info.ty, InferredType::Concrete(Type::Void)) {
            self.parent.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!("function `{}` must not return void", self.function.name),
                self.function.body.span,
            ));
        }
        FunctionSpecialization {
            expr_info: self.expr_info,
            user_function_calls: self.user_function_calls,
            return_info,
            param_bindings: self.param_bindings,
        }
    }

    fn analyze_expr(&mut self, expr: &Expr) -> ExprInfo {
        let info = match &expr.kind {
            ExprKind::Number(_) => ExprInfo::scalar(Type::F64),
            ExprKind::Bool(_) => ExprInfo::scalar(Type::Bool),
            ExprKind::Na => ExprInfo {
                ty: InferredType::Na,
                update_mask: 0,
            },
            ExprKind::String(_) => {
                self.parent.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "string literals are only allowed in source declarations",
                    expr.span,
                ));
                ExprInfo::scalar(Type::F64)
            }
            ExprKind::EnumVariant {
                namespace,
                variant,
                variant_span,
                ..
            } => match resolve_enum_variant(namespace, variant) {
                Some(value) => ExprInfo::scalar(scalar_type_for_value(&value)),
                None => {
                    self.parent.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("unknown enum variant `{}.{}`", namespace, variant),
                        *variant_span,
                    ));
                    ExprInfo::scalar(Type::MaType)
                }
            },
            ExprKind::PositionField { field_span, .. } => {
                self.parent.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "`position.*` is only available inside `protect` and `target` declarations",
                    *field_span,
                ));
                ExprInfo::scalar(Type::F64)
            }
            ExprKind::PositionEventField { .. } => ExprInfo {
                ty: InferredType::Concrete(Type::SeriesBool),
                update_mask: BASE_UPDATE_MASK,
            },
            ExprKind::LastExitField { field, .. } => ExprInfo::scalar(last_exit_field_type(*field)),
            ExprKind::Ident(name) => match self.lookup_symbol(name) {
                Some(symbol) => symbol.info,
                None => {
                    self.parent.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "function bodies may only reference parameters or declared source series; found `{name}`"
                        ),
                        expr.span,
                    ));
                    ExprInfo::scalar(Type::F64)
                }
            },
            ExprKind::SourceSeries { interval, .. } => {
                ExprInfo::series(interval.map_or(BASE_UPDATE_MASK, Interval::mask))
            }
            ExprKind::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                let condition_info = self.analyze_expr(condition);
                if !condition_info.ty.allow_bool() {
                    self.parent.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "conditional expression condition must be bool, series<bool>, or na",
                        condition.span,
                    ));
                }
                let true_info = self.analyze_expr(when_true);
                let false_info = self.analyze_expr(when_false);
                ExprInfo {
                    ty: infer_conditional(
                        true_info.ty,
                        false_info.ty,
                        expr.span,
                        &mut self.parent.diagnostics,
                    ),
                    update_mask: condition_info.update_mask
                        | true_info.update_mask
                        | false_info.update_mask,
                }
            }
            ExprKind::Unary { op, expr: inner } => {
                let inner_info = self.analyze_expr(inner);
                ExprInfo {
                    ty: infer_unary(*op, inner_info.ty, inner.span, &mut self.parent.diagnostics),
                    update_mask: inner_info.update_mask,
                }
            }
            ExprKind::Binary { op, left, right } => {
                let left_info = self.analyze_expr(left);
                let right_info = self.analyze_expr(right);
                ExprInfo {
                    ty: infer_binary(
                        *op,
                        left_info.ty,
                        right_info.ty,
                        left.span.merge(right.span),
                        &mut self.parent.diagnostics,
                    ),
                    update_mask: left_info.update_mask | right_info.update_mask,
                }
            }
            ExprKind::Call { callee, args, .. } => self.analyze_call(expr, callee, args),
            ExprKind::Index { target, index } => self.analyze_index(target, index, expr.span),
        };
        self.expr_info.insert(expr.id, info);
        info
    }

    fn analyze_call(&mut self, expr: &Expr, callee: &str, args: &[Expr]) -> ExprInfo {
        if let Some(builtin) = BuiltinId::from_name(callee) {
            return self.analyze_builtin_call(builtin, callee, args, expr.span);
        }
        if let Some(metadata) = talib_metadata_by_name(callee) {
            let arg_info: Vec<ExprInfo> = args.iter().map(|arg| self.analyze_expr(arg)).collect();
            self.parent.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "builtin `{callee}` is reserved by the TA-Lib catalog but is not implemented yet"
                ),
                expr.span,
            ));
            return fallback_expr_info_for_talib(metadata, &arg_info);
        }

        let arg_info: Vec<ExprInfo> = args.iter().map(|arg| self.analyze_expr(arg)).collect();
        let Some(function) = self.parent.functions_by_name.get(callee).copied() else {
            self.parent.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!("unknown function `{callee}`"),
                expr.span,
            ));
            return ExprInfo::scalar(Type::F64);
        };

        if args.len() != function.params.len() {
            self.parent.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                format!(
                    "function `{callee}` expects {} argument(s), found {}",
                    function.params.len(),
                    args.len()
                ),
                expr.span,
            ));
            return ExprInfo::scalar(Type::F64);
        }

        let key = FunctionSpecializationKey {
            function_id: function.id,
            arg_shapes: arg_info
                .iter()
                .map(|info| FunctionArgShape {
                    ty: info.ty,
                    update_mask: info.update_mask,
                })
                .collect(),
        };
        self.user_function_calls.insert(expr.id, key.clone());
        self.parent.ensure_function_specialization(&key, expr.span)
    }

    fn analyze_builtin_call(
        &mut self,
        builtin: BuiltinId,
        callee: &str,
        args: &[Expr],
        span: Span,
    ) -> ExprInfo {
        match builtin {
            BuiltinId::Plot => {
                self.parent.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "function bodies may not call `plot`",
                    span,
                ));
                if args.len() != 1 {
                    self.parent.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "plot expects exactly one argument",
                        span,
                    ));
                    return ExprInfo::scalar(Type::Void);
                }
                let arg_info = self.analyze_expr(&args[0]);
                if !matches!(
                    arg_info.ty,
                    InferredType::Concrete(Type::F64 | Type::SeriesF64) | InferredType::Na
                ) {
                    self.parent.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "plot expects a numeric or series numeric value",
                        args[0].span,
                    ));
                }
                ExprInfo::scalar(Type::Void)
            }
            BuiltinId::Open
            | BuiltinId::High
            | BuiltinId::Low
            | BuiltinId::Close
            | BuiltinId::Volume
            | BuiltinId::Time => {
                self.parent.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "market data builtins are identifiers, not callable functions",
                    span,
                ));
                for arg in args {
                    self.analyze_expr(arg);
                }
                ExprInfo::series(0)
            }
            _ => {
                let arg_info: Vec<ExprInfo> =
                    args.iter().map(|arg| self.analyze_expr(arg)).collect();
                analyze_helper_builtin(
                    builtin,
                    callee,
                    args,
                    &arg_info,
                    &self.parent.analysis.immutable_values,
                    span,
                    &mut self.parent.diagnostics,
                )
            }
        }
    }

    fn analyze_index(&mut self, target: &Expr, index: &Expr, span: Span) -> ExprInfo {
        let target_info = self.analyze_expr(target);
        let Some(_) = literal_window(index, &self.parent.analysis.immutable_values) else {
            self.parent.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "series indexing requires a non-negative integer literal",
                index.span,
            ));
            return ExprInfo::scalar(Type::F64);
        };
        let ty = match target_info.ty {
            InferredType::Concrete(Type::SeriesF64) => InferredType::Concrete(Type::F64),
            InferredType::Concrete(Type::SeriesBool) => InferredType::Concrete(Type::Bool),
            InferredType::Na => InferredType::Na,
            _ => {
                self.parent.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "only series values can be indexed",
                    span,
                ));
                InferredType::Concrete(Type::F64)
            }
        };
        ExprInfo {
            ty,
            update_mask: target_info.update_mask,
        }
    }

    fn lookup_symbol(&self, name: &str) -> Option<AnalyzerSymbol> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}

enum OrderSizeExpr<'a> {
    CapitalFraction(&'a Expr),
    RiskPct { pct: &'a Expr, stop_price: &'a Expr },
    InvalidCapitalFractionArity,
    InvalidRiskPctArity,
}

fn classify_order_size_expr(expr: &Expr) -> OrderSizeExpr<'_> {
    match &expr.kind {
        ExprKind::Call { callee, args, .. } if callee == "capital_fraction" => {
            if args.len() == 1 {
                OrderSizeExpr::CapitalFraction(&args[0])
            } else {
                OrderSizeExpr::InvalidCapitalFractionArity
            }
        }
        ExprKind::Call { callee, args, .. } if callee == "risk_pct" => {
            if args.len() == 2 {
                OrderSizeExpr::RiskPct {
                    pct: &args[0],
                    stop_price: &args[1],
                }
            } else {
                OrderSizeExpr::InvalidRiskPctArity
            }
        }
        _ => OrderSizeExpr::CapitalFraction(expr),
    }
}

fn collect_source_series_stmt(
    stmt: &Stmt,
    refs: &mut BTreeSet<(String, Option<Interval>, MarketField)>,
) {
    match &stmt.kind {
        StmtKind::Let { expr, .. }
        | StmtKind::Const { expr, .. }
        | StmtKind::Input { expr, .. }
        | StmtKind::LetTuple { expr, .. }
        | StmtKind::Export { expr, .. }
        | StmtKind::Regime { expr, .. }
        | StmtKind::Trigger { expr, .. }
        | StmtKind::Signal { expr, .. }
        | StmtKind::RiskControl { expr, .. }
        | StmtKind::PortfolioControl { expr, .. }
        | StmtKind::Expr(expr) => collect_source_series_refs(expr, refs),
        StmtKind::Order { spec, .. } => collect_order_spec_series_refs(spec, refs),
        StmtKind::OrderSize { expr, .. } => collect_source_series_refs(expr, refs),
        StmtKind::PortfolioGroup { .. } => {}
        StmtKind::If {
            condition,
            then_block,
            else_block,
        } => {
            collect_source_series_refs(condition, refs);
            for stmt in &then_block.statements {
                collect_source_series_stmt(stmt, refs);
            }
            for stmt in &else_block.statements {
                collect_source_series_stmt(stmt, refs);
            }
        }
    }
}

fn collect_order_spec_series_refs(
    spec: &OrderSpec,
    refs: &mut BTreeSet<(String, Option<Interval>, MarketField)>,
) {
    match &spec.kind {
        OrderSpecKind::Market => {}
        OrderSpecKind::Limit { price, .. } => collect_source_series_refs(price, refs),
        OrderSpecKind::StopMarket { trigger_price, .. }
        | OrderSpecKind::TakeProfitMarket { trigger_price, .. } => {
            collect_source_series_refs(trigger_price, refs);
        }
        OrderSpecKind::StopLimit {
            trigger_price,
            limit_price,
            expire_time_ms,
            ..
        }
        | OrderSpecKind::TakeProfitLimit {
            trigger_price,
            limit_price,
            expire_time_ms,
            ..
        } => {
            collect_source_series_refs(trigger_price, refs);
            collect_source_series_refs(limit_price, refs);
            collect_source_series_refs(expire_time_ms, refs);
        }
    }
}

fn collect_source_series_refs(
    expr: &Expr,
    refs: &mut BTreeSet<(String, Option<Interval>, MarketField)>,
) {
    match &expr.kind {
        ExprKind::SourceSeries {
            source,
            interval,
            field,
            ..
        } => {
            refs.insert((source.clone(), *interval, *field));
        }
        ExprKind::Unary { expr, .. } => collect_source_series_refs(expr, refs),
        ExprKind::Binary { left, right, .. } => {
            collect_source_series_refs(left, refs);
            collect_source_series_refs(right, refs);
        }
        ExprKind::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            collect_source_series_refs(condition, refs);
            collect_source_series_refs(when_true, refs);
            collect_source_series_refs(when_false, refs);
        }
        ExprKind::Call { args, .. } => {
            for arg in args {
                collect_source_series_refs(arg, refs);
            }
        }
        ExprKind::Index { target, index } => {
            collect_source_series_refs(target, refs);
            collect_source_series_refs(index, refs);
        }
        ExprKind::Number(_)
        | ExprKind::Bool(_)
        | ExprKind::Na
        | ExprKind::String(_)
        | ExprKind::Ident(_)
        | ExprKind::PositionField { .. }
        | ExprKind::PositionEventField { .. }
        | ExprKind::LastExitField { .. }
        | ExprKind::EnumVariant { .. } => {}
    }
}

fn source_ref_span(ast: &Ast, source: &str, target: Option<Interval>) -> Option<Span> {
    for function in &ast.functions {
        if let Some(span) = expr_source_ref_span(&function.body, source, target) {
            return Some(span);
        }
    }
    for stmt in &ast.statements {
        if let Some(span) = stmt_source_ref_span(stmt, source, target) {
            return Some(span);
        }
    }
    None
}

fn stmt_source_ref_span(stmt: &Stmt, source: &str, target: Option<Interval>) -> Option<Span> {
    match &stmt.kind {
        StmtKind::Let { expr, .. }
        | StmtKind::Const { expr, .. }
        | StmtKind::Input { expr, .. }
        | StmtKind::LetTuple { expr, .. }
        | StmtKind::Export { expr, .. }
        | StmtKind::Regime { expr, .. }
        | StmtKind::Trigger { expr, .. }
        | StmtKind::Signal { expr, .. }
        | StmtKind::RiskControl { expr, .. }
        | StmtKind::PortfolioControl { expr, .. }
        | StmtKind::Expr(expr) => expr_source_ref_span(expr, source, target),
        StmtKind::Order { spec, .. } => order_spec_source_ref_span(spec, source, target),
        StmtKind::OrderSize { expr, .. } => expr_source_ref_span(expr, source, target),
        StmtKind::PortfolioGroup { .. } => None,
        StmtKind::If {
            condition,
            then_block,
            else_block,
        } => expr_source_ref_span(condition, source, target)
            .or_else(|| {
                then_block
                    .statements
                    .iter()
                    .find_map(|stmt| stmt_source_ref_span(stmt, source, target))
            })
            .or_else(|| {
                else_block
                    .statements
                    .iter()
                    .find_map(|stmt| stmt_source_ref_span(stmt, source, target))
            }),
    }
}

fn order_spec_source_ref_span(
    spec: &OrderSpec,
    source: &str,
    target: Option<Interval>,
) -> Option<Span> {
    match &spec.kind {
        OrderSpecKind::Market => None,
        OrderSpecKind::Limit { price, .. } => expr_source_ref_span(price, source, target),
        OrderSpecKind::StopMarket { trigger_price, .. }
        | OrderSpecKind::TakeProfitMarket { trigger_price, .. } => {
            expr_source_ref_span(trigger_price, source, target)
        }
        OrderSpecKind::StopLimit {
            trigger_price,
            limit_price,
            expire_time_ms,
            ..
        }
        | OrderSpecKind::TakeProfitLimit {
            trigger_price,
            limit_price,
            expire_time_ms,
            ..
        } => expr_source_ref_span(trigger_price, source, target)
            .or_else(|| expr_source_ref_span(limit_price, source, target))
            .or_else(|| expr_source_ref_span(expire_time_ms, source, target)),
    }
}

fn expr_source_ref_span(expr: &Expr, source: &str, target: Option<Interval>) -> Option<Span> {
    match &expr.kind {
        ExprKind::SourceSeries {
            source: expr_source,
            interval,
            ..
        } if expr_source == source && *interval == target => Some(expr.span),
        ExprKind::Unary { expr, .. } => expr_source_ref_span(expr, source, target),
        ExprKind::Binary { left, right, .. } => expr_source_ref_span(left, source, target)
            .or_else(|| expr_source_ref_span(right, source, target)),
        ExprKind::Conditional {
            condition,
            when_true,
            when_false,
        } => expr_source_ref_span(condition, source, target)
            .or_else(|| expr_source_ref_span(when_true, source, target))
            .or_else(|| expr_source_ref_span(when_false, source, target)),
        ExprKind::Call { args, .. } => args
            .iter()
            .find_map(|arg| expr_source_ref_span(arg, source, target)),
        ExprKind::Index {
            target: inner,
            index,
        } => expr_source_ref_span(inner, source, target)
            .or_else(|| expr_source_ref_span(index, source, target)),
        ExprKind::Number(_)
        | ExprKind::Bool(_)
        | ExprKind::Na
        | ExprKind::String(_)
        | ExprKind::Ident(_)
        | ExprKind::PositionField { .. }
        | ExprKind::PositionEventField { .. }
        | ExprKind::LastExitField { .. }
        | ExprKind::EnumVariant { .. }
        | ExprKind::SourceSeries { .. } => None,
    }
}

fn called_user_functions<'a>(
    expr: &'a Expr,
    functions_by_name: &'a HashMap<String, &'a FunctionDecl>,
) -> Vec<&'a str> {
    let mut calls = Vec::new();
    collect_called_user_functions(expr, functions_by_name, &mut calls);
    calls
}

fn collect_called_user_functions<'a>(
    expr: &'a Expr,
    functions_by_name: &'a HashMap<String, &'a FunctionDecl>,
    calls: &mut Vec<&'a str>,
) {
    match &expr.kind {
        ExprKind::Unary { expr, .. } => {
            collect_called_user_functions(expr, functions_by_name, calls)
        }
        ExprKind::Binary { left, right, .. } => {
            collect_called_user_functions(left, functions_by_name, calls);
            collect_called_user_functions(right, functions_by_name, calls);
        }
        ExprKind::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            collect_called_user_functions(condition, functions_by_name, calls);
            collect_called_user_functions(when_true, functions_by_name, calls);
            collect_called_user_functions(when_false, functions_by_name, calls);
        }
        ExprKind::Call { callee, args, .. } => {
            if functions_by_name.contains_key(callee) {
                calls.push(callee.as_str());
            }
            for arg in args {
                collect_called_user_functions(arg, functions_by_name, calls);
            }
        }
        ExprKind::Index { target, index } => {
            collect_called_user_functions(target, functions_by_name, calls);
            collect_called_user_functions(index, functions_by_name, calls);
        }
        ExprKind::Number(_)
        | ExprKind::Bool(_)
        | ExprKind::Na
        | ExprKind::String(_)
        | ExprKind::Ident(_)
        | ExprKind::PositionField { .. }
        | ExprKind::PositionEventField { .. }
        | ExprKind::LastExitField { .. }
        | ExprKind::EnumVariant { .. }
        | ExprKind::SourceSeries { .. } => {}
    }
}

fn infer_unary(
    op: UnaryOp,
    inner_ty: InferredType,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> InferredType {
    match op {
        UnaryOp::Neg => {
            if inner_ty.is_numeric_like() {
                match inner_ty {
                    InferredType::Concrete(Type::SeriesF64) => {
                        InferredType::Concrete(Type::SeriesF64)
                    }
                    InferredType::Na => InferredType::Na,
                    _ => InferredType::Concrete(Type::F64),
                }
            } else {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "unary `-` requires numeric input",
                    span,
                ));
                InferredType::Concrete(Type::F64)
            }
        }
        UnaryOp::Not => {
            if inner_ty.allow_bool() {
                match inner_ty {
                    InferredType::Concrete(Type::SeriesBool) => {
                        InferredType::Concrete(Type::SeriesBool)
                    }
                    InferredType::Na => InferredType::Na,
                    _ => InferredType::Concrete(Type::Bool),
                }
            } else {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "unary `!` requires bool input",
                    span,
                ));
                InferredType::Concrete(Type::Bool)
            }
        }
    }
}

fn infer_binary(
    op: BinaryOp,
    left_ty: InferredType,
    right_ty: InferredType,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> InferredType {
    match op {
        BinaryOp::And | BinaryOp::Or => {
            if !(left_ty.allow_bool() && right_ty.allow_bool()) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "logical operators require bool, series<bool>, or na operands",
                    span,
                ));
            }
            if matches!(
                (left_ty, right_ty),
                (InferredType::Concrete(Type::SeriesBool), _)
                    | (_, InferredType::Concrete(Type::SeriesBool))
            ) {
                InferredType::Concrete(Type::SeriesBool)
            } else if matches!((left_ty, right_ty), (InferredType::Na, InferredType::Na)) {
                InferredType::Na
            } else {
                InferredType::Concrete(Type::Bool)
            }
        }
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
            if !(left_ty.is_numeric_like() && right_ty.is_numeric_like()) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "arithmetic operators require numeric operands",
                    span,
                ));
            }
            if matches!(
                (left_ty, right_ty),
                (InferredType::Concrete(Type::SeriesF64), _)
                    | (_, InferredType::Concrete(Type::SeriesF64))
            ) {
                InferredType::Concrete(Type::SeriesF64)
            } else if matches!((left_ty, right_ty), (InferredType::Na, InferredType::Na)) {
                InferredType::Na
            } else {
                InferredType::Concrete(Type::F64)
            }
        }
        BinaryOp::Eq | BinaryOp::Ne => match (left_ty, right_ty) {
            (InferredType::Concrete(Type::SeriesBool), _)
            | (_, InferredType::Concrete(Type::SeriesBool))
            | (InferredType::Concrete(Type::SeriesF64), _)
            | (_, InferredType::Concrete(Type::SeriesF64)) => {
                InferredType::Concrete(Type::SeriesBool)
            }
            (InferredType::Na, InferredType::Na) => InferredType::Na,
            _ => InferredType::Concrete(Type::Bool),
        },
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
            if !(left_ty.is_numeric_like() && right_ty.is_numeric_like()) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "comparison operators require numeric operands",
                    span,
                ));
            }
            if matches!(
                (left_ty, right_ty),
                (InferredType::Concrete(Type::SeriesF64), _)
                    | (_, InferredType::Concrete(Type::SeriesF64))
            ) {
                InferredType::Concrete(Type::SeriesBool)
            } else if matches!((left_ty, right_ty), (InferredType::Na, InferredType::Na)) {
                InferredType::Na
            } else {
                InferredType::Concrete(Type::Bool)
            }
        }
    }
}

fn infer_conditional(
    when_true: InferredType,
    when_false: InferredType,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> InferredType {
    match (when_true, when_false) {
        (InferredType::Concrete(Type::F64), InferredType::Concrete(Type::Bool))
        | (InferredType::Concrete(Type::Bool), InferredType::Concrete(Type::F64)) => {
            diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "conditional expression branches must resolve to compatible types",
                span,
            ));
            InferredType::Concrete(Type::F64)
        }
        (InferredType::Na, other) | (other, InferredType::Na) => other,
        (InferredType::Concrete(Type::SeriesF64), _)
        | (_, InferredType::Concrete(Type::SeriesF64)) => {
            if !(when_true.is_numeric_like() && when_false.is_numeric_like()) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "conditional expression branches must resolve to compatible types",
                    span,
                ));
            }
            InferredType::Concrete(Type::SeriesF64)
        }
        (InferredType::Concrete(Type::SeriesBool), _)
        | (_, InferredType::Concrete(Type::SeriesBool)) => {
            if !(when_true.allow_bool() && when_false.allow_bool()) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "conditional expression branches must resolve to compatible types",
                    span,
                ));
            }
            InferredType::Concrete(Type::SeriesBool)
        }
        (InferredType::Concrete(Type::F64), InferredType::Concrete(Type::F64)) => {
            InferredType::Concrete(Type::F64)
        }
        (InferredType::Concrete(Type::Bool), InferredType::Concrete(Type::Bool)) => {
            InferredType::Concrete(Type::Bool)
        }
        (InferredType::Concrete(Type::MaType), InferredType::Concrete(Type::MaType)) => {
            InferredType::Concrete(Type::MaType)
        }
        (InferredType::Concrete(Type::TimeInForce), InferredType::Concrete(Type::TimeInForce)) => {
            InferredType::Concrete(Type::TimeInForce)
        }
        (
            InferredType::Concrete(Type::TriggerReference),
            InferredType::Concrete(Type::TriggerReference),
        ) => InferredType::Concrete(Type::TriggerReference),
        (
            InferredType::Concrete(Type::PositionSide),
            InferredType::Concrete(Type::PositionSide),
        ) => InferredType::Concrete(Type::PositionSide),
        (InferredType::Concrete(Type::ExitKind), InferredType::Concrete(Type::ExitKind)) => {
            InferredType::Concrete(Type::ExitKind)
        }
        (InferredType::Tuple2(left), InferredType::Tuple2(right)) if left == right => {
            InferredType::Tuple2(left)
        }
        (InferredType::Tuple3(left), InferredType::Tuple3(right)) if left == right => {
            InferredType::Tuple3(left)
        }
        _ => {
            diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "conditional expression branches must resolve to compatible types",
                span,
            ));
            InferredType::Concrete(Type::F64)
        }
    }
}

fn analyze_helper_builtin(
    builtin: BuiltinId,
    callee: &str,
    args: &[Expr],
    arg_info: &[ExprInfo],
    immutable_values: &HashMap<String, Value>,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> ExprInfo {
    let expected_arity = builtin.arity();
    if matches!(expected_arity, BuiltinArity::NonCallable) {
        diagnostics.push(Diagnostic::new(
            DiagnosticKind::Type,
            "market data builtins are identifiers, not callable functions",
            span,
        ));
        return ExprInfo::series(0);
    }
    if !expected_arity.accepts(args.len()) {
        diagnostics.push(Diagnostic::new(
            DiagnosticKind::Type,
            expected_arity_message(callee, expected_arity),
            span,
        ));
        return fallback_expr_info_for_builtin(builtin, arg_info);
    }

    match builtin.kind() {
        BuiltinKind::Indicator | BuiltinKind::Highest | BuiltinKind::Lowest => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            validate_positive_window_literal(callee, &args[1], immutable_values, diagnostics);
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::HighestBars | BuiltinKind::LowestBars => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            validate_positive_window_literal(callee, &args[1], immutable_values, diagnostics);
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::Rising | BuiltinKind::Falling => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            validate_positive_window_literal(callee, &args[1], immutable_values, diagnostics);
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesBool),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::MovingAverage => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            validate_positive_window_literal(callee, &args[1], immutable_values, diagnostics);
            if !matches!(arg_info[2].ty, InferredType::Concrete(Type::MaType)) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires ma_type as the third argument"),
                    args[2].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::MaOscillator => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if args.len() >= 2 {
                validate_min_window_literal(callee, &args[1], immutable_values, 2, diagnostics);
            }
            if args.len() >= 3 {
                validate_min_window_literal(callee, &args[2], immutable_values, 2, diagnostics);
            }
            if args.len() == 4 && !matches!(arg_info[3].ty, InferredType::Concrete(Type::MaType)) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires ma_type as the fourth argument"),
                    args[3].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::IndicatorTuple => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            validate_positive_window_literal(callee, &args[1], immutable_values, diagnostics);
            validate_positive_window_literal(callee, &args[2], immutable_values, diagnostics);
            validate_positive_window_literal(callee, &args[3], immutable_values, diagnostics);
            ExprInfo {
                ty: InferredType::Tuple3([Type::SeriesF64, Type::SeriesF64, Type::SeriesF64]),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::IndicatorTupleSignal => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if args.len() == 2 {
                validate_positive_window_literal(callee, &args[1], immutable_values, diagnostics);
            }
            ExprInfo {
                ty: InferredType::Tuple3([Type::SeriesF64, Type::SeriesF64, Type::SeriesF64]),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::IndicatorTupleMa => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if args.len() >= 2 {
                validate_min_window_literal(callee, &args[1], immutable_values, 2, diagnostics);
            }
            if args.len() >= 3 && !matches!(arg_info[2].ty, InferredType::Concrete(Type::MaType)) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires ma_type as the third argument"),
                    args[2].span,
                ));
            }
            if args.len() >= 4 {
                validate_min_window_literal(callee, &args[3], immutable_values, 2, diagnostics);
            }
            if args.len() >= 5 && !matches!(arg_info[4].ty, InferredType::Concrete(Type::MaType)) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires ma_type as the fifth argument"),
                    args[4].span,
                ));
            }
            if args.len() >= 6 {
                validate_min_window_literal(callee, &args[5], immutable_values, 1, diagnostics);
            }
            if args.len() == 7 && !matches!(arg_info[6].ty, InferredType::Concrete(Type::MaType)) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires ma_type as the seventh argument"),
                    args[6].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Tuple3([Type::SeriesF64, Type::SeriesF64, Type::SeriesF64]),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::Bands => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if args.len() >= 2 {
                validate_positive_window_literal(callee, &args[1], immutable_values, diagnostics);
            }
            if args.len() >= 3 && !arg_info[2].ty.is_scalar_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} deviations_up must be a numeric scalar value"),
                    args[2].span,
                ));
            }
            if args.len() >= 4 && !arg_info[3].ty.is_scalar_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} deviations_down must be a numeric scalar value"),
                    args[3].span,
                ));
            }
            if args.len() == 5 && !matches!(arg_info[4].ty, InferredType::Concrete(Type::MaType)) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires ma_type as the fifth argument"),
                    args[4].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Tuple3([Type::SeriesF64, Type::SeriesF64, Type::SeriesF64]),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::RollingHighLowCloseBands => {
            let high_info = arg_info[0];
            let low_info = arg_info[1];
            let close_info = arg_info[2];
            for (index, (arg, info)) in args.iter().zip(arg_info.iter()).take(3).enumerate() {
                if !info.ty.is_series_numeric() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "{callee} requires series<float> as the {} argument",
                            match index {
                                0 => "first",
                                1 => "second",
                                2 => "third",
                                _ => unreachable!(),
                            }
                        ),
                        arg.span,
                    ));
                }
            }
            if args.len() == 4 {
                validate_min_window_literal(callee, &args[3], immutable_values, 2, diagnostics);
            }
            ExprInfo {
                ty: InferredType::Tuple3([Type::SeriesF64, Type::SeriesF64, Type::SeriesF64]),
                update_mask: high_info.update_mask | low_info.update_mask | close_info.update_mask,
            }
        }
        BuiltinKind::UnaryMathTransform
        | BuiltinKind::NumericBinary
        | BuiltinKind::PriceTransform => {
            for (arg, info) in args.iter().zip(arg_info.iter()) {
                if !info.ty.is_numeric_like() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} requires numeric or series numeric arguments"),
                        arg.span,
                    ));
                }
            }
            numeric_result(arg_info)
        }
        BuiltinKind::CurrentOhlc => {
            for (index, (arg, info)) in args.iter().zip(arg_info.iter()).enumerate() {
                if !info.ty.is_series_numeric() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "{callee} requires series<float> as the {} argument",
                            match index {
                                0 => "first",
                                1 => "second",
                                2 => "third",
                                3 => "fourth",
                                _ => unreachable!(),
                            }
                        ),
                        arg.span,
                    ));
                }
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: arg_info
                    .iter()
                    .fold(0, |mask, info| mask | info.update_mask),
            }
        }
        BuiltinKind::RollingSingleInput => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if args.len() == 2 {
                let minimum = if matches!(
                    builtin,
                    BuiltinId::Dema
                        | BuiltinId::Tema
                        | BuiltinId::Trima
                        | BuiltinId::Kama
                        | BuiltinId::Trix
                        | BuiltinId::Zscore
                        | BuiltinId::UlcerIndex
                ) {
                    1
                } else {
                    2
                };
                validate_min_window_literal(
                    callee,
                    &args[1],
                    immutable_values,
                    minimum,
                    diagnostics,
                );
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::RollingSingleInputPercentile => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if args.len() >= 2 {
                validate_min_window_literal(callee, &args[1], immutable_values, 1, diagnostics);
            }
            if args.len() == 3 && !arg_info[2].ty.is_scalar_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} percentage must be a numeric scalar value"),
                    args[2].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::RollingSingleInputFactor => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if args.len() >= 2 {
                let minimum = if matches!(builtin, BuiltinId::Var | BuiltinId::T3) {
                    1
                } else {
                    2
                };
                validate_min_window_literal(
                    callee,
                    &args[1],
                    immutable_values,
                    minimum,
                    diagnostics,
                );
            }
            if args.len() == 3 && !arg_info[2].ty.is_scalar_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} deviations must be a numeric scalar value"),
                    args[2].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::RollingSingleInputTuple => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if args.len() == 2 {
                validate_min_window_literal(callee, &args[1], immutable_values, 2, diagnostics);
            }
            ExprInfo {
                ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesF64]),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::RollingHighLowTuple => {
            let high_info = arg_info[0];
            let low_info = arg_info[1];
            if !high_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if !low_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the second argument"),
                    args[1].span,
                ));
            }
            if args.len() == 3 {
                validate_min_window_literal(callee, &args[2], immutable_values, 2, diagnostics);
            }
            ExprInfo {
                ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesF64]),
                update_mask: high_info.update_mask | low_info.update_mask,
            }
        }
        BuiltinKind::RollingHighLowBands => {
            let high_info = arg_info[0];
            let low_info = arg_info[1];
            if !high_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if !low_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the second argument"),
                    args[1].span,
                ));
            }
            if args.len() == 3 {
                validate_min_window_literal(callee, &args[2], immutable_values, 1, diagnostics);
            }
            ExprInfo {
                ty: InferredType::Tuple3([Type::SeriesF64, Type::SeriesF64, Type::SeriesF64]),
                update_mask: high_info.update_mask | low_info.update_mask,
            }
        }
        BuiltinKind::RollingDoubleInput => {
            let left_info = arg_info[0];
            let right_info = arg_info[1];
            if !left_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if !right_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the second argument"),
                    args[1].span,
                ));
            }
            if args.len() == 3 {
                validate_min_window_literal(callee, &args[2], immutable_values, 1, diagnostics);
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: left_info.update_mask | right_info.update_mask,
            }
        }
        BuiltinKind::RollingHighLow => {
            let high_info = arg_info[0];
            let low_info = arg_info[1];
            if !high_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if !low_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the second argument"),
                    args[1].span,
                ));
            }
            if args.len() == 3 {
                let minimum = if matches!(builtin, BuiltinId::PlusDm | BuiltinId::MinusDm) {
                    1
                } else {
                    2
                };
                validate_min_window_literal(
                    callee,
                    &args[2],
                    immutable_values,
                    minimum,
                    diagnostics,
                );
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: high_info.update_mask | low_info.update_mask,
            }
        }
        BuiltinKind::RollingHighLowClose => {
            let high_info = arg_info[0];
            let low_info = arg_info[1];
            let close_info = arg_info[2];
            if !high_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if !low_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the second argument"),
                    args[1].span,
                ));
            }
            if !close_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the third argument"),
                    args[2].span,
                ));
            }
            if args.len() == 4 {
                let minimum = if matches!(
                    builtin,
                    BuiltinId::Atr
                        | BuiltinId::Natr
                        | BuiltinId::PlusDi
                        | BuiltinId::MinusDi
                        | BuiltinId::Dx
                        | BuiltinId::Adx
                        | BuiltinId::Adxr
                ) {
                    1
                } else {
                    2
                };
                validate_min_window_literal(
                    callee,
                    &args[3],
                    immutable_values,
                    minimum,
                    diagnostics,
                );
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: high_info.update_mask | low_info.update_mask | close_info.update_mask,
            }
        }
        BuiltinKind::RollingQuadInputWindow => {
            let update_mask = arg_info
                .iter()
                .fold(0, |mask, info| mask | info.update_mask);
            for (index, (arg, info)) in args.iter().zip(arg_info.iter()).take(4).enumerate() {
                if !info.ty.is_series_numeric() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "{callee} requires series<float> as the {} argument",
                            match index {
                                0 => "first",
                                1 => "second",
                                2 => "third",
                                3 => "fourth",
                                _ => unreachable!(),
                            }
                        ),
                        arg.span,
                    ));
                }
            }
            if args.len() == 5 {
                validate_min_window_literal(callee, &args[4], immutable_values, 1, diagnostics);
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask,
            }
        }
        BuiltinKind::RollingQuadInputDoubleWindow => {
            let update_mask = arg_info
                .iter()
                .fold(0, |mask, info| mask | info.update_mask);
            for (index, (arg, info)) in args.iter().zip(arg_info.iter()).take(4).enumerate() {
                if !info.ty.is_series_numeric() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "{callee} requires series<float> as the {} argument",
                            match index {
                                0 => "first",
                                1 => "second",
                                2 => "third",
                                3 => "fourth",
                                _ => unreachable!(),
                            }
                        ),
                        arg.span,
                    ));
                }
            }
            if args.len() >= 5 {
                validate_min_window_literal(callee, &args[4], immutable_values, 1, diagnostics);
            }
            if args.len() == 6 {
                validate_min_window_literal(callee, &args[5], immutable_values, 1, diagnostics);
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask,
            }
        }
        BuiltinKind::RollingHighLowCloseTuple => {
            let update_mask = arg_info
                .iter()
                .take(3)
                .fold(0, |mask, info| mask | info.update_mask);
            for (index, (arg, info)) in args.iter().zip(arg_info.iter()).take(3).enumerate() {
                if !info.ty.is_series_numeric() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "{callee} requires series<float> as the {} argument",
                            match index {
                                0 => "first",
                                1 => "second",
                                2 => "third",
                                _ => unreachable!(),
                            }
                        ),
                        arg.span,
                    ));
                }
            }
            if args.len() >= 4 {
                validate_min_window_literal(callee, &args[3], immutable_values, 1, diagnostics);
            }
            if matches!(builtin, BuiltinId::Stoch) {
                if args.len() >= 5 {
                    validate_min_window_literal(callee, &args[4], immutable_values, 1, diagnostics);
                }
                if args.len() >= 6
                    && !matches!(arg_info[5].ty, InferredType::Concrete(Type::MaType))
                {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} requires ma_type as the sixth argument"),
                        args[5].span,
                    ));
                }
                if args.len() >= 7 {
                    validate_min_window_literal(callee, &args[6], immutable_values, 1, diagnostics);
                }
                if args.len() == 8
                    && !matches!(arg_info[7].ty, InferredType::Concrete(Type::MaType))
                {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} requires ma_type as the eighth argument"),
                        args[7].span,
                    ));
                }
            } else {
                if args.len() >= 5 {
                    validate_min_window_literal(callee, &args[4], immutable_values, 1, diagnostics);
                }
                if args.len() == 6
                    && !matches!(arg_info[5].ty, InferredType::Concrete(Type::MaType))
                {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} requires ma_type as the sixth argument"),
                        args[5].span,
                    ));
                }
            }
            ExprInfo {
                ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesF64]),
                update_mask,
            }
        }
        BuiltinKind::RollingHighLowCloseTrendTuple => {
            let update_mask = arg_info
                .iter()
                .take(3)
                .fold(0, |mask, info| mask | info.update_mask);
            for (index, (arg, info)) in args.iter().zip(arg_info.iter()).take(3).enumerate() {
                if !info.ty.is_series_numeric() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "{callee} requires series<float> as the {} argument",
                            match index {
                                0 => "first",
                                1 => "second",
                                2 => "third",
                                _ => unreachable!(),
                            }
                        ),
                        arg.span,
                    ));
                }
            }
            if args.len() >= 4 {
                validate_min_window_literal(callee, &args[3], immutable_values, 1, diagnostics);
            }
            if args.len() == 5 && !arg_info[4].ty.is_scalar_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} multiplier must be a numeric scalar value"),
                    args[4].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesBool]),
                update_mask,
            }
        }
        BuiltinKind::RollingSingleInputTupleMa => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if args.len() >= 2 {
                validate_min_window_literal(callee, &args[1], immutable_values, 2, diagnostics);
            }
            if args.len() >= 3 {
                validate_min_window_literal(callee, &args[2], immutable_values, 1, diagnostics);
            }
            if args.len() >= 4 {
                validate_min_window_literal(callee, &args[3], immutable_values, 1, diagnostics);
            }
            if args.len() == 5 && !matches!(arg_info[4].ty, InferredType::Concrete(Type::MaType)) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires ma_type as the fifth argument"),
                    args[4].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesF64]),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::AdaptiveCycleTuple => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if args.len() >= 2 && !arg_info[1].ty.is_scalar_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} fast_limit must be a numeric scalar value"),
                    args[1].span,
                ));
            }
            if args.len() >= 3 && !arg_info[2].ty.is_scalar_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} slow_limit must be a numeric scalar value"),
                    args[2].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesF64]),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::VariablePeriodMovingAverage => {
            let price_info = arg_info[0];
            let period_info = arg_info[1];
            if !price_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if !period_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the second argument"),
                    args[1].span,
                ));
            }
            validate_min_window_literal(callee, &args[2], immutable_values, 2, diagnostics);
            validate_min_window_literal(callee, &args[3], immutable_values, 2, diagnostics);
            if !matches!(arg_info[4].ty, InferredType::Concrete(Type::MaType)) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires ma_type as the fifth argument"),
                    args[4].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: price_info.update_mask | period_info.update_mask,
            }
        }
        BuiltinKind::ParabolicSar | BuiltinKind::ParabolicSarExt => {
            let update_mask = arg_info
                .iter()
                .take(2)
                .fold(0, |mask, info| mask | info.update_mask);
            for (index, (arg, info)) in args.iter().zip(arg_info.iter()).take(2).enumerate() {
                if !info.ty.is_series_numeric() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "{callee} requires series<float> as the {} argument",
                            if index == 0 { "first" } else { "second" }
                        ),
                        arg.span,
                    ));
                }
            }
            for (arg, info) in args.iter().zip(arg_info.iter()).skip(2) {
                if !info.ty.is_scalar_numeric() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} optional parameters must be numeric scalar values"),
                        arg.span,
                    ));
                }
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask,
            }
        }
        BuiltinKind::Relation2 => {
            for (arg, info) in args.iter().zip(arg_info.iter()) {
                if !info.ty.is_numeric_like() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} requires numeric or series numeric arguments"),
                        arg.span,
                    ));
                }
            }
            bool_result(arg_info)
        }
        BuiltinKind::Relation3 => {
            for (arg, info) in args.iter().zip(arg_info.iter()) {
                if !info.ty.is_numeric_like() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} requires numeric or series numeric arguments"),
                        arg.span,
                    ));
                }
            }
            bool_result(arg_info)
        }
        BuiltinKind::Cross => {
            for (arg, info) in args.iter().zip(arg_info.iter()) {
                if !info.ty.is_numeric_like() {
                    diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} requires numeric or series numeric arguments"),
                        arg.span,
                    ));
                }
            }
            if !arg_info.iter().any(|info| info.ty.is_series_numeric()) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires at least one series<float> argument"),
                    span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesBool),
                update_mask: arg_info
                    .iter()
                    .fold(0, |mask, info| mask | info.update_mask),
            }
        }
        BuiltinKind::Change => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            validate_positive_window_literal(callee, &args[1], immutable_values, diagnostics);
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::Roc => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if args.len() == 2 {
                validate_positive_window_literal(callee, &args[1], immutable_values, diagnostics);
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: series_info.update_mask,
            }
        }
        BuiltinKind::BoolEdge => {
            let condition_info = arg_info[0];
            if !condition_info.ty.is_series_bool() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<bool> as the first argument"),
                    args[0].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesBool),
                update_mask: condition_info.update_mask,
            }
        }
        BuiltinKind::StateMachine => {
            let enter_info = arg_info[0];
            let exit_info = arg_info[1];
            if !enter_info.ty.is_series_bool() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<bool> as the first argument"),
                    args[0].span,
                ));
            }
            if !exit_info.ty.is_series_bool() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<bool> as the second argument"),
                    args[1].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesBool),
                update_mask: enter_info.update_mask | exit_info.update_mask,
            }
        }
        BuiltinKind::BarsSince => {
            let condition_info = arg_info[0];
            if !condition_info.ty.is_series_bool() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<bool> as the first argument"),
                    args[0].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: condition_info.update_mask,
            }
        }
        BuiltinKind::SinceExtrema => {
            let anchor_info = arg_info[0];
            let source_info = arg_info[1];
            if !anchor_info.ty.is_series_bool() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<bool> as the first argument"),
                    args[0].span,
                ));
            }
            if !source_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the second argument"),
                    args[1].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: anchor_info.update_mask | source_info.update_mask,
            }
        }
        BuiltinKind::SinceOffset => {
            let anchor_info = arg_info[0];
            let source_info = arg_info[1];
            if !anchor_info.ty.is_series_bool() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<bool> as the first argument"),
                    args[0].span,
                ));
            }
            if !source_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the second argument"),
                    args[1].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: anchor_info.update_mask | source_info.update_mask,
            }
        }
        BuiltinKind::SinceCount => {
            let anchor_info = arg_info[0];
            let condition_info = arg_info[1];
            if !anchor_info.ty.is_series_bool() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<bool> as the first argument"),
                    args[0].span,
                ));
            }
            if !condition_info.ty.is_series_bool() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<bool> as the second argument"),
                    args[1].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: anchor_info.update_mask | condition_info.update_mask,
            }
        }
        BuiltinKind::NullCheck => ExprInfo {
            ty: if arg_info[0].concrete().is_some_and(Type::is_series) {
                InferredType::Concrete(Type::SeriesBool)
            } else if matches!(arg_info[0].ty, InferredType::Na) {
                InferredType::Na
            } else {
                InferredType::Concrete(Type::Bool)
            },
            update_mask: arg_info[0].update_mask,
        },
        BuiltinKind::NullCoalesce => {
            let fallback = if matches!(builtin, BuiltinId::Nz) {
                if args.len() == 2 {
                    arg_info[1]
                } else {
                    default_nz_fallback(arg_info[0].ty, args[0].span, diagnostics)
                }
            } else {
                arg_info[1]
            };
            if !matches!(
                (arg_info[0].ty, fallback.ty),
                (InferredType::Na, _)
                    | (_, InferredType::Na)
                    | (
                        InferredType::Concrete(Type::F64),
                        InferredType::Concrete(Type::F64)
                    )
                    | (
                        InferredType::Concrete(Type::SeriesF64),
                        InferredType::Concrete(Type::SeriesF64)
                    )
                    | (
                        InferredType::Concrete(Type::F64),
                        InferredType::Concrete(Type::SeriesF64)
                    )
                    | (
                        InferredType::Concrete(Type::SeriesF64),
                        InferredType::Concrete(Type::F64)
                    )
                    | (
                        InferredType::Concrete(Type::Bool),
                        InferredType::Concrete(Type::Bool)
                    )
                    | (
                        InferredType::Concrete(Type::SeriesBool),
                        InferredType::Concrete(Type::SeriesBool)
                    )
                    | (
                        InferredType::Concrete(Type::Bool),
                        InferredType::Concrete(Type::SeriesBool)
                    )
                    | (
                        InferredType::Concrete(Type::SeriesBool),
                        InferredType::Concrete(Type::Bool)
                    )
            ) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires compatible numeric or bool arguments"),
                    span,
                ));
            }
            coalesce_result(arg_info[0], fallback)
        }
        BuiltinKind::Cumulative => {
            let input = arg_info[0];
            if !input.ty.is_numeric_like() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires numeric or series numeric input"),
                    args[0].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: input.update_mask,
            }
        }
        BuiltinKind::ValueWhen => {
            let condition_info = arg_info[0];
            let source_info = arg_info[1];
            if !condition_info.ty.is_series_bool() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<bool> as the first argument"),
                    args[0].span,
                ));
            }
            if !matches!(
                source_info.ty,
                InferredType::Concrete(Type::SeriesF64 | Type::SeriesBool)
            ) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!(
                        "{callee} requires series<float> or series<bool> as the second argument"
                    ),
                    args[1].span,
                ));
            }
            validate_non_negative_literal(
                callee,
                "occurrence",
                &args[2],
                immutable_values,
                diagnostics,
            );
            ExprInfo {
                ty: match source_info.ty {
                    InferredType::Concrete(Type::SeriesBool) => {
                        InferredType::Concrete(Type::SeriesBool)
                    }
                    _ => InferredType::Concrete(Type::SeriesF64),
                },
                update_mask: condition_info.update_mask | source_info.update_mask,
            }
        }
        BuiltinKind::SinceValueWhen => {
            let anchor_info = arg_info[0];
            let condition_info = arg_info[1];
            let source_info = arg_info[2];
            if !anchor_info.ty.is_series_bool() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<bool> as the first argument"),
                    args[0].span,
                ));
            }
            if !condition_info.ty.is_series_bool() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<bool> as the second argument"),
                    args[1].span,
                ));
            }
            if !matches!(
                source_info.ty,
                InferredType::Concrete(Type::SeriesF64 | Type::SeriesBool)
            ) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!(
                        "{callee} requires series<float> or series<bool> as the third argument"
                    ),
                    args[2].span,
                ));
            }
            validate_non_negative_literal(
                callee,
                "occurrence",
                &args[3],
                immutable_values,
                diagnostics,
            );
            ExprInfo {
                ty: match source_info.ty {
                    InferredType::Concrete(Type::SeriesBool) => {
                        InferredType::Concrete(Type::SeriesBool)
                    }
                    _ => InferredType::Concrete(Type::SeriesF64),
                },
                update_mask: anchor_info.update_mask
                    | condition_info.update_mask
                    | source_info.update_mask,
            }
        }
        BuiltinKind::VolumeIndicator => {
            let price_info = arg_info[0];
            let volume_info = arg_info[1];
            if !price_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if !volume_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the second argument"),
                    args[1].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: price_info.update_mask | volume_info.update_mask,
            }
        }
        BuiltinKind::AnchoredPriceVolume => {
            let anchor_info = arg_info[0];
            let price_info = arg_info[1];
            let volume_info = arg_info[2];
            if !matches!(anchor_info.ty, InferredType::Concrete(Type::SeriesBool)) {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<bool> as the first argument"),
                    args[0].span,
                ));
            }
            if !price_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the second argument"),
                    args[1].span,
                ));
            }
            if !volume_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the third argument"),
                    args[2].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: anchor_info.update_mask
                    | price_info.update_mask
                    | volume_info.update_mask,
            }
        }
        BuiltinKind::VolatilityIndicator => {
            let high_info = arg_info[0];
            let low_info = arg_info[1];
            let close_info = arg_info[2];
            if !high_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            if !low_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the second argument"),
                    args[1].span,
                ));
            }
            if !close_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the third argument"),
                    args[2].span,
                ));
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: high_info.update_mask | low_info.update_mask | close_info.update_mask,
            }
        }
        BuiltinKind::Plot | BuiltinKind::MarketSeries => unreachable!(),
    }
}

fn fallback_expr_info_for_builtin(builtin: BuiltinId, arg_info: &[ExprInfo]) -> ExprInfo {
    match builtin.kind() {
        BuiltinKind::Plot => ExprInfo::scalar(Type::Void),
        BuiltinKind::Relation2 | BuiltinKind::Relation3 => ExprInfo::scalar(Type::Bool),
        BuiltinKind::Cross => ExprInfo::series(0),
        BuiltinKind::Rising
        | BuiltinKind::Falling
        | BuiltinKind::BoolEdge
        | BuiltinKind::StateMachine => ExprInfo {
            ty: InferredType::Concrete(Type::SeriesBool),
            update_mask: 0,
        },
        BuiltinKind::NullCheck => bool_result(arg_info),
        BuiltinKind::NullCoalesce => {
            let left = arg_info
                .first()
                .copied()
                .unwrap_or_else(|| ExprInfo::scalar(Type::F64));
            let right = arg_info.get(1).copied().unwrap_or(left);
            coalesce_result(left, right)
        }
        BuiltinKind::ValueWhen
        | BuiltinKind::SinceExtrema
        | BuiltinKind::SinceOffset
        | BuiltinKind::SinceCount => ExprInfo::series(0),
        BuiltinKind::SinceValueWhen => ExprInfo::series(0),
        BuiltinKind::IndicatorTuple => ExprInfo {
            ty: InferredType::Tuple3([Type::SeriesF64, Type::SeriesF64, Type::SeriesF64]),
            update_mask: 0,
        },
        BuiltinKind::IndicatorTupleSignal
        | BuiltinKind::IndicatorTupleMa
        | BuiltinKind::Bands
        | BuiltinKind::RollingHighLowCloseBands
        | BuiltinKind::RollingHighLowBands => ExprInfo {
            ty: InferredType::Tuple3([Type::SeriesF64, Type::SeriesF64, Type::SeriesF64]),
            update_mask: 0,
        },
        BuiltinKind::RollingSingleInputTuple => ExprInfo {
            ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesF64]),
            update_mask: 0,
        },
        BuiltinKind::RollingHighLowTuple => ExprInfo {
            ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesF64]),
            update_mask: 0,
        },
        BuiltinKind::RollingHighLowCloseTuple
        | BuiltinKind::RollingSingleInputTupleMa
        | BuiltinKind::AdaptiveCycleTuple => ExprInfo {
            ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesF64]),
            update_mask: 0,
        },
        BuiltinKind::RollingHighLowCloseTrendTuple => ExprInfo {
            ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesBool]),
            update_mask: 0,
        },
        BuiltinKind::UnaryMathTransform
        | BuiltinKind::NumericBinary
        | BuiltinKind::PriceTransform => numeric_result(arg_info),
        BuiltinKind::CurrentOhlc => ExprInfo::series(0),
        BuiltinKind::BarsSince
        | BuiltinKind::Cumulative
        | BuiltinKind::Indicator
        | BuiltinKind::MovingAverage
        | BuiltinKind::MaOscillator
        | BuiltinKind::Change
        | BuiltinKind::Roc
        | BuiltinKind::Highest
        | BuiltinKind::Lowest
        | BuiltinKind::HighestBars
        | BuiltinKind::LowestBars
        | BuiltinKind::RollingSingleInput
        | BuiltinKind::RollingSingleInputPercentile
        | BuiltinKind::RollingSingleInputFactor
        | BuiltinKind::RollingDoubleInput
        | BuiltinKind::RollingHighLow
        | BuiltinKind::RollingHighLowClose
        | BuiltinKind::RollingQuadInputWindow
        | BuiltinKind::RollingQuadInputDoubleWindow
        | BuiltinKind::VariablePeriodMovingAverage
        | BuiltinKind::ParabolicSar
        | BuiltinKind::ParabolicSarExt
        | BuiltinKind::VolumeIndicator
        | BuiltinKind::AnchoredPriceVolume
        | BuiltinKind::VolatilityIndicator => ExprInfo::series(0),
        BuiltinKind::MarketSeries => ExprInfo::series(0),
    }
}

fn fallback_expr_info_for_talib(
    metadata: &TalibFunctionMetadata,
    arg_info: &[ExprInfo],
) -> ExprInfo {
    let update_mask = arg_info
        .iter()
        .fold(0, |mask, info| mask | info.update_mask);
    match metadata.output_count {
        2 => ExprInfo {
            ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesF64]),
            update_mask,
        },
        3 => ExprInfo {
            ty: InferredType::Tuple3([Type::SeriesF64, Type::SeriesF64, Type::SeriesF64]),
            update_mask,
        },
        _ => ExprInfo {
            ty: InferredType::Concrete(Type::SeriesF64),
            update_mask,
        },
    }
}

fn bool_result(arg_info: &[ExprInfo]) -> ExprInfo {
    let update_mask = arg_info
        .iter()
        .fold(0, |mask, info| mask | info.update_mask);
    let has_series = arg_info.iter().any(|info| {
        matches!(
            info.ty,
            InferredType::Concrete(Type::SeriesF64 | Type::SeriesBool)
        )
    });
    ExprInfo {
        ty: if has_series {
            InferredType::Concrete(Type::SeriesBool)
        } else if arg_info
            .iter()
            .all(|info| matches!(info.ty, InferredType::Na))
        {
            InferredType::Na
        } else {
            InferredType::Concrete(Type::Bool)
        },
        update_mask,
    }
}

fn coalesce_result(left: ExprInfo, right: ExprInfo) -> ExprInfo {
    ExprInfo {
        ty: match (left.ty, right.ty) {
            (InferredType::Concrete(Type::SeriesBool), _)
            | (_, InferredType::Concrete(Type::SeriesBool)) => {
                InferredType::Concrete(Type::SeriesBool)
            }
            (InferredType::Concrete(Type::Bool), InferredType::Concrete(Type::Bool)) => {
                InferredType::Concrete(Type::Bool)
            }
            (InferredType::Concrete(Type::SeriesF64), _)
            | (_, InferredType::Concrete(Type::SeriesF64)) => {
                InferredType::Concrete(Type::SeriesF64)
            }
            (InferredType::Concrete(Type::F64), InferredType::Concrete(Type::F64)) => {
                InferredType::Concrete(Type::F64)
            }
            (InferredType::Na, other) | (other, InferredType::Na) => other,
            _ => InferredType::Concrete(Type::F64),
        },
        update_mask: left.update_mask | right.update_mask,
    }
}

fn numeric_result(arg_info: &[ExprInfo]) -> ExprInfo {
    let update_mask = arg_info
        .iter()
        .fold(0, |mask, info| mask | info.update_mask);
    let has_series = arg_info
        .iter()
        .any(|info| matches!(info.ty, InferredType::Concrete(Type::SeriesF64)));
    ExprInfo {
        ty: if has_series {
            InferredType::Concrete(Type::SeriesF64)
        } else if arg_info
            .iter()
            .all(|info| matches!(info.ty, InferredType::Na))
        {
            InferredType::Na
        } else {
            InferredType::Concrete(Type::F64)
        },
        update_mask,
    }
}

fn default_nz_fallback(
    ty: InferredType,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> ExprInfo {
    match ty {
        InferredType::Concrete(Type::Bool) => ExprInfo::scalar(Type::Bool),
        InferredType::Concrete(Type::SeriesBool) => ExprInfo {
            ty: InferredType::Concrete(Type::SeriesBool),
            update_mask: 0,
        },
        InferredType::Concrete(Type::F64 | Type::SeriesF64) | InferredType::Na => ExprInfo {
            ty: match ty {
                InferredType::Concrete(Type::SeriesF64) => InferredType::Concrete(Type::SeriesF64),
                InferredType::Na => InferredType::Na,
                _ => InferredType::Concrete(Type::F64),
            },
            update_mask: 0,
        },
        _ => {
            diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "nz requires numeric or bool input",
                span,
            ));
            ExprInfo::scalar(Type::F64)
        }
    }
}

fn builtin_allowed_in_const(builtin: BuiltinId) -> bool {
    matches!(
        builtin.kind(),
        BuiltinKind::UnaryMathTransform
            | BuiltinKind::NumericBinary
            | BuiltinKind::Relation2
            | BuiltinKind::Relation3
            | BuiltinKind::NullCheck
            | BuiltinKind::NullCoalesce
    )
}

fn compiled_signal_role(role: AstSignalRole) -> CompiledSignalRole {
    match role {
        AstSignalRole::LongEntry => CompiledSignalRole::LongEntry,
        AstSignalRole::LongEntry2 => CompiledSignalRole::LongEntry2,
        AstSignalRole::LongEntry3 => CompiledSignalRole::LongEntry3,
        AstSignalRole::LongExit => CompiledSignalRole::LongExit,
        AstSignalRole::ShortEntry => CompiledSignalRole::ShortEntry,
        AstSignalRole::ShortEntry2 => CompiledSignalRole::ShortEntry2,
        AstSignalRole::ShortEntry3 => CompiledSignalRole::ShortEntry3,
        AstSignalRole::ShortExit => CompiledSignalRole::ShortExit,
        AstSignalRole::ProtectLong => CompiledSignalRole::ProtectLong,
        AstSignalRole::ProtectAfterTarget1Long => CompiledSignalRole::ProtectAfterTarget1Long,
        AstSignalRole::ProtectAfterTarget2Long => CompiledSignalRole::ProtectAfterTarget2Long,
        AstSignalRole::ProtectAfterTarget3Long => CompiledSignalRole::ProtectAfterTarget3Long,
        AstSignalRole::ProtectShort => CompiledSignalRole::ProtectShort,
        AstSignalRole::ProtectAfterTarget1Short => CompiledSignalRole::ProtectAfterTarget1Short,
        AstSignalRole::ProtectAfterTarget2Short => CompiledSignalRole::ProtectAfterTarget2Short,
        AstSignalRole::ProtectAfterTarget3Short => CompiledSignalRole::ProtectAfterTarget3Short,
        AstSignalRole::TargetLong => CompiledSignalRole::TargetLong,
        AstSignalRole::TargetLong2 => CompiledSignalRole::TargetLong2,
        AstSignalRole::TargetLong3 => CompiledSignalRole::TargetLong3,
        AstSignalRole::TargetShort => CompiledSignalRole::TargetShort,
        AstSignalRole::TargetShort2 => CompiledSignalRole::TargetShort2,
        AstSignalRole::TargetShort3 => CompiledSignalRole::TargetShort3,
    }
}

fn compiled_risk_control_kind(kind: AstRiskControlKind) -> CompiledRiskControlKind {
    match kind {
        AstRiskControlKind::Cooldown => CompiledRiskControlKind::Cooldown,
        AstRiskControlKind::MaxBarsInTrade => CompiledRiskControlKind::MaxBarsInTrade,
    }
}

fn compiled_portfolio_control_kind(kind: AstPortfolioControlKind) -> CompiledPortfolioControlKind {
    match kind {
        AstPortfolioControlKind::MaxPositions => CompiledPortfolioControlKind::MaxPositions,
        AstPortfolioControlKind::MaxLongPositions => CompiledPortfolioControlKind::MaxLongPositions,
        AstPortfolioControlKind::MaxShortPositions => {
            CompiledPortfolioControlKind::MaxShortPositions
        }
        AstPortfolioControlKind::MaxGrossExposurePct => {
            CompiledPortfolioControlKind::MaxGrossExposurePct
        }
        AstPortfolioControlKind::MaxNetExposurePct => {
            CompiledPortfolioControlKind::MaxNetExposurePct
        }
    }
}

fn risk_control_name(kind: CompiledRiskControlKind) -> &'static str {
    match kind {
        CompiledRiskControlKind::Cooldown => "`cooldown`",
        CompiledRiskControlKind::MaxBarsInTrade => "`max_bars_in_trade`",
    }
}

fn portfolio_control_name(kind: CompiledPortfolioControlKind) -> &'static str {
    match kind {
        CompiledPortfolioControlKind::MaxPositions => "`max_positions`",
        CompiledPortfolioControlKind::MaxLongPositions => "`max_long_positions`",
        CompiledPortfolioControlKind::MaxShortPositions => "`max_short_positions`",
        CompiledPortfolioControlKind::MaxGrossExposurePct => "`max_gross_exposure_pct`",
        CompiledPortfolioControlKind::MaxNetExposurePct => "`max_net_exposure_pct`",
    }
}

fn side_name(side: PositionSide) -> &'static str {
    match side {
        PositionSide::Long => "long",
        PositionSide::Short => "short",
    }
}

fn eval_immutable_expr(expr: &Expr, values: &HashMap<String, Value>) -> Option<Value> {
    match &expr.kind {
        ExprKind::Number(value) => Some(Value::F64(*value)),
        ExprKind::Bool(value) => Some(Value::Bool(*value)),
        ExprKind::Na => Some(Value::NA),
        ExprKind::Ident(name) => values.get(name).cloned(),
        ExprKind::EnumVariant {
            namespace, variant, ..
        } => resolve_enum_variant(namespace, variant),
        ExprKind::Unary { op, expr } => {
            let value = eval_immutable_expr(expr, values)?;
            match (op, value) {
                (UnaryOp::Neg, Value::F64(value)) => Some(Value::F64(-value)),
                (UnaryOp::Neg, Value::NA) => Some(Value::NA),
                (UnaryOp::Not, Value::Bool(value)) => Some(Value::Bool(!value)),
                (UnaryOp::Not, Value::NA) => Some(Value::NA),
                _ => None,
            }
        }
        ExprKind::Binary { op, left, right } => {
            let left = eval_immutable_expr(left, values)?;
            let right = eval_immutable_expr(right, values)?;
            eval_immutable_binary(*op, left, right)
        }
        ExprKind::Conditional {
            condition,
            when_true,
            when_false,
        } => match eval_immutable_expr(condition, values)? {
            Value::Bool(true) => eval_immutable_expr(when_true, values),
            Value::Bool(false) | Value::NA => eval_immutable_expr(when_false, values),
            _ => None,
        },
        ExprKind::Call { callee, args, .. } => {
            let builtin = BuiltinId::from_name(callee)?;
            if !builtin_allowed_in_const(builtin) {
                return None;
            }
            let args: Vec<Value> = args
                .iter()
                .map(|arg| eval_immutable_expr(arg, values))
                .collect::<Option<_>>()?;
            eval_immutable_builtin(builtin, &args)
        }
        ExprKind::String(_)
        | ExprKind::SourceSeries { .. }
        | ExprKind::PositionField { .. }
        | ExprKind::PositionEventField { .. }
        | ExprKind::LastExitField { .. }
        | ExprKind::Index { .. } => None,
    }
}

fn eval_immutable_binary(op: BinaryOp, left: Value, right: Value) -> Option<Value> {
    match op {
        BinaryOp::Add => match (left, right) {
            (Value::F64(left), Value::F64(right)) => Some(Value::F64(left + right)),
            (Value::NA, _) | (_, Value::NA) => Some(Value::NA),
            _ => None,
        },
        BinaryOp::Sub => match (left, right) {
            (Value::F64(left), Value::F64(right)) => Some(Value::F64(left - right)),
            (Value::NA, _) | (_, Value::NA) => Some(Value::NA),
            _ => None,
        },
        BinaryOp::Mul => match (left, right) {
            (Value::F64(left), Value::F64(right)) => Some(Value::F64(left * right)),
            (Value::NA, _) | (_, Value::NA) => Some(Value::NA),
            _ => None,
        },
        BinaryOp::Div => match (left, right) {
            (Value::F64(left), Value::F64(right)) => Some(Value::F64(left / right)),
            (Value::NA, _) | (_, Value::NA) => Some(Value::NA),
            _ => None,
        },
        BinaryOp::Eq => Some(Value::Bool(eq_values_const(&left, &right)?)),
        BinaryOp::Ne => Some(Value::Bool(!eq_values_const(&left, &right)?)),
        BinaryOp::Lt => compare_f64_const(left, right, |left, right| left < right),
        BinaryOp::Le => compare_f64_const(left, right, |left, right| left <= right),
        BinaryOp::Gt => compare_f64_const(left, right, |left, right| left > right),
        BinaryOp::Ge => compare_f64_const(left, right, |left, right| left >= right),
        BinaryOp::And => match (left, right) {
            (Value::Bool(left), Value::Bool(right)) => Some(Value::Bool(left && right)),
            (Value::Bool(false), Value::NA) | (Value::NA, Value::Bool(false)) => {
                Some(Value::Bool(false))
            }
            (Value::Bool(true), Value::NA)
            | (Value::NA, Value::Bool(true))
            | (Value::NA, Value::NA) => Some(Value::NA),
            _ => None,
        },
        BinaryOp::Or => match (left, right) {
            (Value::Bool(left), Value::Bool(right)) => Some(Value::Bool(left || right)),
            (Value::Bool(true), Value::NA) | (Value::NA, Value::Bool(true)) => {
                Some(Value::Bool(true))
            }
            (Value::Bool(false), Value::NA)
            | (Value::NA, Value::Bool(false))
            | (Value::NA, Value::NA) => Some(Value::NA),
            _ => None,
        },
    }
}

fn eval_immutable_builtin(builtin: BuiltinId, args: &[Value]) -> Option<Value> {
    match builtin {
        BuiltinId::Add => eval_immutable_binary(BinaryOp::Add, args[0].clone(), args[1].clone()),
        BuiltinId::Sub => eval_immutable_binary(BinaryOp::Sub, args[0].clone(), args[1].clone()),
        BuiltinId::Mult => eval_immutable_binary(BinaryOp::Mul, args[0].clone(), args[1].clone()),
        BuiltinId::Div => eval_immutable_binary(BinaryOp::Div, args[0].clone(), args[1].clone()),
        BuiltinId::Above => {
            compare_f64_const(args[0].clone(), args[1].clone(), |left, right| left > right)
        }
        BuiltinId::Below => {
            compare_f64_const(args[0].clone(), args[1].clone(), |left, right| left < right)
        }
        BuiltinId::Between => {
            let low = expect_const_f64(&args[1])?;
            let value = expect_const_f64(&args[0])?;
            let high = expect_const_f64(&args[2])?;
            Some(Value::Bool(low < value && value < high))
        }
        BuiltinId::Outside => {
            let value = expect_const_f64(&args[0])?;
            let low = expect_const_f64(&args[1])?;
            let high = expect_const_f64(&args[2])?;
            Some(Value::Bool(value < low || value > high))
        }
        BuiltinId::Nz | BuiltinId::Coalesce => Some(match &args[0] {
            Value::NA => args[1].clone(),
            other => other.clone(),
        }),
        BuiltinId::NaFunc => Some(Value::Bool(matches!(args[0], Value::NA))),
        _ => None,
    }
}

fn expect_const_f64(value: &Value) -> Option<f64> {
    match value {
        Value::F64(value) => Some(*value),
        _ => None,
    }
}

fn compare_f64_const(
    left: Value,
    right: Value,
    predicate: impl FnOnce(f64, f64) -> bool,
) -> Option<Value> {
    match (left, right) {
        (Value::F64(left), Value::F64(right)) => Some(Value::Bool(predicate(left, right))),
        (Value::NA, _) | (_, Value::NA) => Some(Value::NA),
        _ => None,
    }
}

fn eq_values_const(left: &Value, right: &Value) -> Option<bool> {
    match (left, right) {
        (Value::F64(left), Value::F64(right)) => Some(left == right),
        (Value::Bool(left), Value::Bool(right)) => Some(left == right),
        (Value::MaType(left), Value::MaType(right)) => Some(left == right),
        (Value::TimeInForce(left), Value::TimeInForce(right)) => Some(left == right),
        (Value::TriggerReference(left), Value::TriggerReference(right)) => Some(left == right),
        (Value::PositionSide(left), Value::PositionSide(right)) => Some(left == right),
        (Value::ExitKind(left), Value::ExitKind(right)) => Some(left == right),
        (Value::NA, Value::NA) => Some(true),
        _ => None,
    }
}

fn validate_positive_window_literal(
    callee: &str,
    expr: &Expr,
    immutable_values: &HashMap<String, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match literal_window(expr, immutable_values) {
        Some(window) if window > 0 => {}
        Some(_) => diagnostics.push(Diagnostic::new(
            DiagnosticKind::Type,
            format!("{callee} length must be greater than zero"),
            expr.span,
        )),
        None => diagnostics.push(Diagnostic::new(
            DiagnosticKind::Type,
            format!("{callee} length must be a non-negative integer literal"),
            expr.span,
        )),
    }
}

fn validate_min_window_literal(
    callee: &str,
    expr: &Expr,
    immutable_values: &HashMap<String, Value>,
    minimum: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match literal_window(expr, immutable_values) {
        Some(window) if window >= minimum => {}
        Some(_) => diagnostics.push(Diagnostic::new(
            DiagnosticKind::Type,
            format!("{callee} length must be greater than or equal to {minimum}"),
            expr.span,
        )),
        None => diagnostics.push(Diagnostic::new(
            DiagnosticKind::Type,
            format!("{callee} length must be a non-negative integer literal"),
            expr.span,
        )),
    }
}

fn validate_non_negative_literal(
    callee: &str,
    noun: &str,
    expr: &Expr,
    immutable_values: &HashMap<String, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if literal_window(expr, immutable_values).is_none() {
        diagnostics.push(Diagnostic::new(
            DiagnosticKind::Type,
            format!("{callee} {noun} must be a non-negative integer literal"),
            expr.span,
        ));
    }
}

fn expected_arity_message(callee: &str, arity: BuiltinArity) -> String {
    match arity {
        BuiltinArity::NonCallable => format!("{callee} is not callable"),
        BuiltinArity::Exact(exact) => {
            let quantity = match exact {
                1 => "one".to_string(),
                2 => "two".to_string(),
                3 => "three".to_string(),
                4 => "four".to_string(),
                _ => exact.to_string(),
            };
            format!(
                "{callee} expects exactly {quantity} argument{}",
                if exact == 1 { "" } else { "s" }
            )
        }
        BuiltinArity::Range { min, max } if max == min + 1 => {
            let left = match min {
                1 => "one".to_string(),
                2 => "two".to_string(),
                3 => "three".to_string(),
                _ => min.to_string(),
            };
            let right = match max {
                2 => "two".to_string(),
                3 => "three".to_string(),
                4 => "four".to_string(),
                _ => max.to_string(),
            };
            format!("{callee} expects either {left} or {right} arguments")
        }
        BuiltinArity::Range { min, max } => {
            format!("{callee} expects between {min} and {max} arguments")
        }
    }
}

fn literal_window(expr: &Expr, immutable_values: &HashMap<String, Value>) -> Option<usize> {
    match &expr.kind {
        ExprKind::Number(value) if *value >= 0.0 && value.fract() == 0.0 => Some(*value as usize),
        ExprKind::Ident(name) => match immutable_values.get(name) {
            Some(Value::F64(value)) if *value >= 0.0 && value.fract() == 0.0 => {
                Some(*value as usize)
            }
            _ => None,
        },
        _ => None,
    }
}

fn literal_ma_type(expr: &Expr, immutable_values: &HashMap<String, Value>) -> Option<MaType> {
    match &expr.kind {
        ExprKind::Ident(name) => match immutable_values.get(name) {
            Some(Value::MaType(ma_type)) => Some(*ma_type),
            _ => None,
        },
        ExprKind::EnumVariant {
            namespace, variant, ..
        } => match resolve_enum_variant(namespace, variant) {
            Some(Value::MaType(ma_type)) => Some(ma_type),
            _ => None,
        },
        _ => None,
    }
}

fn literal_bool(expr: &Expr, immutable_values: &HashMap<String, Value>) -> Option<bool> {
    match &expr.kind {
        ExprKind::Bool(value) => Some(*value),
        ExprKind::Ident(name) => match immutable_values.get(name) {
            Some(Value::Bool(value)) => Some(*value),
            _ => None,
        },
        _ => None,
    }
}

fn literal_time_in_force(
    expr: &Expr,
    immutable_values: &HashMap<String, Value>,
) -> Option<TimeInForce> {
    match &expr.kind {
        ExprKind::Ident(name) => match immutable_values.get(name) {
            Some(Value::TimeInForce(value)) => Some(*value),
            _ => None,
        },
        ExprKind::EnumVariant {
            namespace, variant, ..
        } => match resolve_enum_variant(namespace, variant) {
            Some(Value::TimeInForce(value)) => Some(value),
            _ => None,
        },
        _ => None,
    }
}

fn literal_trigger_reference(
    expr: &Expr,
    immutable_values: &HashMap<String, Value>,
) -> Option<TriggerReference> {
    match &expr.kind {
        ExprKind::Ident(name) => match immutable_values.get(name) {
            Some(Value::TriggerReference(value)) => Some(*value),
            _ => None,
        },
        ExprKind::EnumVariant {
            namespace, variant, ..
        } => match resolve_enum_variant(namespace, variant) {
            Some(Value::TriggerReference(value)) => Some(value),
            _ => None,
        },
        _ => None,
    }
}

fn ma_input_history_hint(
    ma_type_expr: Option<&Expr>,
    window: usize,
    immutable_values: &HashMap<String, Value>,
) -> usize {
    match ma_type_expr.and_then(|expr| literal_ma_type(expr, immutable_values)) {
        Some(ma_type) => crate::indicators::MovingAverageState::input_history(window, ma_type),
        None => window + 1,
    }
}

fn resolve_enum_variant(namespace: &str, variant: &str) -> Option<Value> {
    match namespace {
        "ma_type" => MaType::from_variant(variant).map(Value::MaType),
        "tif" => TimeInForce::from_variant(variant).map(Value::TimeInForce),
        "trigger_ref" => TriggerReference::from_variant(variant).map(Value::TriggerReference),
        "position_side" => {
            crate::position::PositionSide::from_variant(variant).map(Value::PositionSide)
        }
        "exit_kind" => ExitKind::from_variant(variant).map(Value::ExitKind),
        _ => None,
    }
}

fn scalar_type_for_value(value: &Value) -> Type {
    match value {
        Value::F64(_) => Type::F64,
        Value::Bool(_) => Type::Bool,
        Value::MaType(_) => Type::MaType,
        Value::TimeInForce(_) => Type::TimeInForce,
        Value::TriggerReference(_) => Type::TriggerReference,
        Value::PositionSide(_) => Type::PositionSide,
        Value::ExitKind(_) => Type::ExitKind,
        Value::NA => Type::F64,
        Value::Void | Value::SeriesRef(_) | Value::Tuple2(_) | Value::Tuple3(_) => Type::F64,
    }
}

fn position_field_type(field: PositionField) -> Type {
    match field {
        PositionField::IsLong | PositionField::IsShort => Type::Bool,
        PositionField::Side => Type::PositionSide,
        PositionField::EntryPrice
        | PositionField::EntryTime
        | PositionField::EntryBarIndex
        | PositionField::BarsHeld
        | PositionField::MarketPrice
        | PositionField::UnrealizedPnl
        | PositionField::UnrealizedReturn
        | PositionField::Mae
        | PositionField::Mfe => Type::F64,
    }
}

fn last_exit_field_type(field: LastExitField) -> Type {
    match field {
        LastExitField::Kind => Type::ExitKind,
        LastExitField::Stage => Type::F64,
        LastExitField::Side => Type::PositionSide,
        LastExitField::Price
        | LastExitField::Time
        | LastExitField::BarIndex
        | LastExitField::RealizedPnl
        | LastExitField::RealizedReturn
        | LastExitField::BarsHeld => Type::F64,
    }
}

fn param_binding(arg_shape: FunctionArgShape) -> FunctionParamBinding {
    match arg_shape.ty {
        InferredType::Concrete(ty) => FunctionParamBinding {
            ty,
            kind: if ty.is_series() {
                SlotKind::Series
            } else {
                SlotKind::Scalar
            },
            update_mask: arg_shape.update_mask,
        },
        InferredType::Tuple2(_) | InferredType::Tuple3(_) => FunctionParamBinding {
            ty: Type::F64,
            kind: SlotKind::Scalar,
            update_mask: arg_shape.update_mask,
        },
        InferredType::Na => FunctionParamBinding {
            ty: Type::SeriesF64,
            kind: SlotKind::Series,
            update_mask: arg_shape.update_mask,
        },
    }
}

fn output_series_type(
    ty: InferredType,
    kind: OutputKind,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> Type {
    match kind {
        OutputKind::ExportSeries => match ty {
            InferredType::Concrete(Type::F64 | Type::SeriesF64) | InferredType::Na => {
                Type::SeriesF64
            }
            InferredType::Concrete(Type::Bool | Type::SeriesBool) => Type::SeriesBool,
            InferredType::Concrete(Type::Void)
            | InferredType::Concrete(Type::MaType)
            | InferredType::Concrete(Type::TimeInForce)
            | InferredType::Concrete(Type::TriggerReference)
            | InferredType::Concrete(Type::PositionSide)
            | InferredType::Concrete(Type::ExitKind)
            | InferredType::Tuple2(_)
            | InferredType::Tuple3(_) => {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    "export requires a numeric, bool, series numeric, series bool, or na value",
                    span,
                ));
                Type::SeriesF64
            }
        },
        OutputKind::Trigger => Type::SeriesBool,
    }
}

fn is_predefined_series_name(name: &str) -> bool {
    matches!(name, "open" | "high" | "low" | "close" | "volume" | "time")
}

struct Compiler<'a> {
    source: &'a str,
    ast: &'a Ast,
    analysis: Analysis,
    program: Program,
    diagnostics: Vec<Diagnostic>,
    builtin_call_count: u16,
    scopes: Vec<HashMap<String, CompilerSymbol>>,
    functions_by_id: HashMap<NodeId, &'a FunctionDecl>,
}

impl<'a> Compiler<'a> {
    fn new(source: &'a str, ast: &'a Ast) -> Self {
        let functions_by_id = ast
            .functions
            .iter()
            .map(|function| (function.id, function))
            .collect();
        Self {
            source,
            ast,
            analysis: Analysis::default(),
            program: Program::default(),
            diagnostics: Vec::new(),
            builtin_call_count: 0,
            scopes: Vec::new(),
            functions_by_id,
        }
    }

    fn compile(mut self) -> Result<CompiledProgram, CompileError> {
        self.analysis = Analyzer::new(self.ast).analyze(self.ast)?;
        self.program.locals = self.analysis.locals.clone();
        self.program.inputs = collect_input_decls(self.ast, &self.analysis);
        self.program.outputs = self.analysis.outputs.clone();
        self.program.order_fields = self.analysis.order_fields.clone();
        self.program.position_fields = self.analysis.position_fields.clone();
        self.program.position_event_fields = self.analysis.position_event_fields.clone();
        self.program.last_exit_fields = self.analysis.last_exit_fields.clone();
        self.program.orders = self.analysis.orders.clone();
        self.program.risk_controls = self.analysis.risk_controls.clone();
        self.program.portfolio_controls = self.analysis.portfolio_controls.clone();
        self.program.portfolio_groups = self.analysis.portfolio_groups.clone();
        self.program.base_interval = self.analysis.base_interval;
        self.program.declared_sources = self.analysis.declared_sources.clone();
        self.program.declared_executions = self.analysis.declared_executions.clone();
        self.program.source_intervals = self.analysis.source_intervals.clone();
        self.rebuild_scopes();
        let expr_info = self.analysis.expr_info.clone();
        let user_calls = self.analysis.user_function_calls.clone();
        for stmt in &self.ast.statements {
            self.emit_stmt(stmt, &expr_info, &user_calls);
        }
        self.program
            .instructions
            .push(Instruction::new(OpCode::Return));
        self.program.history_capacity = self
            .program
            .locals
            .iter()
            .map(|local| local.history_capacity)
            .max()
            .unwrap_or(2)
            .max(2);
        if self.diagnostics.is_empty() {
            Ok(CompiledProgram {
                program: self.program,
                source: self.source.to_string(),
            })
        } else {
            Err(CompileError::new(self.diagnostics))
        }
    }

    fn emit_stmt(
        &mut self,
        stmt: &Stmt,
        expr_info: &HashMap<NodeId, ExprInfo>,
        user_calls: &HashMap<NodeId, FunctionSpecializationKey>,
    ) {
        match &stmt.kind {
            StmtKind::Let { name, expr, .. } => {
                self.emit_expr(expr, expr_info, user_calls);
                let slot = self.analysis.resolved_let_slots[&stmt.id];
                self.emit(
                    Instruction::new(OpCode::StoreLocal)
                        .with_a(slot)
                        .with_span(stmt.span),
                );
                let local = &self.program.locals[slot as usize];
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(name.clone(), CompilerSymbol { slot, ty: local.ty });
            }
            StmtKind::LetTuple { names, expr } => {
                self.emit_expr(expr, expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::UnpackTuple)
                        .with_a(names.len() as u16)
                        .with_span(stmt.span),
                );
                let slots = self.analysis.resolved_let_tuple_slots[&stmt.id].clone();
                for (binding, slot) in names.iter().zip(slots.iter()).rev() {
                    self.emit(
                        Instruction::new(OpCode::StoreLocal)
                            .with_a(*slot)
                            .with_span(stmt.span),
                    );
                    let local = &self.program.locals[*slot as usize];
                    self.scopes.last_mut().unwrap().insert(
                        binding.name.clone(),
                        CompilerSymbol {
                            slot: *slot,
                            ty: local.ty,
                        },
                    );
                }
            }
            StmtKind::Const { name, expr, .. } | StmtKind::Input { name, expr, .. } => {
                self.emit_expr(expr, expr_info, user_calls);
                let slot = self.analysis.resolved_let_slots[&stmt.id];
                self.emit(
                    Instruction::new(OpCode::StoreLocal)
                        .with_a(slot)
                        .with_span(stmt.span),
                );
                let local = &self.program.locals[slot as usize];
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(name.clone(), CompilerSymbol { slot, ty: local.ty });
            }
            StmtKind::Export { name, expr, .. }
            | StmtKind::Regime { name, expr, .. }
            | StmtKind::Trigger { name, expr, .. } => {
                self.emit_expr(expr, expr_info, user_calls);
                let slot = self.analysis.resolved_output_slots[&stmt.id];
                self.emit(
                    Instruction::new(OpCode::StoreLocal)
                        .with_a(slot)
                        .with_span(stmt.span),
                );
                let local = &self.program.locals[slot as usize];
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(name.clone(), CompilerSymbol { slot, ty: local.ty });
            }
            StmtKind::Signal { expr, .. } => {
                self.emit_expr(expr, expr_info, user_calls);
                let slot = self.analysis.resolved_output_slots[&stmt.id];
                self.emit(
                    Instruction::new(OpCode::StoreLocal)
                        .with_a(slot)
                        .with_span(stmt.span),
                );
            }
            StmtKind::Order { spec, .. } => {
                let resolved = self.analysis.resolved_order_field_slots[&stmt.id];
                match &spec.kind {
                    OrderSpecKind::Market => {}
                    OrderSpecKind::Limit { price, .. } => {
                        if let Some(slot) = resolved.price_slot {
                            self.emit_expr(price, expr_info, user_calls);
                            self.emit(
                                Instruction::new(OpCode::StoreLocal)
                                    .with_a(slot)
                                    .with_span(stmt.span),
                            );
                        }
                    }
                    OrderSpecKind::StopMarket { trigger_price, .. }
                    | OrderSpecKind::TakeProfitMarket { trigger_price, .. } => {
                        if let Some(slot) = resolved.trigger_price_slot {
                            self.emit_expr(trigger_price, expr_info, user_calls);
                            self.emit(
                                Instruction::new(OpCode::StoreLocal)
                                    .with_a(slot)
                                    .with_span(stmt.span),
                            );
                        }
                    }
                    OrderSpecKind::StopLimit {
                        trigger_price,
                        limit_price,
                        expire_time_ms,
                        ..
                    }
                    | OrderSpecKind::TakeProfitLimit {
                        trigger_price,
                        limit_price,
                        expire_time_ms,
                        ..
                    } => {
                        if let Some(slot) = resolved.trigger_price_slot {
                            self.emit_expr(trigger_price, expr_info, user_calls);
                            self.emit(
                                Instruction::new(OpCode::StoreLocal)
                                    .with_a(slot)
                                    .with_span(stmt.span),
                            );
                        }
                        if let Some(slot) = resolved.price_slot {
                            self.emit_expr(limit_price, expr_info, user_calls);
                            self.emit(
                                Instruction::new(OpCode::StoreLocal)
                                    .with_a(slot)
                                    .with_span(stmt.span),
                            );
                        }
                        if let Some(slot) = resolved.expire_time_slot {
                            self.emit_expr(expire_time_ms, expr_info, user_calls);
                            self.emit(
                                Instruction::new(OpCode::StoreLocal)
                                    .with_a(slot)
                                    .with_span(stmt.span),
                            );
                        }
                    }
                }
            }
            StmtKind::OrderSize { expr, .. } => {
                let resolved = self.analysis.resolved_order_field_slots[&stmt.id];
                match classify_order_size_expr(expr) {
                    OrderSizeExpr::CapitalFraction(size_expr) => {
                        if let Some(slot) = resolved.size_slot {
                            self.emit_expr(size_expr, expr_info, user_calls);
                            self.emit(
                                Instruction::new(OpCode::StoreLocal)
                                    .with_a(slot)
                                    .with_span(stmt.span),
                            );
                        }
                    }
                    OrderSizeExpr::RiskPct { pct, stop_price } => {
                        if let Some(slot) = resolved.size_slot {
                            self.emit_expr(pct, expr_info, user_calls);
                            self.emit(
                                Instruction::new(OpCode::StoreLocal)
                                    .with_a(slot)
                                    .with_span(stmt.span),
                            );
                        }
                        if let Some(slot) = resolved.risk_stop_slot {
                            self.emit_expr(stop_price, expr_info, user_calls);
                            self.emit(
                                Instruction::new(OpCode::StoreLocal)
                                    .with_a(slot)
                                    .with_span(stmt.span),
                            );
                        }
                    }
                    OrderSizeExpr::InvalidCapitalFractionArity
                    | OrderSizeExpr::InvalidRiskPctArity => {}
                }
            }
            StmtKind::RiskControl { .. }
            | StmtKind::PortfolioControl { .. }
            | StmtKind::PortfolioGroup { .. } => {}
            StmtKind::If {
                condition,
                then_block,
                else_block,
            } => {
                self.emit_expr(condition, expr_info, user_calls);
                let jump_if_false = self.emit_placeholder(OpCode::JumpIfFalse, condition.span);
                self.push_scope();
                self.emit_block(then_block, expr_info, user_calls);
                self.pop_scope();
                let jump_over_else = self.emit_placeholder(OpCode::Jump, stmt.span);
                self.patch_jump(jump_if_false, self.program.instructions.len());
                self.push_scope();
                self.emit_block(else_block, expr_info, user_calls);
                self.pop_scope();
                self.patch_jump(jump_over_else, self.program.instructions.len());
            }
            StmtKind::Expr(expr) => {
                self.emit_expr(expr, expr_info, user_calls);
                if expr_info.get(&expr.id).and_then(|info| info.concrete()) != Some(Type::Void) {
                    self.emit(Instruction::new(OpCode::Pop).with_span(stmt.span));
                }
            }
        }
    }

    fn emit_block(
        &mut self,
        block: &Block,
        expr_info: &HashMap<NodeId, ExprInfo>,
        user_calls: &HashMap<NodeId, FunctionSpecializationKey>,
    ) {
        for stmt in &block.statements {
            self.emit_stmt(stmt, expr_info, user_calls);
        }
    }

    fn emit_expr(
        &mut self,
        expr: &Expr,
        expr_info: &HashMap<NodeId, ExprInfo>,
        user_calls: &HashMap<NodeId, FunctionSpecializationKey>,
    ) {
        match &expr.kind {
            ExprKind::Number(value) => {
                let index = self.push_constant(Value::F64(*value));
                self.emit(
                    Instruction::new(OpCode::LoadConst)
                        .with_a(index)
                        .with_span(expr.span),
                );
            }
            ExprKind::Bool(value) => {
                let index = self.push_constant(Value::Bool(*value));
                self.emit(
                    Instruction::new(OpCode::LoadConst)
                        .with_a(index)
                        .with_span(expr.span),
                );
            }
            ExprKind::Na => {
                let index = self.push_constant(Value::NA);
                self.emit(
                    Instruction::new(OpCode::LoadConst)
                        .with_a(index)
                        .with_span(expr.span),
                );
            }
            ExprKind::EnumVariant {
                namespace,
                variant,
                variant_span,
                ..
            } => match resolve_enum_variant(namespace, variant) {
                Some(value) => {
                    let index = self.push_constant(value);
                    self.emit(
                        Instruction::new(OpCode::LoadConst)
                            .with_a(index)
                            .with_span(expr.span),
                    );
                }
                None => self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Compile,
                    format!("unknown enum variant `{}.{}`", namespace, variant),
                    *variant_span,
                )),
            },
            ExprKind::PositionField { field, .. } => {
                let Some(slot) = self.analysis.position_field_slots.get(field).copied() else {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Compile,
                        format!(
                            "missing compiled position slot for `position.{}`",
                            field.as_str()
                        ),
                        expr.span,
                    ));
                    return;
                };
                self.emit(
                    Instruction::new(OpCode::LoadLocal)
                        .with_a(slot)
                        .with_span(expr.span),
                );
            }
            ExprKind::PositionEventField { field, .. } => {
                let Some(slot) = self.analysis.position_event_field_slots.get(field).copied()
                else {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Compile,
                        format!(
                            "missing compiled position-event slot for `position_event.{}`",
                            field.as_str()
                        ),
                        expr.span,
                    ));
                    return;
                };
                self.emit(
                    Instruction::new(OpCode::LoadLocal)
                        .with_a(slot)
                        .with_span(expr.span),
                );
            }
            ExprKind::LastExitField { scope, field, .. } => {
                let Some(slot) = self
                    .analysis
                    .last_exit_field_slots
                    .get(&(*scope, *field))
                    .copied()
                else {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Compile,
                        format!(
                            "missing compiled last-exit slot for `{}.{}`",
                            scope.namespace(),
                            field.as_str()
                        ),
                        expr.span,
                    ));
                    return;
                };
                self.emit(
                    Instruction::new(OpCode::LoadLocal)
                        .with_a(slot)
                        .with_span(expr.span),
                );
            }
            ExprKind::String(_) => {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Compile,
                    "string literals are not executable expressions",
                    expr.span,
                ));
            }
            ExprKind::Ident(name) => match self.lookup_symbol(name) {
                Some(symbol) => self.emit(
                    Instruction::new(OpCode::LoadLocal)
                        .with_a(symbol.slot)
                        .with_span(expr.span),
                ),
                None => self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Compile,
                    format!("unknown identifier `{name}` during emission"),
                    expr.span,
                )),
            },
            ExprKind::SourceSeries {
                source,
                interval,
                field,
                ..
            } => {
                let slot = self.source_slot(source, *interval, *field);
                self.emit(
                    Instruction::new(OpCode::LoadLocal)
                        .with_a(slot)
                        .with_span(expr.span),
                );
            }
            ExprKind::Unary { op, expr: inner } => {
                self.emit_expr(inner, expr_info, user_calls);
                let opcode = match op {
                    UnaryOp::Neg => OpCode::Neg,
                    UnaryOp::Not => OpCode::Not,
                };
                self.emit(Instruction::new(opcode).with_span(expr.span));
            }
            ExprKind::Binary { op, left, right } => {
                self.emit_expr(left, expr_info, user_calls);
                self.emit_expr(right, expr_info, user_calls);
                let opcode = match op {
                    BinaryOp::And => OpCode::And,
                    BinaryOp::Or => OpCode::Or,
                    BinaryOp::Add => OpCode::Add,
                    BinaryOp::Sub => OpCode::Sub,
                    BinaryOp::Mul => OpCode::Mul,
                    BinaryOp::Div => OpCode::Div,
                    BinaryOp::Eq => OpCode::Eq,
                    BinaryOp::Ne => OpCode::Ne,
                    BinaryOp::Lt => OpCode::Lt,
                    BinaryOp::Le => OpCode::Le,
                    BinaryOp::Gt => OpCode::Gt,
                    BinaryOp::Ge => OpCode::Ge,
                };
                self.emit(Instruction::new(opcode).with_span(expr.span));
            }
            ExprKind::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                self.emit_expr(condition, expr_info, user_calls);
                let jump_if_false = self.emit_placeholder(OpCode::JumpIfFalse, condition.span);
                self.emit_expr(when_true, expr_info, user_calls);
                let jump_over_else = self.emit_placeholder(OpCode::Jump, expr.span);
                self.patch_jump(jump_if_false, self.program.instructions.len());
                self.emit_expr(when_false, expr_info, user_calls);
                self.patch_jump(jump_over_else, self.program.instructions.len());
            }
            ExprKind::Call { callee, args, .. } => {
                self.emit_call(expr, callee, args, expr_info, user_calls);
            }
            ExprKind::Index { target, index } => {
                let required_history =
                    literal_window(index, &self.analysis.immutable_values).unwrap_or_default() + 1;
                self.emit_series_ref(target, required_history.max(2), expr_info, user_calls);
                let offset = literal_window(index, &self.analysis.immutable_values)
                    .unwrap_or_default() as u16;
                self.emit(
                    Instruction::new(OpCode::SeriesGet)
                        .with_a(offset)
                        .with_span(expr.span),
                );
            }
        }
    }

    fn emit_call(
        &mut self,
        expr: &Expr,
        callee: &str,
        args: &[Expr],
        expr_info: &HashMap<NodeId, ExprInfo>,
        user_calls: &HashMap<NodeId, FunctionSpecializationKey>,
    ) {
        if let Some(key) = user_calls.get(&expr.id) {
            let Some(function) = self.functions_by_id.get(&key.function_id).copied() else {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Compile,
                    format!("unknown function `{callee}` during emission"),
                    expr.span,
                ));
                return;
            };
            let Some(spec) = self.analysis.function_specializations.get(key).cloned() else {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Compile,
                    format!("missing specialization for function `{callee}`"),
                    expr.span,
                ));
                return;
            };

            let mut scope = self.scopes.first().cloned().unwrap_or_default();
            for ((param, arg), binding) in function
                .params
                .iter()
                .zip(args.iter())
                .zip(spec.param_bindings.iter())
            {
                self.emit_expr(arg, expr_info, user_calls);
                let slot =
                    self.allocate_hidden_slot(binding.ty, binding.kind, binding.update_mask, 2);
                self.emit(
                    Instruction::new(OpCode::StoreLocal)
                        .with_a(slot)
                        .with_span(arg.span),
                );
                scope.insert(
                    param.name.clone(),
                    CompilerSymbol {
                        slot,
                        ty: binding.ty,
                    },
                );
            }
            self.scopes.push(scope);
            self.emit_expr(&function.body, &spec.expr_info, &spec.user_function_calls);
            self.pop_scope();
            return;
        }

        let Some(builtin) = BuiltinId::from_name(callee) else {
            let message = if talib_metadata_by_name(callee).is_some() {
                format!("builtin `{callee}` is reserved by the TA-Lib catalog but is not implemented yet")
            } else {
                format!("unknown builtin `{callee}`")
            };
            self.diagnostics
                .push(Diagnostic::new(DiagnosticKind::Compile, message, expr.span));
            return;
        };

        let callsite = self.next_callsite();
        match builtin {
            BuiltinId::Plot => {
                self.emit_expr(&args[0], expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(1)
                        .with_c(0)
                        .with_span(expr.span),
                );
                self.program.plot_count = self.program.plot_count.max(1);
            }
            BuiltinId::Sma
            | BuiltinId::Ema
            | BuiltinId::Rsi
            | BuiltinId::Acos
            | BuiltinId::Asin
            | BuiltinId::Atan
            | BuiltinId::Ceil
            | BuiltinId::Cos
            | BuiltinId::Cosh
            | BuiltinId::Exp
            | BuiltinId::Floor
            | BuiltinId::Ln
            | BuiltinId::Log10
            | BuiltinId::Sin
            | BuiltinId::Sinh
            | BuiltinId::Sqrt
            | BuiltinId::Tan
            | BuiltinId::Tanh
            | BuiltinId::Highest
            | BuiltinId::Lowest
            | BuiltinId::Sum
            | BuiltinId::Rising
            | BuiltinId::Falling
            | BuiltinId::Activated
            | BuiltinId::Deactivated
            | BuiltinId::State
            | BuiltinId::BarsSince
            | BuiltinId::ValueWhen
            | BuiltinId::HighestSince
            | BuiltinId::LowestSince
            | BuiltinId::HighestBarsSince
            | BuiltinId::LowestBarsSince
            | BuiltinId::ValueWhenSince
            | BuiltinId::CountSince
            | BuiltinId::Cross
            | BuiltinId::Crossover
            | BuiltinId::Crossunder
            | BuiltinId::Change
            | BuiltinId::Roc
            | BuiltinId::Ma
            | BuiltinId::Macd
            | BuiltinId::Obv
            | BuiltinId::AnchoredVwap
            | BuiltinId::Trange
            | BuiltinId::Wma
            | BuiltinId::Avgdev
            | BuiltinId::Percentile
            | BuiltinId::MaxIndex
            | BuiltinId::MinIndex
            | BuiltinId::MinMax
            | BuiltinId::MinMaxIndex
            | BuiltinId::Donchian
            | BuiltinId::Stddev
            | BuiltinId::Var
            | BuiltinId::Zscore
            | BuiltinId::UlcerIndex
            | BuiltinId::LinearReg
            | BuiltinId::LinearRegAngle
            | BuiltinId::LinearRegIntercept
            | BuiltinId::LinearRegSlope
            | BuiltinId::Tsf
            | BuiltinId::Beta
            | BuiltinId::Correl
            | BuiltinId::Mom
            | BuiltinId::Rocp
            | BuiltinId::Rocr
            | BuiltinId::Rocr100
            | BuiltinId::Apo
            | BuiltinId::Ppo
            | BuiltinId::Cmo
            | BuiltinId::Willr
            | BuiltinId::Aroon
            | BuiltinId::AroonOsc
            | BuiltinId::Supertrend
            | BuiltinId::Bop
            | BuiltinId::Cci
            | BuiltinId::Nz
            | BuiltinId::NaFunc
            | BuiltinId::Coalesce
            | BuiltinId::Cum
            | BuiltinId::HighestBars
            | BuiltinId::LowestBars
            | BuiltinId::Atr
            | BuiltinId::Natr
            | BuiltinId::PlusDm
            | BuiltinId::MinusDm
            | BuiltinId::PlusDi
            | BuiltinId::MinusDi
            | BuiltinId::Dx
            | BuiltinId::Adx
            | BuiltinId::Adxr
            | BuiltinId::Ad
            | BuiltinId::Adosc
            | BuiltinId::Mfi
            | BuiltinId::Imi
            | BuiltinId::Macdfix
            | BuiltinId::Bbands
            | BuiltinId::Dema
            | BuiltinId::Tema
            | BuiltinId::Trima
            | BuiltinId::Kama
            | BuiltinId::T3
            | BuiltinId::Trix
            | BuiltinId::Accbands
            | BuiltinId::Macdext
            | BuiltinId::Mavp
            | BuiltinId::Sar
            | BuiltinId::Sarext
            | BuiltinId::Stoch
            | BuiltinId::Stochf
            | BuiltinId::Stochrsi
            | BuiltinId::HtDcPeriod
            | BuiltinId::HtDcPhase
            | BuiltinId::HtPhasor
            | BuiltinId::HtSine
            | BuiltinId::HtTrendline
            | BuiltinId::HtTrendmode
            | BuiltinId::Mama => {
                self.emit_runtime_builtin_call(
                    builtin, expr, args, expr_info, user_calls, callsite,
                );
            }
            BuiltinId::Add | BuiltinId::Div | BuiltinId::Mult | BuiltinId::Sub => {
                self.emit_expr(&args[0], expr_info, user_calls);
                self.emit_expr(&args[1], expr_info, user_calls);
                let opcode = match builtin {
                    BuiltinId::Add => OpCode::Add,
                    BuiltinId::Div => OpCode::Div,
                    BuiltinId::Mult => OpCode::Mul,
                    BuiltinId::Sub => OpCode::Sub,
                    _ => unreachable!(),
                };
                self.emit(Instruction::new(opcode).with_span(expr.span));
            }
            BuiltinId::Avgprice => {
                self.emit_expr(&args[0], expr_info, user_calls);
                self.emit_expr(&args[1], expr_info, user_calls);
                self.emit(Instruction::new(OpCode::Add).with_span(expr.span));
                self.emit_expr(&args[2], expr_info, user_calls);
                self.emit(Instruction::new(OpCode::Add).with_span(expr.span));
                self.emit_expr(&args[3], expr_info, user_calls);
                self.emit(Instruction::new(OpCode::Add).with_span(expr.span));
                self.emit_f64_constant(4.0, expr.span);
                self.emit(Instruction::new(OpCode::Div).with_span(expr.span));
            }
            BuiltinId::Medprice => {
                self.emit_expr(&args[0], expr_info, user_calls);
                self.emit_expr(&args[1], expr_info, user_calls);
                self.emit(Instruction::new(OpCode::Add).with_span(expr.span));
                self.emit_f64_constant(2.0, expr.span);
                self.emit(Instruction::new(OpCode::Div).with_span(expr.span));
            }
            BuiltinId::Typprice => {
                self.emit_expr(&args[0], expr_info, user_calls);
                self.emit_expr(&args[1], expr_info, user_calls);
                self.emit(Instruction::new(OpCode::Add).with_span(expr.span));
                self.emit_expr(&args[2], expr_info, user_calls);
                self.emit(Instruction::new(OpCode::Add).with_span(expr.span));
                self.emit_f64_constant(3.0, expr.span);
                self.emit(Instruction::new(OpCode::Div).with_span(expr.span));
            }
            BuiltinId::Wclprice => {
                self.emit_expr(&args[0], expr_info, user_calls);
                self.emit_expr(&args[1], expr_info, user_calls);
                self.emit(Instruction::new(OpCode::Add).with_span(expr.span));
                self.emit_expr(&args[2], expr_info, user_calls);
                self.emit(Instruction::new(OpCode::Add).with_span(expr.span));
                self.emit_expr(&args[2], expr_info, user_calls);
                self.emit(Instruction::new(OpCode::Add).with_span(expr.span));
                self.emit_f64_constant(4.0, expr.span);
                self.emit(Instruction::new(OpCode::Div).with_span(expr.span));
            }
            BuiltinId::Max => {
                self.emit_series_window_alias_call(
                    BuiltinId::Highest,
                    &args[0],
                    args.get(1),
                    30,
                    expr_info,
                    user_calls,
                );
            }
            BuiltinId::Min => {
                self.emit_series_window_alias_call(
                    BuiltinId::Lowest,
                    &args[0],
                    args.get(1),
                    30,
                    expr_info,
                    user_calls,
                );
            }
            BuiltinId::Midpoint => {
                self.emit_series_window_alias_call(
                    BuiltinId::Highest,
                    &args[0],
                    args.get(1),
                    14,
                    expr_info,
                    user_calls,
                );
                self.emit_series_window_alias_call(
                    BuiltinId::Lowest,
                    &args[0],
                    args.get(1),
                    14,
                    expr_info,
                    user_calls,
                );
                self.emit(Instruction::new(OpCode::Add).with_span(expr.span));
                self.emit_f64_constant(2.0, expr.span);
                self.emit(Instruction::new(OpCode::Div).with_span(expr.span));
            }
            BuiltinId::Midprice => {
                self.emit_high_low_window_alias_call(
                    BuiltinId::Highest,
                    &args[0],
                    args.get(2),
                    14,
                    expr_info,
                    user_calls,
                );
                self.emit_high_low_window_alias_call(
                    BuiltinId::Lowest,
                    &args[1],
                    args.get(2),
                    14,
                    expr_info,
                    user_calls,
                );
                self.emit(Instruction::new(OpCode::Add).with_span(expr.span));
                self.emit_f64_constant(2.0, expr.span);
                self.emit(Instruction::new(OpCode::Div).with_span(expr.span));
            }
            BuiltinId::Above | BuiltinId::Below => {
                self.emit_expr(&args[0], expr_info, user_calls);
                self.emit_expr(&args[1], expr_info, user_calls);
                let opcode = if matches!(builtin, BuiltinId::Above) {
                    OpCode::Gt
                } else {
                    OpCode::Lt
                };
                self.emit(Instruction::new(opcode).with_span(expr.span));
            }
            BuiltinId::Between | BuiltinId::Outside => {
                if matches!(builtin, BuiltinId::Between) {
                    self.emit_expr(&args[1], expr_info, user_calls);
                    self.emit_expr(&args[0], expr_info, user_calls);
                    self.emit(Instruction::new(OpCode::Lt).with_span(expr.span));
                    self.emit_expr(&args[0], expr_info, user_calls);
                    self.emit_expr(&args[2], expr_info, user_calls);
                    self.emit(Instruction::new(OpCode::Lt).with_span(expr.span));
                } else {
                    self.emit_expr(&args[0], expr_info, user_calls);
                    self.emit_expr(&args[1], expr_info, user_calls);
                    self.emit(Instruction::new(OpCode::Lt).with_span(expr.span));
                    self.emit_expr(&args[2], expr_info, user_calls);
                    self.emit_expr(&args[0], expr_info, user_calls);
                    self.emit(Instruction::new(OpCode::Lt).with_span(expr.span));
                }
                let opcode = if matches!(builtin, BuiltinId::Between) {
                    OpCode::And
                } else {
                    OpCode::Or
                };
                self.emit(Instruction::new(opcode).with_span(expr.span));
            }
            BuiltinId::Open
            | BuiltinId::High
            | BuiltinId::Low
            | BuiltinId::Close
            | BuiltinId::Volume
            | BuiltinId::Time => {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Compile,
                    format!("builtin `{callee}` is not callable in v0.1"),
                    expr.span,
                ));
            }
        }
    }

    fn emit_runtime_builtin_call(
        &mut self,
        builtin: BuiltinId,
        expr: &Expr,
        args: &[Expr],
        expr_info: &HashMap<NodeId, ExprInfo>,
        user_calls: &HashMap<NodeId, FunctionSpecializationKey>,
        callsite: u16,
    ) {
        match builtin.kind() {
            BuiltinKind::NullCheck | BuiltinKind::NullCoalesce => {
                for arg in args {
                    self.emit_expr(arg, expr_info, user_calls);
                }
                if matches!(builtin, BuiltinId::Nz) && args.len() == 1 {
                    let fallback = expr_info
                        .get(&args[0].id)
                        .and_then(|info| info.concrete())
                        .unwrap_or(Type::F64);
                    match fallback {
                        Type::Bool | Type::SeriesBool => {
                            let index = self.push_constant(Value::Bool(false));
                            self.emit(
                                Instruction::new(OpCode::LoadConst)
                                    .with_a(index)
                                    .with_span(expr.span),
                            );
                        }
                        _ => self.emit_f64_constant(0.0, expr.span),
                    }
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(if matches!(builtin, BuiltinId::Nz) && args.len() == 1 {
                            2
                        } else {
                            args.len() as u16
                        })
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::UnaryMathTransform => {
                self.emit_expr(&args[0], expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(1)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::Indicator | BuiltinKind::Highest | BuiltinKind::Lowest => {
                let required_history = literal_window(&args[1], &self.analysis.immutable_values)
                    .map(|window| {
                        if matches!(builtin, BuiltinId::Highest | BuiltinId::Lowest) {
                            window
                        } else {
                            window + 1
                        }
                    })
                    .unwrap_or(2);
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_expr(&args[1], expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(args.len() as u16)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::HighestBars | BuiltinKind::LowestBars => {
                let required_history =
                    literal_window(&args[1], &self.analysis.immutable_values).unwrap_or(2);
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_expr(&args[1], expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(2)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::Cumulative => {
                self.emit_expr(&args[0], expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(1)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingSingleInput => {
                let default_window = match builtin {
                    BuiltinId::Max | BuiltinId::Min | BuiltinId::Sum => 30,
                    BuiltinId::Midpoint => 14,
                    BuiltinId::Wma | BuiltinId::MaxIndex | BuiltinId::MinIndex => 30,
                    BuiltinId::Avgdev => 14,
                    BuiltinId::Cmo => 14,
                    BuiltinId::Zscore => 20,
                    BuiltinId::UlcerIndex => 14,
                    BuiltinId::Dema
                    | BuiltinId::Tema
                    | BuiltinId::Trima
                    | BuiltinId::Kama
                    | BuiltinId::Trix => 30,
                    BuiltinId::HtDcPeriod
                    | BuiltinId::HtDcPhase
                    | BuiltinId::HtTrendline
                    | BuiltinId::HtTrendmode => 4,
                    BuiltinId::LinearReg
                    | BuiltinId::LinearRegAngle
                    | BuiltinId::LinearRegIntercept
                    | BuiltinId::LinearRegSlope
                    | BuiltinId::Tsf => 14,
                    _ => unreachable!(),
                };
                let required_history = if matches!(
                    builtin,
                    BuiltinId::HtDcPeriod
                        | BuiltinId::HtDcPhase
                        | BuiltinId::HtTrendline
                        | BuiltinId::HtTrendmode
                ) {
                    64
                } else {
                    args.get(1)
                        .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                        .map(|window| {
                            if matches!(builtin, BuiltinId::Kama) {
                                window + 1
                            } else {
                                window
                            }
                        })
                        .unwrap_or_else(|| {
                            if matches!(builtin, BuiltinId::Kama) {
                                default_window + 1
                            } else {
                                default_window
                            }
                        })
                };
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                if let Some(window) = args.get(1) {
                    self.emit_expr(window, expr_info, user_calls);
                } else if matches!(
                    builtin,
                    BuiltinId::HtDcPeriod
                        | BuiltinId::HtDcPhase
                        | BuiltinId::HtTrendline
                        | BuiltinId::HtTrendmode
                ) {
                    // Keep the stack shape uniform while still emitting a 1-arg builtin call.
                } else {
                    self.emit_f64_constant(default_window as f64, expr.span);
                }
                let arity = if matches!(
                    builtin,
                    BuiltinId::HtDcPeriod
                        | BuiltinId::HtDcPhase
                        | BuiltinId::HtTrendline
                        | BuiltinId::HtTrendmode
                ) {
                    1
                } else {
                    2
                };
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(arity)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingSingleInputPercentile => {
                let default_window = 20usize;
                let required_history = args
                    .get(1)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(default_window);
                self.emit_series_ref(&args[0], required_history.max(1), expr_info, user_calls);
                if let Some(window) = args.get(1) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(default_window as f64, expr.span);
                }
                if let Some(percentage) = args.get(2) {
                    self.emit_expr(percentage, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(50.0, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(3)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingSingleInputFactor => {
                let default_window = 5usize;
                let minimum = if matches!(builtin, BuiltinId::Var | BuiltinId::T3) {
                    1
                } else {
                    2
                };
                let required_history = args
                    .get(1)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(default_window);
                self.emit_series_ref(
                    &args[0],
                    required_history.max(minimum),
                    expr_info,
                    user_calls,
                );
                if let Some(window) = args.get(1) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(default_window as f64, expr.span);
                }
                if let Some(deviations) = args.get(2) {
                    self.emit_expr(deviations, expr_info, user_calls);
                } else {
                    let default_factor = if matches!(builtin, BuiltinId::T3) {
                        0.7
                    } else {
                        1.0
                    };
                    self.emit_f64_constant(default_factor, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(3)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingSingleInputTuple => {
                let default_window = match builtin {
                    BuiltinId::MinMax | BuiltinId::MinMaxIndex => 30,
                    BuiltinId::HtPhasor | BuiltinId::HtSine => 4,
                    _ => unreachable!(),
                };
                let required_history = if matches!(builtin, BuiltinId::HtPhasor | BuiltinId::HtSine)
                {
                    64
                } else {
                    args.get(1)
                        .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                        .unwrap_or(default_window)
                };
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                if let Some(window) = args.get(1) {
                    self.emit_expr(window, expr_info, user_calls);
                } else if matches!(builtin, BuiltinId::HtPhasor | BuiltinId::HtSine) {
                    // Keep the stack shape uniform while still emitting a 1-arg builtin call.
                } else {
                    self.emit_f64_constant(default_window as f64, expr.span);
                }
                let arity = if matches!(builtin, BuiltinId::HtPhasor | BuiltinId::HtSine) {
                    1
                } else {
                    2
                };
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(arity)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingHighLowTuple => {
                let required_history = args
                    .get(2)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(14)
                    + 1;
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[1], required_history.max(2), expr_info, user_calls);
                if let Some(window) = args.get(2) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(14.0, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(3)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingHighLowBands => {
                let required_history = args
                    .get(2)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(20);
                self.emit_series_ref(&args[0], required_history.max(1), expr_info, user_calls);
                self.emit_series_ref(&args[1], required_history.max(1), expr_info, user_calls);
                if let Some(window) = args.get(2) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(20.0, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(3)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingDoubleInput => {
                let default_window = match builtin {
                    BuiltinId::Beta => 5usize,
                    BuiltinId::Correl => 30usize,
                    BuiltinId::Imi => 14usize,
                    _ => unreachable!(),
                };
                let required_history = args
                    .get(2)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(default_window);
                let required_steps = match builtin {
                    BuiltinId::Beta => required_history + 1,
                    BuiltinId::Correl | BuiltinId::Imi => required_history,
                    _ => unreachable!(),
                };
                self.emit_series_ref(&args[0], required_steps.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[1], required_steps.max(2), expr_info, user_calls);
                if let Some(window) = args.get(2) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(default_window as f64, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(3)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingHighLow => {
                let required_history = args
                    .get(2)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .map(|window| {
                        if matches!(builtin, BuiltinId::AroonOsc) {
                            window + 1
                        } else {
                            window
                        }
                    })
                    .unwrap_or_else(|| {
                        if matches!(builtin, BuiltinId::AroonOsc) {
                            15
                        } else {
                            14
                        }
                    });
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[1], required_history.max(2), expr_info, user_calls);
                if let Some(window) = args.get(2) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(14.0, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(3)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingHighLowClose => {
                let required_history = args
                    .get(3)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .map(|window| {
                        if matches!(
                            builtin,
                            BuiltinId::Atr
                                | BuiltinId::Natr
                                | BuiltinId::PlusDi
                                | BuiltinId::MinusDi
                                | BuiltinId::Dx
                                | BuiltinId::Adx
                                | BuiltinId::Adxr
                        ) {
                            window + 1
                        } else {
                            window
                        }
                    })
                    .unwrap_or_else(|| {
                        if matches!(
                            builtin,
                            BuiltinId::Atr
                                | BuiltinId::Natr
                                | BuiltinId::PlusDi
                                | BuiltinId::MinusDi
                                | BuiltinId::Dx
                                | BuiltinId::Adx
                                | BuiltinId::Adxr
                        ) {
                            15
                        } else {
                            14
                        }
                    });
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[1], required_history.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[2], required_history.max(2), expr_info, user_calls);
                if let Some(window) = args.get(3) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(14.0, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(4)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::CurrentOhlc => {
                self.emit_series_ref(&args[0], 1, expr_info, user_calls);
                self.emit_series_ref(&args[1], 1, expr_info, user_calls);
                self.emit_series_ref(&args[2], 1, expr_info, user_calls);
                self.emit_series_ref(&args[3], 1, expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(4)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::MovingAverage => {
                let required_history =
                    literal_window(&args[1], &self.analysis.immutable_values).unwrap_or(2);
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_expr(&args[1], expr_info, user_calls);
                self.emit_expr(&args[2], expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(3)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::MaOscillator => {
                let fast = args
                    .get(1)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(12);
                let slow = args
                    .get(2)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(26);
                self.emit_series_ref(&args[0], fast.max(slow).max(2), expr_info, user_calls);
                if let Some(fast_expr) = args.get(1) {
                    self.emit_expr(fast_expr, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(12.0, expr.span);
                }
                if let Some(slow_expr) = args.get(2) {
                    self.emit_expr(slow_expr, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(26.0, expr.span);
                }
                if let Some(ma_type) = args.get(3) {
                    self.emit_expr(ma_type, expr_info, user_calls);
                } else {
                    let index = self.push_constant(Value::MaType(MaType::Sma));
                    self.emit(
                        Instruction::new(OpCode::LoadConst)
                            .with_a(index)
                            .with_span(expr.span),
                    );
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(4)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::IndicatorTuple => {
                let required_history =
                    literal_window(&args[2], &self.analysis.immutable_values).unwrap_or(2) + 1;
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_expr(&args[1], expr_info, user_calls);
                self.emit_expr(&args[2], expr_info, user_calls);
                self.emit_expr(&args[3], expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(4)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::IndicatorTupleSignal => {
                let signal = args
                    .get(1)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(9);
                self.emit_series_ref(&args[0], (26 + signal).max(2), expr_info, user_calls);
                if let Some(signal_expr) = args.get(1) {
                    self.emit_expr(signal_expr, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(9.0, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(2)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::IndicatorTupleMa => {
                let fast = args
                    .get(1)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(12);
                let slow = args
                    .get(3)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(26);
                let required_history =
                    ma_input_history_hint(args.get(2), fast, &self.analysis.immutable_values).max(
                        ma_input_history_hint(args.get(4), slow, &self.analysis.immutable_values),
                    );
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                if let Some(fast_expr) = args.get(1) {
                    self.emit_expr(fast_expr, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(12.0, expr.span);
                }
                self.emit_ma_type_argument(args.get(2), expr.span, expr_info, user_calls);
                if let Some(slow_expr) = args.get(3) {
                    self.emit_expr(slow_expr, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(26.0, expr.span);
                }
                self.emit_ma_type_argument(args.get(4), expr.span, expr_info, user_calls);
                if let Some(signal_expr) = args.get(5) {
                    self.emit_expr(signal_expr, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(9.0, expr.span);
                }
                self.emit_ma_type_argument(args.get(6), expr.span, expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(7)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::Bands => {
                let required_history = args
                    .get(1)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(5)
                    + 1;
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                if let Some(window) = args.get(1) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(5.0, expr.span);
                }
                if let Some(deviations_up) = args.get(2) {
                    self.emit_expr(deviations_up, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(2.0, expr.span);
                }
                if let Some(deviations_down) = args.get(3) {
                    self.emit_expr(deviations_down, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(2.0, expr.span);
                }
                if let Some(ma_type) = args.get(4) {
                    self.emit_expr(ma_type, expr_info, user_calls);
                } else {
                    let index = self.push_constant(Value::MaType(MaType::Sma));
                    self.emit(
                        Instruction::new(OpCode::LoadConst)
                            .with_a(index)
                            .with_span(expr.span),
                    );
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(5)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingHighLowCloseBands => {
                let required_history = args
                    .get(3)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(20);
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[1], required_history.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[2], required_history.max(2), expr_info, user_calls);
                if let Some(window) = args.get(3) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(20.0, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(4)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingQuadInputWindow => {
                let required_history = args
                    .get(4)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(14)
                    + 1;
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[1], required_history.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[2], required_history.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[3], required_history.max(2), expr_info, user_calls);
                if let Some(window) = args.get(4) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(14.0, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(5)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingQuadInputDoubleWindow => {
                let fast = args
                    .get(4)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(3);
                let slow = args
                    .get(5)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(10);
                let required_history = fast.max(slow).max(2);
                self.emit_series_ref(&args[0], required_history, expr_info, user_calls);
                self.emit_series_ref(&args[1], required_history, expr_info, user_calls);
                self.emit_series_ref(&args[2], required_history, expr_info, user_calls);
                self.emit_series_ref(&args[3], required_history, expr_info, user_calls);
                if let Some(fast) = args.get(4) {
                    self.emit_expr(fast, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(3.0, expr.span);
                }
                if let Some(slow) = args.get(5) {
                    self.emit_expr(slow, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(10.0, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(6)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingHighLowCloseTuple => {
                let fast_k = args
                    .get(3)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(5);
                self.emit_series_ref(&args[0], fast_k.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[1], fast_k.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[2], fast_k.max(2), expr_info, user_calls);
                if let Some(fast_k_expr) = args.get(3) {
                    self.emit_expr(fast_k_expr, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(5.0, expr.span);
                }
                if matches!(builtin, BuiltinId::Stoch) {
                    if let Some(slow_k_expr) = args.get(4) {
                        self.emit_expr(slow_k_expr, expr_info, user_calls);
                    } else {
                        self.emit_f64_constant(3.0, expr.span);
                    }
                    self.emit_ma_type_argument(args.get(5), expr.span, expr_info, user_calls);
                    if let Some(slow_d_expr) = args.get(6) {
                        self.emit_expr(slow_d_expr, expr_info, user_calls);
                    } else {
                        self.emit_f64_constant(3.0, expr.span);
                    }
                    self.emit_ma_type_argument(args.get(7), expr.span, expr_info, user_calls);
                    self.emit(
                        Instruction::new(OpCode::CallBuiltin)
                            .with_a(builtin as u16)
                            .with_b(8)
                            .with_c(callsite)
                            .with_span(expr.span),
                    );
                } else {
                    if let Some(fast_d_expr) = args.get(4) {
                        self.emit_expr(fast_d_expr, expr_info, user_calls);
                    } else {
                        self.emit_f64_constant(3.0, expr.span);
                    }
                    self.emit_ma_type_argument(args.get(5), expr.span, expr_info, user_calls);
                    self.emit(
                        Instruction::new(OpCode::CallBuiltin)
                            .with_a(builtin as u16)
                            .with_b(6)
                            .with_c(callsite)
                            .with_span(expr.span),
                    );
                }
            }
            BuiltinKind::RollingHighLowCloseTrendTuple => {
                let default_window = 10usize;
                let required_history = args
                    .get(3)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(default_window)
                    + 1;
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[1], required_history.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[2], required_history.max(2), expr_info, user_calls);
                if let Some(window) = args.get(3) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(default_window as f64, expr.span);
                }
                if let Some(multiplier) = args.get(4) {
                    self.emit_expr(multiplier, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(3.0, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(5)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingSingleInputTupleMa => {
                let time_period = args
                    .get(1)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(14);
                self.emit_series_ref(&args[0], (time_period + 1).max(2), expr_info, user_calls);
                if let Some(time_expr) = args.get(1) {
                    self.emit_expr(time_expr, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(14.0, expr.span);
                }
                if let Some(fast_k_expr) = args.get(2) {
                    self.emit_expr(fast_k_expr, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(5.0, expr.span);
                }
                if let Some(fast_d_expr) = args.get(3) {
                    self.emit_expr(fast_d_expr, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(3.0, expr.span);
                }
                self.emit_ma_type_argument(args.get(4), expr.span, expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(5)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::AdaptiveCycleTuple => {
                self.emit_series_ref(&args[0], 8, expr_info, user_calls);
                if let Some(fast_limit) = args.get(1) {
                    self.emit_expr(fast_limit, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(0.5, expr.span);
                }
                if let Some(slow_limit) = args.get(2) {
                    self.emit_expr(slow_limit, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(0.05, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(3)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::VariablePeriodMovingAverage => {
                let max_period =
                    literal_window(&args[3], &self.analysis.immutable_values).unwrap_or(30);
                let required_history =
                    ma_input_history_hint(args.get(4), max_period, &self.analysis.immutable_values);
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_series_ref(&args[1], 2, expr_info, user_calls);
                self.emit_expr(&args[2], expr_info, user_calls);
                self.emit_expr(&args[3], expr_info, user_calls);
                self.emit_expr(&args[4], expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(5)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::ParabolicSar => {
                self.emit_series_ref(&args[0], 2, expr_info, user_calls);
                self.emit_series_ref(&args[1], 2, expr_info, user_calls);
                if let Some(accel) = args.get(2) {
                    self.emit_expr(accel, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(0.02, expr.span);
                }
                if let Some(maximum) = args.get(3) {
                    self.emit_expr(maximum, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(0.2, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(4)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::ParabolicSarExt => {
                self.emit_series_ref(&args[0], 2, expr_info, user_calls);
                self.emit_series_ref(&args[1], 2, expr_info, user_calls);
                for (index, default) in [0.0, 0.0, 0.02, 0.02, 0.2, 0.02, 0.02, 0.2]
                    .iter()
                    .enumerate()
                {
                    if let Some(expr_arg) = args.get(index + 2) {
                        self.emit_expr(expr_arg, expr_info, user_calls);
                    } else {
                        self.emit_f64_constant(*default, expr.span);
                    }
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(10)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::Rising | BuiltinKind::Falling => {
                let required_history = literal_window(&args[1], &self.analysis.immutable_values)
                    .map(|window| window + 1)
                    .unwrap_or(2);
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_expr(&args[1], expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(2)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::BoolEdge => {
                self.emit_series_ref(&args[0], 2, expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(1)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::StateMachine => {
                self.emit_series_ref(&args[0], 2, expr_info, user_calls);
                self.emit_series_ref(&args[1], 2, expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(2)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::BarsSince => {
                self.emit_series_ref(&args[0], 2, expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(1)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::SinceExtrema | BuiltinKind::SinceOffset => {
                self.emit_series_ref(&args[0], 2, expr_info, user_calls);
                self.emit_series_ref(&args[1], 2, expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(2)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::SinceCount => {
                self.emit_series_ref(&args[0], 2, expr_info, user_calls);
                self.emit_series_ref(&args[1], 2, expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(2)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::ValueWhen => {
                self.emit_series_ref(&args[0], 2, expr_info, user_calls);
                self.emit_series_ref(&args[1], 2, expr_info, user_calls);
                self.emit_expr(&args[2], expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(3)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::SinceValueWhen => {
                self.emit_series_ref(&args[0], 2, expr_info, user_calls);
                self.emit_series_ref(&args[1], 2, expr_info, user_calls);
                self.emit_series_ref(&args[2], 2, expr_info, user_calls);
                self.emit_expr(&args[3], expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(4)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::Cross => {
                self.emit_cross_arg(&args[0], expr_info, user_calls, 2, 0);
                self.emit_cross_arg(&args[1], expr_info, user_calls, 2, 0);
                self.emit_cross_arg(&args[0], expr_info, user_calls, 2, 1);
                self.emit_cross_arg(&args[1], expr_info, user_calls, 2, 1);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(4)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::Change => {
                let required_history = literal_window(&args[1], &self.analysis.immutable_values)
                    .map(|window| window + 1)
                    .unwrap_or(2);
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                self.emit_expr(&args[1], expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(2)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::Roc => {
                let required_history = args
                    .get(1)
                    .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
                    .unwrap_or(10)
                    + 1;
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                if let Some(window) = args.get(1) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(10.0, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(2)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::VolumeIndicator => {
                self.emit_series_ref(&args[0], 2, expr_info, user_calls);
                self.emit_series_ref(&args[1], 2, expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(2)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::AnchoredPriceVolume => {
                self.emit_series_ref(&args[0], 1, expr_info, user_calls);
                self.emit_series_ref(&args[1], 1, expr_info, user_calls);
                self.emit_series_ref(&args[2], 1, expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(3)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::VolatilityIndicator => {
                self.emit_series_ref(&args[0], 2, expr_info, user_calls);
                self.emit_series_ref(&args[1], 2, expr_info, user_calls);
                self.emit_series_ref(&args[2], 2, expr_info, user_calls);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(3)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            _ => unreachable!(),
        }
    }

    fn emit_f64_constant(&mut self, value: f64, span: Span) {
        let index = self.push_constant(Value::F64(value));
        self.emit(
            Instruction::new(OpCode::LoadConst)
                .with_a(index)
                .with_span(span),
        );
    }

    fn emit_ma_type_argument(
        &mut self,
        arg: Option<&Expr>,
        span: Span,
        expr_info: &HashMap<NodeId, ExprInfo>,
        user_calls: &HashMap<NodeId, FunctionSpecializationKey>,
    ) {
        if let Some(arg) = arg {
            self.emit_expr(arg, expr_info, user_calls);
        } else {
            let index = self.push_constant(Value::MaType(MaType::Sma));
            self.emit(
                Instruction::new(OpCode::LoadConst)
                    .with_a(index)
                    .with_span(span),
            );
        }
    }

    fn emit_series_window_alias_call(
        &mut self,
        builtin: BuiltinId,
        series: &Expr,
        window: Option<&Expr>,
        default_window: usize,
        expr_info: &HashMap<NodeId, ExprInfo>,
        user_calls: &HashMap<NodeId, FunctionSpecializationKey>,
    ) {
        let required_history = window
            .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
            .unwrap_or(default_window);
        let callsite = self.next_callsite();
        self.emit_series_ref(series, required_history.max(2), expr_info, user_calls);
        if let Some(window) = window {
            self.emit_expr(window, expr_info, user_calls);
        } else {
            self.emit_f64_constant(default_window as f64, series.span);
        }
        self.emit(
            Instruction::new(OpCode::CallBuiltin)
                .with_a(builtin as u16)
                .with_b(2)
                .with_c(callsite)
                .with_span(series.span),
        );
    }

    fn emit_high_low_window_alias_call(
        &mut self,
        builtin: BuiltinId,
        target: &Expr,
        window: Option<&Expr>,
        default_window: usize,
        expr_info: &HashMap<NodeId, ExprInfo>,
        user_calls: &HashMap<NodeId, FunctionSpecializationKey>,
    ) {
        let required_history = window
            .and_then(|expr| literal_window(expr, &self.analysis.immutable_values))
            .unwrap_or(default_window);
        let callsite = self.next_callsite();
        self.emit_series_ref(target, required_history.max(2), expr_info, user_calls);
        if let Some(window) = window {
            self.emit_expr(window, expr_info, user_calls);
        } else {
            self.emit_f64_constant(default_window as f64, target.span);
        }
        self.emit(
            Instruction::new(OpCode::CallBuiltin)
                .with_a(builtin as u16)
                .with_b(2)
                .with_c(callsite)
                .with_span(target.span),
        );
    }

    fn emit_cross_arg(
        &mut self,
        expr: &Expr,
        expr_info: &HashMap<NodeId, ExprInfo>,
        user_calls: &HashMap<NodeId, FunctionSpecializationKey>,
        required_history: usize,
        offset: usize,
    ) {
        let info = expr_info
            .get(&expr.id)
            .copied()
            .unwrap_or_else(|| ExprInfo::scalar(Type::F64));
        if matches!(info.ty, InferredType::Concrete(Type::SeriesF64)) {
            self.emit_series_ref(expr, required_history.max(2), expr_info, user_calls);
            self.emit(
                Instruction::new(OpCode::SeriesGet)
                    .with_a(offset as u16)
                    .with_span(expr.span),
            );
        } else {
            self.emit_expr(expr, expr_info, user_calls);
        }
    }

    fn emit_series_ref(
        &mut self,
        expr: &Expr,
        required_history: usize,
        expr_info: &HashMap<NodeId, ExprInfo>,
        user_calls: &HashMap<NodeId, FunctionSpecializationKey>,
    ) {
        match &expr.kind {
            ExprKind::Ident(name) => match self.lookup_symbol(name) {
                Some(symbol) if symbol.ty.is_series() => {
                    self.bump_slot_history(symbol.slot, required_history);
                    self.emit(
                        Instruction::new(OpCode::LoadSeries)
                            .with_a(symbol.slot)
                            .with_span(expr.span),
                    );
                }
                _ => {
                    self.emit_materialized_series_ref(expr, required_history, expr_info, user_calls)
                }
            },
            ExprKind::SourceSeries {
                source,
                interval,
                field,
                ..
            } => {
                let slot = self.source_slot(source, *interval, *field);
                self.bump_slot_history(slot, required_history);
                self.emit(
                    Instruction::new(OpCode::LoadSeries)
                        .with_a(slot)
                        .with_span(expr.span),
                );
            }
            _ => self.emit_materialized_series_ref(expr, required_history, expr_info, user_calls),
        }
    }

    fn emit_materialized_series_ref(
        &mut self,
        expr: &Expr,
        required_history: usize,
        expr_info: &HashMap<NodeId, ExprInfo>,
        user_calls: &HashMap<NodeId, FunctionSpecializationKey>,
    ) {
        let info = expr_info
            .get(&expr.id)
            .copied()
            .unwrap_or(ExprInfo::series(0));
        let ty = match info.ty {
            InferredType::Concrete(Type::Bool | Type::SeriesBool) => Type::SeriesBool,
            _ => Type::SeriesF64,
        };
        let temp_slot =
            self.allocate_hidden_slot(ty, SlotKind::Series, info.update_mask, required_history);
        self.emit_expr(expr, expr_info, user_calls);
        self.emit(
            Instruction::new(OpCode::StoreLocal)
                .with_a(temp_slot)
                .with_span(expr.span),
        );
        self.emit(
            Instruction::new(OpCode::LoadSeries)
                .with_a(temp_slot)
                .with_span(expr.span),
        );
    }

    fn emit(&mut self, instruction: Instruction) {
        self.program.instructions.push(instruction);
    }

    fn emit_placeholder(&mut self, opcode: OpCode, span: Span) -> usize {
        let index = self.program.instructions.len();
        self.program
            .instructions
            .push(Instruction::new(opcode).with_span(span));
        index
    }

    fn patch_jump(&mut self, index: usize, target: usize) {
        self.program.instructions[index].a = target as u16;
    }

    fn push_constant(&mut self, value: Value) -> u16 {
        let index = self.program.constants.len() as u16;
        self.program.constants.push(Constant::Value(value));
        index
    }

    fn allocate_hidden_slot(
        &mut self,
        ty: Type,
        kind: SlotKind,
        update_mask: u32,
        history_capacity: usize,
    ) -> u16 {
        let slot = self.program.locals.len() as u16;
        let mut local = if matches!(kind, SlotKind::Series) {
            LocalInfo::series(None, ty, true, update_mask, None)
        } else {
            LocalInfo::scalar(None, ty, true)
        };
        local.history_capacity = if matches!(kind, SlotKind::Series) {
            history_capacity.max(2)
        } else {
            1
        };
        self.program.locals.push(local);
        slot
    }

    fn bump_slot_history(&mut self, slot: u16, required_history: usize) {
        if let Some(local) = self.program.locals.get_mut(slot as usize) {
            if matches!(local.kind, SlotKind::Series) {
                local.history_capacity = local.history_capacity.max(required_history.max(2));
            }
        }
    }

    fn source_slot(&self, source: &str, interval: Option<Interval>, field: MarketField) -> u16 {
        let source_id = self
            .analysis
            .declared_sources
            .iter()
            .find(|decl| decl.alias == source)
            .map(|decl| decl.id)
            .expect("source slots should only be emitted for validated aliases");
        self.analysis.source_slots[&(source_id, interval, field)]
    }

    fn next_callsite(&mut self) -> u16 {
        let callsite = self.builtin_call_count;
        self.builtin_call_count += 1;
        callsite
    }

    fn rebuild_scopes(&mut self) {
        let mut root = HashMap::new();
        for (name, info) in &self.analysis.immutable_bindings {
            let Some(&slot) = self.analysis.immutable_binding_slots.get(name) else {
                continue;
            };
            let ty = info.concrete().unwrap_or(Type::F64);
            root.insert(name.clone(), CompilerSymbol { slot, ty });
        }
        self.scopes = vec![root];
    }

    fn lookup_symbol(&self, name: &str) -> Option<CompilerSymbol> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

fn collect_input_decls(ast: &Ast, analysis: &Analysis) -> Vec<InputDecl> {
    let mut inputs = Vec::new();
    for stmt in &ast.statements {
        let StmtKind::Input {
            name, optimization, ..
        } = &stmt.kind
        else {
            continue;
        };
        let Some(info) = analysis.immutable_bindings.get(name).copied() else {
            continue;
        };
        let Some(ty) = info.concrete() else {
            continue;
        };
        let Some(default_value) = analysis.immutable_values.get(name).cloned() else {
            continue;
        };
        inputs.push(InputDecl {
            name: name.clone(),
            ty,
            default_value,
            optimization: optimization.as_ref().map(|metadata| InputOptimizationDecl {
                kind: match &metadata.kind {
                    InputOptimizationKind::IntegerRange { low, high, step } => {
                        InputOptimizationDeclKind::IntegerRange {
                            low: *low,
                            high: *high,
                            step: *step,
                        }
                    }
                    InputOptimizationKind::FloatRange { low, high, step } => {
                        InputOptimizationDeclKind::FloatRange {
                            low: *low,
                            high: *high,
                            step: *step,
                        }
                    }
                    InputOptimizationKind::Choice { values } => InputOptimizationDeclKind::Choice {
                        values: values.clone(),
                    },
                },
            }),
        });
    }
    inputs
}
