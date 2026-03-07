//! Source-to-bytecode compilation for PalmScript programs.
//!
//! This module drives lexing and parsing, performs semantic analysis and type
//! inference, resolves locals and builtins, and emits deterministic bytecode.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ast::{
    Ast, BinaryOp, Block, Expr, ExprKind, FunctionDecl, NodeId, Stmt, StmtKind, UnaryOp,
};
use crate::builtins::{BuiltinArity, BuiltinId, BuiltinKind};
use crate::bytecode::{Constant, Instruction, LocalInfo, OpCode, OutputDecl, OutputKind, Program};
use crate::diagnostic::{CompileError, Diagnostic, DiagnosticKind};
use crate::interval::{
    DeclaredMarketSource, Interval, MarketBinding, MarketField, MarketSource, SourceIntervalRef,
};
use crate::lexer;
use crate::parser;
use crate::span::Span;
use crate::talib::{metadata_by_name as talib_metadata_by_name, MaType, TalibFunctionMetadata};
use crate::types::{SlotKind, Type, Value};

const BASE_UPDATE_MASK: u32 = 1;
const PREDEFINED_SERIES: [(&str, MarketField); 6] = [
    ("open", MarketField::Open),
    ("high", MarketField::High),
    ("low", MarketField::Low),
    ("close", MarketField::Close),
    ("volume", MarketField::Volume),
    ("time", MarketField::Time),
];

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CompiledProgram {
    pub program: Program,
    pub source: String,
}

pub fn compile(source: &str) -> Result<CompiledProgram, CompileError> {
    let tokens = lexer::lex(source)?;
    let ast = parser::parse(&tokens)?;
    Compiler::new(source, &ast).compile()
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
    declared_intervals: Vec<Interval>,
    declared_sources: Vec<DeclaredMarketSource>,
    source_intervals: Vec<SourceIntervalRef>,
    expr_info: HashMap<NodeId, ExprInfo>,
    user_function_calls: HashMap<NodeId, FunctionSpecializationKey>,
    resolved_let_slots: HashMap<NodeId, u16>,
    resolved_let_tuple_slots: HashMap<NodeId, Vec<u16>>,
    resolved_output_slots: HashMap<NodeId, u16>,
    locals: Vec<LocalInfo>,
    outputs: Vec<OutputDecl>,
    qualified_slots: HashMap<(Interval, MarketField), u16>,
    source_slots: HashMap<(u16, Option<Interval>, MarketField), u16>,
    function_specializations: HashMap<FunctionSpecializationKey, FunctionSpecialization>,
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
        };

        if ast.strategy_intervals.sources.is_empty() {
            for (name, field) in PREDEFINED_SERIES {
                analyzer.define_symbol(
                    name.to_string(),
                    ExprInfo::series(BASE_UPDATE_MASK),
                    true,
                    Some(MarketBinding {
                        source: MarketSource::Base,
                        field,
                    }),
                );
            }
        }
        analyzer.validate_strategy_intervals(ast);
        analyzer.collect_functions(ast);
        analyzer.collect_qualified_series(ast);
        analyzer.validate_function_bodies();
        analyzer.validate_function_cycles();
        analyzer
    }

    fn analyze(mut self, ast: &Ast) -> Result<Analysis, CompileError> {
        for stmt in &ast.statements {
            self.analyze_stmt(stmt);
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
            let mut supplemental = BTreeSet::new();
            for decl in &ast.strategy_intervals.supplemental {
                if decl.interval == base_interval {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "`use {}` duplicates the base interval",
                            decl.interval.as_str()
                        ),
                        decl.span,
                    ));
                    continue;
                }
                if !supplemental.insert(decl.interval) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("duplicate `use {}` declaration", decl.interval.as_str()),
                        decl.span,
                    ));
                }
            }
            self.analysis.declared_intervals = supplemental.iter().copied().collect();

            let mut refs = BTreeSet::new();
            for function in &ast.functions {
                collect_qualified_series_refs(&function.body, &mut refs);
            }
            for stmt in &ast.statements {
                collect_qualified_series_stmt(stmt, &mut refs);
            }
            for (interval, _field) in refs {
                if interval != base_interval && !supplemental.contains(&interval) {
                    let span = interval_ref_span(ast, interval).unwrap_or_default();
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "interval `{}` must be declared with `use {}`",
                            interval.as_str(),
                            interval.as_str()
                        ),
                        span,
                    ));
                }
            }
            return;
        }

        let mut sources_by_alias = BTreeMap::new();
        for source in &ast.strategy_intervals.sources {
            if sources_by_alias.contains_key(source.alias.as_str()) {
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
            sources_by_alias.insert(source.alias.as_str(), id);
        }

        let mut uses = BTreeSet::new();
        for decl in &ast.strategy_intervals.supplemental {
            let Some(&source_id) = sources_by_alias.get(decl.source.as_str()) else {
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
            let Some(&source_id) = sources_by_alias.get(source.as_str()) else {
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

    fn collect_qualified_series(&mut self, ast: &Ast) {
        if ast.strategy_intervals.sources.is_empty() {
            let mut refs = BTreeSet::new();
            for function in &ast.functions {
                collect_qualified_series_refs(&function.body, &mut refs);
            }
            for stmt in &ast.statements {
                collect_qualified_series_stmt(stmt, &mut refs);
            }

            for (interval, field) in refs {
                let slot = self.analysis.locals.len() as u16;
                self.analysis.locals.push(LocalInfo::series(
                    None,
                    Type::SeriesF64,
                    true,
                    interval.mask(),
                    Some(MarketBinding {
                        source: MarketSource::Qualified(interval),
                        field,
                    }),
                ));
                self.analysis
                    .qualified_slots
                    .insert((interval, field), slot);
            }
            return;
        }

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
                        if self.analysis.declared_sources.is_empty() {
                            format!(
                                "function bodies may only reference parameters or predefined series; found `{name}`"
                            )
                        } else {
                            format!(
                                "function bodies may only reference parameters or declared source series; found `{name}`"
                            )
                        },
                        expr.span,
                    ));
                }
            }
            ExprKind::QualifiedSeries { .. }
            | ExprKind::SourceSeries { .. }
            | ExprKind::EnumVariant { .. } => {}
            ExprKind::Unary { expr, .. } => self.validate_function_expr(expr, params),
            ExprKind::Binary { left, right, .. } => {
                self.validate_function_expr(left, params);
                self.validate_function_expr(right, params);
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
            StmtKind::Export { name, expr, .. } => {
                self.analyze_output_stmt(stmt, name, expr, OutputKind::ExportSeries);
            }
            StmtKind::Trigger { name, expr, .. } => {
                self.analyze_output_stmt(stmt, name, expr, OutputKind::Trigger);
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

    fn analyze_output_stmt(&mut self, stmt: &Stmt, name: &str, expr: &Expr, kind: OutputKind) {
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
                        "trigger requires bool, series<bool>, or na",
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
            ty,
            slot,
        });
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
                Some(ma_type) => {
                    self.analysis
                        .expr_info
                        .insert(expr.id, ExprInfo::scalar(Type::MaType));
                    let _ = ma_type;
                    ExprInfo::scalar(Type::MaType)
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
            ExprKind::Ident(name) => {
                let Some(symbol) = self.lookup_symbol(name) else {
                    if !self.analysis.declared_sources.is_empty() && is_predefined_series_name(name)
                    {
                        self.diagnostics.push(Diagnostic::new(
                            DiagnosticKind::Type,
                            format!(
                                "source-aware scripts require source-qualified market series; found `{name}`"
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
            ExprKind::QualifiedSeries { interval, .. } => ExprInfo::series(interval.mask()),
            ExprKind::SourceSeries { interval, .. } => {
                ExprInfo::series(interval.map_or(BASE_UPDATE_MASK, Interval::mask))
            }
            ExprKind::Unary { op, expr: inner } => self.analyze_unary(*op, inner),
            ExprKind::Binary { op, left, right } => self.analyze_binary(*op, left, right),
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
                    span,
                    &mut self.diagnostics,
                )
            }
        }
    }

    fn analyze_index(&mut self, target: &Expr, index: &Expr, span: Span) -> ExprInfo {
        let target_info = self.analyze_expr(target);
        let Some(_) = literal_window(index) else {
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
        self.analysis.declared_sources.is_empty() && is_predefined_series_name(name)
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
        let mut root = HashMap::new();
        if parent.analysis.declared_sources.is_empty() {
            for (name, _) in PREDEFINED_SERIES {
                root.insert(
                    name.to_string(),
                    AnalyzerSymbol {
                        info: ExprInfo::series(BASE_UPDATE_MASK),
                    },
                );
            }
        }

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
                Some(_) => ExprInfo::scalar(Type::MaType),
                None => {
                    self.parent.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("unknown enum variant `{}.{}`", namespace, variant),
                        *variant_span,
                    ));
                    ExprInfo::scalar(Type::MaType)
                }
            },
            ExprKind::Ident(name) => match self.lookup_symbol(name) {
                Some(symbol) => symbol.info,
                None => {
                    self.parent.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        if self.parent.analysis.declared_sources.is_empty() {
                            format!(
                                "function bodies may only reference parameters or predefined series; found `{name}`"
                            )
                        } else {
                            format!(
                                "function bodies may only reference parameters or declared source series; found `{name}`"
                            )
                        },
                        expr.span,
                    ));
                    ExprInfo::scalar(Type::F64)
                }
            },
            ExprKind::QualifiedSeries { interval, .. } => ExprInfo::series(interval.mask()),
            ExprKind::SourceSeries { interval, .. } => {
                ExprInfo::series(interval.map_or(BASE_UPDATE_MASK, Interval::mask))
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
                    span,
                    &mut self.parent.diagnostics,
                )
            }
        }
    }

    fn analyze_index(&mut self, target: &Expr, index: &Expr, span: Span) -> ExprInfo {
        let target_info = self.analyze_expr(target);
        let Some(_) = literal_window(index) else {
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

fn collect_qualified_series_stmt(stmt: &Stmt, refs: &mut BTreeSet<(Interval, MarketField)>) {
    match &stmt.kind {
        StmtKind::Let { expr, .. }
        | StmtKind::LetTuple { expr, .. }
        | StmtKind::Export { expr, .. }
        | StmtKind::Trigger { expr, .. }
        | StmtKind::Expr(expr) => collect_qualified_series_refs(expr, refs),
        StmtKind::If {
            condition,
            then_block,
            else_block,
        } => {
            collect_qualified_series_refs(condition, refs);
            for stmt in &then_block.statements {
                collect_qualified_series_stmt(stmt, refs);
            }
            for stmt in &else_block.statements {
                collect_qualified_series_stmt(stmt, refs);
            }
        }
    }
}

fn collect_source_series_stmt(
    stmt: &Stmt,
    refs: &mut BTreeSet<(String, Option<Interval>, MarketField)>,
) {
    match &stmt.kind {
        StmtKind::Let { expr, .. }
        | StmtKind::LetTuple { expr, .. }
        | StmtKind::Export { expr, .. }
        | StmtKind::Trigger { expr, .. }
        | StmtKind::Expr(expr) => collect_source_series_refs(expr, refs),
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

fn collect_qualified_series_refs(expr: &Expr, refs: &mut BTreeSet<(Interval, MarketField)>) {
    match &expr.kind {
        ExprKind::QualifiedSeries { interval, field } => {
            refs.insert((*interval, *field));
        }
        ExprKind::Unary { expr, .. } => collect_qualified_series_refs(expr, refs),
        ExprKind::Binary { left, right, .. } => {
            collect_qualified_series_refs(left, refs);
            collect_qualified_series_refs(right, refs);
        }
        ExprKind::Call { args, .. } => {
            for arg in args {
                collect_qualified_series_refs(arg, refs);
            }
        }
        ExprKind::Index { target, index } => {
            collect_qualified_series_refs(target, refs);
            collect_qualified_series_refs(index, refs);
        }
        ExprKind::Number(_)
        | ExprKind::Bool(_)
        | ExprKind::Na
        | ExprKind::String(_)
        | ExprKind::Ident(_)
        | ExprKind::EnumVariant { .. }
        | ExprKind::SourceSeries { .. } => {}
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
        | ExprKind::EnumVariant { .. }
        | ExprKind::QualifiedSeries { .. } => {}
    }
}

fn interval_ref_span(ast: &Ast, target: Interval) -> Option<Span> {
    for function in &ast.functions {
        if let Some(span) = expr_interval_ref_span(&function.body, target) {
            return Some(span);
        }
    }
    for stmt in &ast.statements {
        if let Some(span) = stmt_interval_ref_span(stmt, target) {
            return Some(span);
        }
    }
    None
}

fn stmt_interval_ref_span(stmt: &Stmt, target: Interval) -> Option<Span> {
    match &stmt.kind {
        StmtKind::Let { expr, .. }
        | StmtKind::LetTuple { expr, .. }
        | StmtKind::Export { expr, .. }
        | StmtKind::Trigger { expr, .. }
        | StmtKind::Expr(expr) => expr_interval_ref_span(expr, target),
        StmtKind::If {
            condition,
            then_block,
            else_block,
        } => expr_interval_ref_span(condition, target)
            .or_else(|| {
                then_block
                    .statements
                    .iter()
                    .find_map(|stmt| stmt_interval_ref_span(stmt, target))
            })
            .or_else(|| {
                else_block
                    .statements
                    .iter()
                    .find_map(|stmt| stmt_interval_ref_span(stmt, target))
            }),
    }
}

fn expr_interval_ref_span(expr: &Expr, target: Interval) -> Option<Span> {
    match &expr.kind {
        ExprKind::QualifiedSeries { interval, .. } if *interval == target => Some(expr.span),
        ExprKind::Unary { expr, .. } => expr_interval_ref_span(expr, target),
        ExprKind::Binary { left, right, .. } => {
            expr_interval_ref_span(left, target).or_else(|| expr_interval_ref_span(right, target))
        }
        ExprKind::Call { args, .. } => args
            .iter()
            .find_map(|arg| expr_interval_ref_span(arg, target)),
        ExprKind::Index {
            target: inner,
            index,
        } => {
            expr_interval_ref_span(inner, target).or_else(|| expr_interval_ref_span(index, target))
        }
        ExprKind::Number(_)
        | ExprKind::Bool(_)
        | ExprKind::Na
        | ExprKind::String(_)
        | ExprKind::Ident(_)
        | ExprKind::EnumVariant { .. }
        | ExprKind::SourceSeries { .. } => None,
        ExprKind::QualifiedSeries { .. } => None,
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
        | StmtKind::LetTuple { expr, .. }
        | StmtKind::Export { expr, .. }
        | StmtKind::Trigger { expr, .. }
        | StmtKind::Expr(expr) => expr_source_ref_span(expr, source, target),
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
        | ExprKind::EnumVariant { .. }
        | ExprKind::QualifiedSeries { .. }
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
        | ExprKind::EnumVariant { .. }
        | ExprKind::QualifiedSeries { .. }
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

fn analyze_helper_builtin(
    builtin: BuiltinId,
    callee: &str,
    args: &[Expr],
    arg_info: &[ExprInfo],
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
            validate_positive_window_literal(callee, &args[1], diagnostics);
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
            validate_positive_window_literal(callee, &args[1], diagnostics);
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
            validate_positive_window_literal(callee, &args[1], diagnostics);
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
        BuiltinKind::IndicatorTuple => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            validate_positive_window_literal(callee, &args[1], diagnostics);
            validate_positive_window_literal(callee, &args[2], diagnostics);
            validate_positive_window_literal(callee, &args[3], diagnostics);
            ExprInfo {
                ty: InferredType::Tuple3([Type::SeriesF64, Type::SeriesF64, Type::SeriesF64]),
                update_mask: series_info.update_mask,
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
                validate_min_window_literal(callee, &args[1], 2, diagnostics);
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
                validate_min_window_literal(callee, &args[1], 2, diagnostics);
            }
            ExprInfo {
                ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesF64]),
                update_mask: series_info.update_mask,
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
                validate_min_window_literal(callee, &args[2], 2, diagnostics);
            }
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: high_info.update_mask | low_info.update_mask,
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
        BuiltinKind::Change | BuiltinKind::Roc => {
            let series_info = arg_info[0];
            if !series_info.ty.is_series_numeric() {
                diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("{callee} requires series<float> as the first argument"),
                    args[0].span,
                ));
            }
            validate_positive_window_literal(callee, &args[1], diagnostics);
            ExprInfo {
                ty: InferredType::Concrete(Type::SeriesF64),
                update_mask: series_info.update_mask,
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
            validate_non_negative_literal(callee, "occurrence", &args[2], diagnostics);
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
        BuiltinKind::Rising | BuiltinKind::Falling => ExprInfo {
            ty: InferredType::Concrete(Type::SeriesBool),
            update_mask: 0,
        },
        BuiltinKind::ValueWhen => ExprInfo::series(0),
        BuiltinKind::IndicatorTuple => ExprInfo {
            ty: InferredType::Tuple3([Type::SeriesF64, Type::SeriesF64, Type::SeriesF64]),
            update_mask: 0,
        },
        BuiltinKind::RollingSingleInputTuple => ExprInfo {
            ty: InferredType::Tuple2([Type::SeriesF64, Type::SeriesF64]),
            update_mask: 0,
        },
        BuiltinKind::UnaryMathTransform
        | BuiltinKind::NumericBinary
        | BuiltinKind::PriceTransform => numeric_result(arg_info),
        BuiltinKind::BarsSince
        | BuiltinKind::Indicator
        | BuiltinKind::MovingAverage
        | BuiltinKind::Change
        | BuiltinKind::Roc
        | BuiltinKind::Highest
        | BuiltinKind::Lowest
        | BuiltinKind::RollingSingleInput
        | BuiltinKind::RollingHighLow
        | BuiltinKind::VolumeIndicator
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

fn validate_positive_window_literal(callee: &str, expr: &Expr, diagnostics: &mut Vec<Diagnostic>) {
    match literal_window(expr) {
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
    minimum: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match literal_window(expr) {
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
    diagnostics: &mut Vec<Diagnostic>,
) {
    if literal_window(expr).is_none() {
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

fn literal_window(expr: &Expr) -> Option<usize> {
    match expr.kind {
        ExprKind::Number(value) if value >= 0.0 && value.fract() == 0.0 => Some(value as usize),
        _ => None,
    }
}

fn resolve_enum_variant(namespace: &str, variant: &str) -> Option<Value> {
    match namespace {
        "ma_type" => MaType::from_variant(variant).map(Value::MaType),
        _ => None,
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
    PREDEFINED_SERIES
        .iter()
        .any(|(predefined, _)| *predefined == name)
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
        self.program.outputs = self.analysis.outputs.clone();
        self.program.base_interval = self.analysis.base_interval;
        self.program.declared_intervals = self.analysis.declared_intervals.clone();
        self.program.declared_sources = self.analysis.declared_sources.clone();
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
            StmtKind::Export { name, expr, .. } | StmtKind::Trigger { name, expr, .. } => {
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
            ExprKind::QualifiedSeries { interval, field } => {
                let slot = self.qualified_slot(*interval, *field);
                self.emit(
                    Instruction::new(OpCode::LoadLocal)
                        .with_a(slot)
                        .with_span(expr.span),
                );
            }
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
            ExprKind::Call { callee, args, .. } => {
                self.emit_call(expr, callee, args, expr_info, user_calls);
            }
            ExprKind::Index { target, index } => {
                let required_history = literal_window(index).unwrap_or_default() + 1;
                self.emit_series_ref(target, required_history.max(2), expr_info, user_calls);
                let offset = literal_window(index).unwrap_or_default() as u16;
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

            let mut scope = HashMap::new();
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
            | BuiltinId::BarsSince
            | BuiltinId::ValueWhen
            | BuiltinId::Cross
            | BuiltinId::Crossover
            | BuiltinId::Crossunder
            | BuiltinId::Change
            | BuiltinId::Roc
            | BuiltinId::Ma
            | BuiltinId::Macd
            | BuiltinId::Obv
            | BuiltinId::Trange
            | BuiltinId::Wma
            | BuiltinId::Avgdev
            | BuiltinId::MaxIndex
            | BuiltinId::MinIndex
            | BuiltinId::MinMax
            | BuiltinId::MinMaxIndex => {
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
                let required_history = literal_window(&args[1])
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
            BuiltinKind::RollingSingleInput => {
                let default_window = match builtin {
                    BuiltinId::Max | BuiltinId::Min | BuiltinId::Sum => 30,
                    BuiltinId::Midpoint => 14,
                    BuiltinId::Wma | BuiltinId::MaxIndex | BuiltinId::MinIndex => 30,
                    BuiltinId::Avgdev => 14,
                    _ => unreachable!(),
                };
                let required_history = args
                    .get(1)
                    .and_then(literal_window)
                    .unwrap_or(default_window);
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                if let Some(window) = args.get(1) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(default_window as f64, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(2)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingSingleInputTuple => {
                let default_window = match builtin {
                    BuiltinId::MinMax | BuiltinId::MinMaxIndex => 30,
                    _ => unreachable!(),
                };
                let required_history = args
                    .get(1)
                    .and_then(literal_window)
                    .unwrap_or(default_window);
                self.emit_series_ref(&args[0], required_history.max(2), expr_info, user_calls);
                if let Some(window) = args.get(1) {
                    self.emit_expr(window, expr_info, user_calls);
                } else {
                    self.emit_f64_constant(default_window as f64, expr.span);
                }
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(2)
                        .with_c(callsite)
                        .with_span(expr.span),
                );
            }
            BuiltinKind::RollingHighLow => {
                let required_history = args.get(2).and_then(literal_window).unwrap_or(14);
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
            BuiltinKind::MovingAverage => {
                let required_history = literal_window(&args[1]).unwrap_or(2);
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
            BuiltinKind::IndicatorTuple => {
                let required_history = literal_window(&args[2]).unwrap_or(2) + 1;
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
            BuiltinKind::Rising | BuiltinKind::Falling => {
                let required_history = literal_window(&args[1])
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
            BuiltinKind::Change | BuiltinKind::Roc => {
                let required_history = literal_window(&args[1])
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

    fn emit_series_window_alias_call(
        &mut self,
        builtin: BuiltinId,
        series: &Expr,
        window: Option<&Expr>,
        default_window: usize,
        expr_info: &HashMap<NodeId, ExprInfo>,
        user_calls: &HashMap<NodeId, FunctionSpecializationKey>,
    ) {
        let required_history = window.and_then(literal_window).unwrap_or(default_window);
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
        let required_history = window.and_then(literal_window).unwrap_or(default_window);
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
            ExprKind::QualifiedSeries { interval, field } => {
                let slot = self.qualified_slot(*interval, *field);
                self.bump_slot_history(slot, required_history);
                self.emit(
                    Instruction::new(OpCode::LoadSeries)
                        .with_a(slot)
                        .with_span(expr.span),
                );
            }
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

    fn qualified_slot(&self, interval: Interval, field: MarketField) -> u16 {
        self.analysis.qualified_slots[&(interval, field)]
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
        if self.analysis.declared_sources.is_empty() {
            for (slot, (name, _field)) in PREDEFINED_SERIES.into_iter().enumerate() {
                root.insert(
                    name.to_string(),
                    CompilerSymbol {
                        slot: slot as u16,
                        ty: Type::SeriesF64,
                    },
                );
            }
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
