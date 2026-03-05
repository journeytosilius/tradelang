//! Source-to-bytecode compilation for TradeLang programs.
//!
//! This module drives lexing and parsing, performs semantic analysis and type
//! inference, resolves locals and builtins, and emits deterministic bytecode.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::ast::{
    Ast, BinaryOp, Block, Expr, ExprKind, FunctionDecl, NodeId, Stmt, StmtKind, UnaryOp,
};
use crate::builtins::BuiltinId;
use crate::bytecode::{Constant, Instruction, LocalInfo, OpCode, Program};
use crate::diagnostic::{CompileError, Diagnostic, DiagnosticKind};
use crate::interval::{Interval, MarketBinding, MarketField, MarketSource};
use crate::lexer;
use crate::parser;
use crate::span::Span;
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
enum InferredType {
    Concrete(Type),
    Na,
}

impl InferredType {
    fn concrete(self) -> Option<Type> {
        match self {
            Self::Concrete(ty) => Some(ty),
            Self::Na => None,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ExprInfo {
    ty: InferredType,
    update_mask: u32,
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
    expr_info: HashMap<NodeId, ExprInfo>,
    user_function_calls: HashMap<NodeId, FunctionSpecializationKey>,
    resolved_let_slots: HashMap<NodeId, u16>,
    locals: Vec<LocalInfo>,
    qualified_slots: HashMap<(Interval, MarketField), u16>,
    function_specializations: HashMap<FunctionSpecializationKey, FunctionSpecialization>,
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

    fn collect_functions(&mut self, ast: &'a Ast) {
        for function in &ast.functions {
            if BuiltinId::from_name(&function.name).is_some() {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("function name `{}` collides with a builtin", function.name),
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
                if !params.contains(name.as_str()) && !is_predefined_series_name(name) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "function bodies may only reference parameters or predefined series; found `{name}`"
                        ),
                        expr.span,
                    ));
                }
            }
            ExprKind::QualifiedSeries { .. } => {}
            ExprKind::Unary { expr, .. } => self.validate_function_expr(expr, params),
            ExprKind::Binary { left, right, .. } => {
                self.validate_function_expr(left, params);
                self.validate_function_expr(right, params);
            }
            ExprKind::Call { callee, args } => {
                match BuiltinId::from_name(callee) {
                    Some(BuiltinId::Plot) => {
                        self.diagnostics.push(Diagnostic::new(
                            DiagnosticKind::Type,
                            "function bodies may not call `plot`",
                            expr.span,
                        ));
                    }
                    Some(BuiltinId::Sma | BuiltinId::Ema | BuiltinId::Rsi) => {
                        if args.len() != 2 {
                            self.diagnostics.push(Diagnostic::new(
                                DiagnosticKind::Type,
                                format!("{callee} expects exactly two arguments"),
                                expr.span,
                            ));
                        }
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
            ExprKind::Number(_) | ExprKind::Bool(_) | ExprKind::Na => {}
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
            StmtKind::Let { name, expr } => {
                let expr_info = self.analyze_expr(expr);
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
            ExprKind::Ident(name) => {
                let Some(symbol) = self.lookup_symbol(name) else {
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
            ExprKind::Unary { op, expr: inner } => self.analyze_unary(*op, inner),
            ExprKind::Binary { op, left, right } => self.analyze_binary(*op, left, right),
            ExprKind::Call { callee, args } => self.analyze_call(expr, callee, args),
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
            BuiltinId::Sma | BuiltinId::Ema | BuiltinId::Rsi => {
                if args.len() != 2 {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} expects exactly two arguments"),
                        span,
                    ));
                    return ExprInfo::series(0);
                }
                let series_info = self.analyze_expr(&args[0]);
                if !matches!(series_info.ty, InferredType::Concrete(Type::SeriesF64)) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} requires series<float> as the first argument"),
                        args[0].span,
                    ));
                }
                match literal_window(&args[1]) {
                    Some(window) if window > 0 => {}
                    Some(_) => self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} length must be greater than zero"),
                        args[1].span,
                    )),
                    None => self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} length must be a non-negative integer literal"),
                        args[1].span,
                    )),
                }
                ExprInfo {
                    ty: InferredType::Concrete(Type::SeriesF64),
                    update_mask: series_info.update_mask,
                }
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
        for (name, _) in PREDEFINED_SERIES {
            root.insert(
                name.to_string(),
                AnalyzerSymbol {
                    info: ExprInfo::series(BASE_UPDATE_MASK),
                },
            );
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
            ExprKind::Ident(name) => match self.lookup_symbol(name) {
                Some(symbol) => symbol.info,
                None => {
                    self.parent.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!(
                            "function bodies may only reference parameters or predefined series; found `{name}`"
                        ),
                        expr.span,
                    ));
                    ExprInfo::scalar(Type::F64)
                }
            },
            ExprKind::QualifiedSeries { interval, .. } => ExprInfo::series(interval.mask()),
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
            ExprKind::Call { callee, args } => self.analyze_call(expr, callee, args),
            ExprKind::Index { target, index } => self.analyze_index(target, index, expr.span),
        };
        self.expr_info.insert(expr.id, info);
        info
    }

    fn analyze_call(&mut self, expr: &Expr, callee: &str, args: &[Expr]) -> ExprInfo {
        if let Some(builtin) = BuiltinId::from_name(callee) {
            return self.analyze_builtin_call(builtin, callee, args, expr.span);
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
            BuiltinId::Sma | BuiltinId::Ema | BuiltinId::Rsi => {
                if args.len() != 2 {
                    self.parent.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} expects exactly two arguments"),
                        span,
                    ));
                    return ExprInfo::series(0);
                }
                let series_info = self.analyze_expr(&args[0]);
                if !matches!(series_info.ty, InferredType::Concrete(Type::SeriesF64)) {
                    self.parent.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} requires series<float> as the first argument"),
                        args[0].span,
                    ));
                }
                match literal_window(&args[1]) {
                    Some(window) if window > 0 => {}
                    Some(_) => self.parent.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} length must be greater than zero"),
                        args[1].span,
                    )),
                    None => self.parent.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} length must be a non-negative integer literal"),
                        args[1].span,
                    )),
                }
                ExprInfo {
                    ty: InferredType::Concrete(Type::SeriesF64),
                    update_mask: series_info.update_mask,
                }
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
        StmtKind::Let { expr, .. } | StmtKind::Expr(expr) => {
            collect_qualified_series_refs(expr, refs)
        }
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
        ExprKind::Number(_) | ExprKind::Bool(_) | ExprKind::Na | ExprKind::Ident(_) => {}
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
        ExprKind::Call { callee, args } => {
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
        | ExprKind::Ident(_)
        | ExprKind::QualifiedSeries { .. } => {}
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

fn literal_window(expr: &Expr) -> Option<usize> {
    match expr.kind {
        ExprKind::Number(value) if value >= 0.0 && value.fract() == 0.0 => Some(value as usize),
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
        InferredType::Na => FunctionParamBinding {
            ty: Type::SeriesF64,
            kind: SlotKind::Series,
            update_mask: arg_shape.update_mask,
        },
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
            StmtKind::Let { name, expr } => {
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
            ExprKind::Call { callee, args } => {
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
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Compile,
                format!("unknown builtin `{callee}`"),
                expr.span,
            ));
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
            BuiltinId::Sma | BuiltinId::Ema | BuiltinId::Rsi => {
                let required_history = literal_window(&args[1])
                    .map(|window| window + 1)
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
            _ => {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Compile,
                    format!("builtin `{callee}` is not callable in v0.1"),
                    expr.span,
                ));
            }
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

    fn next_callsite(&mut self) -> u16 {
        let callsite = self.builtin_call_count;
        self.builtin_call_count += 1;
        callsite
    }

    fn rebuild_scopes(&mut self) {
        let mut root = HashMap::new();
        for (slot, (name, _field)) in PREDEFINED_SERIES.into_iter().enumerate() {
            root.insert(
                name.to_string(),
                CompilerSymbol {
                    slot: slot as u16,
                    ty: Type::SeriesF64,
                },
            );
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
