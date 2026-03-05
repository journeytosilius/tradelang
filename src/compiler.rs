//! Source-to-bytecode compilation for TradeLang programs.
//!
//! This module drives lexing and parsing, performs semantic analysis and type
//! inference, resolves locals and builtins, and emits deterministic bytecode.

use std::collections::HashMap;

use crate::ast::{Ast, BinaryOp, Block, Expr, ExprKind, NodeId, Stmt, StmtKind, UnaryOp};
use crate::builtins::BuiltinId;
use crate::bytecode::{Constant, Instruction, LocalInfo, OpCode, Program};
use crate::diagnostic::{CompileError, Diagnostic, DiagnosticKind};
use crate::lexer;
use crate::parser;
use crate::span::Span;
use crate::types::{SlotKind, Type, Value};

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

#[derive(Clone, Copy, Debug)]
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

#[derive(Clone, Copy, Debug)]
struct Symbol {
    slot: u16,
    ty: Type,
}

#[derive(Default)]
struct Analysis {
    expr_types: HashMap<NodeId, InferredType>,
    resolved_expr_slots: HashMap<NodeId, u16>,
    resolved_let_slots: HashMap<NodeId, u16>,
    locals: Vec<LocalInfo>,
    history_capacity: usize,
}

struct Analyzer<'a> {
    diagnostics: Vec<Diagnostic>,
    scopes: Vec<HashMap<String, Symbol>>,
    analysis: Analysis,
    _source: &'a str,
}

impl<'a> Analyzer<'a> {
    fn new(source: &'a str) -> Self {
        let mut analyzer = Self {
            diagnostics: Vec::new(),
            scopes: vec![HashMap::new()],
            analysis: Analysis {
                history_capacity: 2,
                ..Analysis::default()
            },
            _source: source,
        };

        for (name, ty) in [
            ("open", Type::SeriesF64),
            ("high", Type::SeriesF64),
            ("low", Type::SeriesF64),
            ("close", Type::SeriesF64),
            ("volume", Type::SeriesF64),
            ("time", Type::SeriesF64),
        ] {
            analyzer.define_symbol(name.to_string(), ty, true);
        }

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

    fn analyze_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, expr } => {
                let expr_ty = self.analyze_expr(expr);
                let concrete = match expr_ty {
                    InferredType::Concrete(ty) => ty,
                    InferredType::Na => Type::F64,
                };
                if self.scopes.last().unwrap().contains_key(name) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("duplicate binding `{name}` in the same scope"),
                        stmt.span,
                    ));
                    return;
                }
                let slot = self.define_symbol(name.clone(), concrete, false);
                self.analysis.resolved_let_slots.insert(stmt.id, slot);
            }
            StmtKind::If {
                condition,
                then_block,
                else_block,
            } => {
                let ty = self.analyze_expr(condition);
                if !ty.allow_bool() {
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

    fn analyze_expr(&mut self, expr: &Expr) -> InferredType {
        let inferred = match &expr.kind {
            ExprKind::Number(_) => InferredType::Concrete(Type::F64),
            ExprKind::Bool(_) => InferredType::Concrete(Type::Bool),
            ExprKind::Na => InferredType::Na,
            ExprKind::Ident(name) => {
                let Some(symbol) = self.lookup_symbol(name) else {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("unknown identifier `{name}`"),
                        expr.span,
                    ));
                    return InferredType::Concrete(Type::F64);
                };
                self.analysis
                    .resolved_expr_slots
                    .insert(expr.id, symbol.slot);
                InferredType::Concrete(symbol.ty)
            }
            ExprKind::Unary { op, expr: inner } => self.analyze_unary(*op, inner),
            ExprKind::Binary { op, left, right } => self.analyze_binary(*op, left, right),
            ExprKind::Call { callee, args } => self.analyze_call(callee, args, expr.span),
            ExprKind::Index { target, index } => self.analyze_index(target, index, expr.span),
        };
        self.analysis.expr_types.insert(expr.id, inferred);
        inferred
    }

    fn analyze_unary(&mut self, op: UnaryOp, inner: &Expr) -> InferredType {
        let inner_ty = self.analyze_expr(inner);
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
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "unary `-` requires numeric input",
                        inner.span,
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
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "unary `!` requires bool input",
                        inner.span,
                    ));
                    InferredType::Concrete(Type::Bool)
                }
            }
        }
    }

    fn analyze_binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr) -> InferredType {
        let left_ty = self.analyze_expr(left);
        let right_ty = self.analyze_expr(right);
        match op {
            BinaryOp::And | BinaryOp::Or => {
                if !(left_ty.allow_bool() && right_ty.allow_bool()) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "logical operators require bool, series<bool>, or na operands",
                        left.span.merge(right.span),
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
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "arithmetic operators require numeric operands",
                        left.span.merge(right.span),
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
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "comparison operators require numeric operands",
                        left.span.merge(right.span),
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

    fn analyze_call(&mut self, callee: &str, args: &[Expr], span: Span) -> InferredType {
        match BuiltinId::from_name(callee) {
            Some(BuiltinId::Plot) => {
                if args.len() != 1 {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "plot expects exactly one argument",
                        span,
                    ));
                    return InferredType::Concrete(Type::Void);
                }
                let arg_ty = self.analyze_expr(&args[0]);
                if !matches!(
                    arg_ty,
                    InferredType::Concrete(Type::F64 | Type::SeriesF64) | InferredType::Na
                ) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        "plot expects a numeric or series numeric value",
                        args[0].span,
                    ));
                }
                InferredType::Concrete(Type::Void)
            }
            Some(BuiltinId::Sma | BuiltinId::Ema | BuiltinId::Rsi) => {
                if args.len() != 2 {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} expects exactly two arguments"),
                        span,
                    ));
                    return InferredType::Concrete(Type::SeriesF64);
                }
                let series_ty = self.analyze_expr(&args[0]);
                if !matches!(series_ty, InferredType::Concrete(Type::SeriesF64)) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Type,
                        format!("{callee} requires series<float> as the first argument"),
                        args[0].span,
                    ));
                }
                match literal_window(&args[1]) {
                    Some(window) if window > 0 => {
                        self.analysis.history_capacity =
                            self.analysis.history_capacity.max(window + 1);
                    }
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
                InferredType::Concrete(Type::SeriesF64)
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
                    span,
                ));
                InferredType::Concrete(Type::SeriesF64)
            }
            None => {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Type,
                    format!("unknown function `{callee}`"),
                    span,
                ));
                InferredType::Concrete(Type::F64)
            }
        }
    }

    fn analyze_index(&mut self, target: &Expr, index: &Expr, span: Span) -> InferredType {
        let target_ty = self.analyze_expr(target);
        let offset = literal_window(index);
        let Some(offset) = offset else {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Type,
                "series indexing requires a non-negative integer literal",
                index.span,
            ));
            return InferredType::Concrete(Type::F64);
        };
        self.analysis.history_capacity = self.analysis.history_capacity.max(offset + 1);
        match target_ty {
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
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_symbol(&mut self, name: String, ty: Type, hidden: bool) -> u16 {
        let slot = self.analysis.locals.len() as u16;
        let kind = if ty.is_series() {
            SlotKind::Series
        } else {
            SlotKind::Scalar
        };
        self.analysis.locals.push(LocalInfo {
            name: if hidden { None } else { Some(name.clone()) },
            ty,
            kind,
            hidden,
        });
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name, Symbol { slot, ty });
        slot
    }

    fn lookup_symbol(&self, name: &str) -> Option<Symbol> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}

fn literal_window(expr: &Expr) -> Option<usize> {
    match expr.kind {
        ExprKind::Number(value) if value >= 0.0 && value.fract() == 0.0 => Some(value as usize),
        _ => None,
    }
}

struct Compiler<'a> {
    source: &'a str,
    ast: &'a Ast,
    analysis: Analysis,
    program: Program,
    diagnostics: Vec<Diagnostic>,
    builtin_call_count: u16,
    scopes: Vec<HashMap<String, Symbol>>,
}

impl<'a> Compiler<'a> {
    fn new(source: &'a str, ast: &'a Ast) -> Self {
        Self {
            source,
            ast,
            analysis: Analysis::default(),
            program: Program::default(),
            diagnostics: Vec::new(),
            builtin_call_count: 0,
            scopes: Vec::new(),
        }
    }

    fn compile(mut self) -> Result<CompiledProgram, CompileError> {
        self.analysis = Analyzer::new(self.source).analyze(self.ast)?;
        self.program.locals = self.analysis.locals.clone();
        self.program.history_capacity = self.analysis.history_capacity.max(2);
        self.rebuild_scopes();
        for stmt in &self.ast.statements {
            self.emit_stmt(stmt);
        }
        self.program
            .instructions
            .push(Instruction::new(OpCode::Return));
        if self.diagnostics.is_empty() {
            Ok(CompiledProgram {
                program: self.program,
                source: self.source.to_string(),
            })
        } else {
            Err(CompileError::new(self.diagnostics))
        }
    }

    fn emit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let { name, expr } => {
                self.emit_expr(expr);
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
                    .insert(name.clone(), Symbol { slot, ty: local.ty });
            }
            StmtKind::If {
                condition,
                then_block,
                else_block,
            } => {
                self.emit_expr(condition);
                let jump_if_false = self.emit_placeholder(OpCode::JumpIfFalse, condition.span);
                self.push_scope();
                self.emit_block(then_block);
                self.pop_scope();
                let jump_over_else = self.emit_placeholder(OpCode::Jump, stmt.span);
                self.patch_jump(jump_if_false, self.program.instructions.len());
                self.push_scope();
                self.emit_block(else_block);
                self.pop_scope();
                self.patch_jump(jump_over_else, self.program.instructions.len());
            }
            StmtKind::Expr(expr) => {
                self.emit_expr(expr);
                if self.expr_type(expr).concrete() != Some(Type::Void) {
                    self.emit(Instruction::new(OpCode::Pop).with_span(stmt.span));
                }
            }
        }
    }

    fn emit_block(&mut self, block: &Block) {
        for stmt in &block.statements {
            self.emit_stmt(stmt);
        }
    }

    fn emit_expr(&mut self, expr: &Expr) {
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
            ExprKind::Ident(_) => {
                let slot = self.analysis.resolved_expr_slots[&expr.id];
                self.emit(
                    Instruction::new(OpCode::LoadLocal)
                        .with_a(slot)
                        .with_span(expr.span),
                );
            }
            ExprKind::Unary { op, expr: inner } => {
                self.emit_expr(inner);
                let opcode = match op {
                    UnaryOp::Neg => OpCode::Neg,
                    UnaryOp::Not => OpCode::Not,
                };
                self.emit(Instruction::new(opcode).with_span(expr.span));
            }
            ExprKind::Binary { op, left, right } => {
                self.emit_expr(left);
                self.emit_expr(right);
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
            ExprKind::Call { callee, args } => self.emit_call(expr, callee, args),
            ExprKind::Index { target, index } => {
                self.emit_series_ref(target);
                let offset = literal_window(index).unwrap_or_default() as u16;
                self.emit(
                    Instruction::new(OpCode::SeriesGet)
                        .with_a(offset)
                        .with_span(expr.span),
                );
            }
        }
    }

    fn emit_call(&mut self, expr: &Expr, callee: &str, args: &[Expr]) {
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
                self.emit_expr(&args[0]);
                self.emit(
                    Instruction::new(OpCode::CallBuiltin)
                        .with_a(builtin as u16)
                        .with_b(1)
                        .with_c(0)
                        .with_span(expr.span),
                );
                self.program.plot_count = 1;
            }
            BuiltinId::Sma | BuiltinId::Ema | BuiltinId::Rsi => {
                self.emit_series_ref(&args[0]);
                self.emit_expr(&args[1]);
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

    fn emit_series_ref(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Ident(_) if self.expr_type(expr).concrete().is_some_and(Type::is_series) => {
                let slot = self.analysis.resolved_expr_slots[&expr.id];
                self.emit(
                    Instruction::new(OpCode::LoadSeries)
                        .with_a(slot)
                        .with_span(expr.span),
                );
            }
            _ => {
                let temp_slot = self.allocate_hidden_series_slot(Type::SeriesF64);
                self.emit_expr(expr);
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
        }
    }

    fn expr_type(&self, expr: &Expr) -> InferredType {
        self.analysis.expr_types[&expr.id]
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

    fn allocate_hidden_series_slot(&mut self, ty: Type) -> u16 {
        let slot = self.program.locals.len() as u16;
        self.program.locals.push(LocalInfo {
            name: None,
            ty,
            kind: SlotKind::Series,
            hidden: true,
        });
        slot
    }

    fn next_callsite(&mut self) -> u16 {
        let callsite = self.builtin_call_count;
        self.builtin_call_count += 1;
        callsite
    }

    fn rebuild_scopes(&mut self) {
        let mut root = HashMap::new();
        for (slot, local) in self.program.locals.iter().enumerate() {
            if let Some(name) = &local.name {
                root.insert(
                    name.clone(),
                    Symbol {
                        slot: slot as u16,
                        ty: local.ty,
                    },
                );
            }
        }
        self.scopes = vec![root];
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}
