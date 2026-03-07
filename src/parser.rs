//! Parsing from tokens into the PalmScript AST.
//!
//! The parser builds typed source-level nodes, preserves spans, and accumulates
//! parse diagnostics instead of emitting bytecode directly.

use crate::ast::{
    Ast, BinaryOp, BindingName, Block, Expr, ExprKind, FunctionDecl, FunctionParam, IntervalDecl,
    NodeId, SignalRole, SourceDecl, SourceIntervalDecl, Stmt, StmtKind, StrategyIntervals, UnaryOp,
};
use crate::diagnostic::{CompileError, Diagnostic, DiagnosticKind};
use crate::span::Span;
use crate::token::{Token, TokenKind};
use crate::{MarketField, SourceTemplate};

pub fn parse(tokens: &[Token]) -> Result<Ast, CompileError> {
    Parser::new(tokens).parse()
}

struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
    next_node_id: NodeId,
    diagnostics: Vec<Diagnostic>,
    block_depth: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            cursor: 0,
            next_node_id: 1,
            diagnostics: Vec::new(),
            block_depth: 0,
        }
    }

    fn parse(mut self) -> Result<Ast, CompileError> {
        let mut strategy_intervals = StrategyIntervals::default();
        let mut functions = Vec::new();
        let mut statements = Vec::new();
        self.skip_separators();
        while !self.is_eof() {
            match self.parse_item() {
                Some(ParsedItem::BaseInterval(decl)) => strategy_intervals.base.push(decl),
                Some(ParsedItem::Source(decl)) => strategy_intervals.sources.push(decl),
                Some(ParsedItem::UseInterval(decl)) => strategy_intervals.supplemental.push(decl),
                Some(ParsedItem::Function(function)) => functions.push(function),
                Some(ParsedItem::Stmt(stmt)) => statements.push(stmt),
                None => self.synchronize(),
            }
            self.skip_separators();
        }

        if self.diagnostics.is_empty() {
            Ok(Ast {
                strategy_intervals,
                functions,
                statements,
            })
        } else {
            Err(CompileError::new(self.diagnostics))
        }
    }

    fn parse_item(&mut self) -> Option<ParsedItem> {
        if self.matches_keyword(&TokenKind::IntervalKw) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "interval directives are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self
                .parse_base_interval_decl()
                .map(ParsedItem::BaseInterval);
        }
        if self.matches_keyword(&TokenKind::Source) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "interval directives are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_source_decl().map(ParsedItem::Source);
        }
        if self.matches_keyword(&TokenKind::Use) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "interval directives are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_use_interval_decl().map(ParsedItem::UseInterval);
        }
        if self.matches_keyword(&TokenKind::Fn) {
            return self.parse_function_decl().map(ParsedItem::Function);
        }
        self.parse_stmt().map(ParsedItem::Stmt)
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        if self.matches_keyword(&TokenKind::Fn) {
            self.push_diagnostic(
                "function declarations are only allowed at the top level",
                self.previous().span,
            );
            return None;
        }
        if self.matches_keyword(&TokenKind::IntervalKw)
            || self.matches_keyword(&TokenKind::Source)
            || self.matches_keyword(&TokenKind::Use)
        {
            self.push_diagnostic(
                "interval directives are only allowed at the top level",
                self.previous().span,
            );
            return None;
        }
        if self.matches_keyword(&TokenKind::Export) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "export statements are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_output_stmt(true);
        }
        if self.matches_keyword(&TokenKind::Trigger) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "trigger statements are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_output_stmt(false);
        }
        if self.matches_keyword(&TokenKind::Entry) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "signal declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_signal_stmt(true);
        }
        if self.matches_keyword(&TokenKind::Exit) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "signal declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_signal_stmt(false);
        }
        if self.matches_keyword(&TokenKind::Const) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "`const` declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_binding_stmt(true);
        }
        if self.matches_keyword(&TokenKind::Input) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "`input` declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_binding_stmt(false);
        }
        if self.matches_keyword(&TokenKind::Let) {
            return self.parse_let_stmt();
        }
        if self.matches_keyword(&TokenKind::If) {
            return self.parse_if_stmt();
        }

        let expr = self.parse_expr(0)?;
        let span = expr.span;
        Some(Stmt {
            id: self.alloc_id(),
            span,
            kind: StmtKind::Expr(expr),
        })
    }

    fn parse_function_decl(&mut self) -> Option<FunctionDecl> {
        let start = self.previous().span;
        let (name, name_span) = self.expect_ident("expected identifier after `fn`")?;
        self.expect_kind(
            |kind| matches!(kind, TokenKind::LeftParen),
            "expected `(` after function name",
        )?;

        let mut params = Vec::new();
        if !matches!(self.peek_kind(), TokenKind::RightParen) {
            loop {
                let (name, span) = self.expect_ident("expected parameter name")?;
                params.push(FunctionParam { name, span });
                if !matches!(self.peek_kind(), TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
        }

        self.expect_kind(
            |kind| matches!(kind, TokenKind::RightParen),
            "expected `)` after parameters",
        )?;
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Assign),
            "expected `=` after function signature",
        )?;
        let body = self.parse_expr(0)?;
        Some(FunctionDecl {
            id: self.alloc_id(),
            name,
            name_span,
            params,
            span: start.merge(body.span),
            body,
        })
    }

    fn parse_let_stmt(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        if matches!(self.peek_kind(), TokenKind::LeftParen) {
            return self.parse_let_tuple_stmt(start);
        }
        let (name, name_span) = self.expect_ident("expected identifier after `let`")?;
        self.expect_assign()?;
        let expr = self.parse_expr(0)?;
        let span = start.merge(expr.span);
        Some(Stmt {
            id: self.alloc_id(),
            span,
            kind: StmtKind::Let {
                name,
                name_span,
                expr,
            },
        })
    }

    fn parse_binding_stmt(&mut self, is_const: bool) -> Option<Stmt> {
        let start = self.previous().span;
        let (name, name_span) = self.expect_ident(if is_const {
            "expected identifier after `const`"
        } else {
            "expected identifier after `input`"
        })?;
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Assign),
            if is_const {
                "expected `=` after const name"
            } else {
                "expected `=` after input name"
            },
        )?;
        let expr = self.parse_expr(0)?;
        let span = start.merge(expr.span);
        Some(Stmt {
            id: self.alloc_id(),
            span,
            kind: if is_const {
                StmtKind::Const {
                    name,
                    name_span,
                    expr,
                }
            } else {
                StmtKind::Input {
                    name,
                    name_span,
                    expr,
                }
            },
        })
    }

    fn parse_let_tuple_stmt(&mut self, start: Span) -> Option<Stmt> {
        self.expect_kind(
            |kind| matches!(kind, TokenKind::LeftParen),
            "expected `(` after `let`",
        )?;
        let mut names = Vec::new();
        loop {
            let (name, span) = self.expect_ident("expected identifier in tuple binding")?;
            names.push(BindingName { name, span });
            if !matches!(self.peek_kind(), TokenKind::Comma) {
                break;
            }
            self.advance();
        }
        let right = self.expect_kind(
            |kind| matches!(kind, TokenKind::RightParen),
            "expected `)` after tuple binding",
        )?;
        self.expect_assign()?;
        let expr = self.parse_expr(0)?;
        Some(Stmt {
            id: self.alloc_id(),
            span: start.merge(expr.span).merge(right.span),
            kind: StmtKind::LetTuple { names, expr },
        })
    }

    fn parse_base_interval_decl(&mut self) -> Option<IntervalDecl> {
        let start = self.previous().span;
        let token = self.expect_kind(
            |kind| matches!(kind, TokenKind::Interval(_)),
            "expected interval literal after `interval`",
        )?;
        let TokenKind::Interval(interval) = token.kind else {
            unreachable!();
        };
        Some(IntervalDecl {
            interval,
            span: start.merge(token.span),
        })
    }

    fn parse_source_decl(&mut self) -> Option<SourceDecl> {
        let start = self.previous().span;
        let (alias, alias_span) = self.expect_ident("expected identifier after `source`")?;
        self.expect_assign()?;
        let (exchange, exchange_span) = self.expect_ident("expected exchange name after `=`")?;
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Dot),
            "expected `.` after exchange name",
        )?;
        let (venue, venue_span) = self.expect_ident("expected venue name after `.`")?;
        let template_span = exchange_span.merge(venue_span);
        let Some(template) = SourceTemplate::parse(&exchange, &venue) else {
            self.push_diagnostic("unsupported source template", template_span);
            return None;
        };
        self.expect_kind(
            |kind| matches!(kind, TokenKind::LeftParen),
            "expected `(` after source template",
        )?;
        let (symbol, symbol_span) = self.expect_string("expected string literal source symbol")?;
        let right = self.expect_kind(
            |kind| matches!(kind, TokenKind::RightParen),
            "expected `)` after source symbol",
        )?;
        Some(SourceDecl {
            alias,
            alias_span,
            template,
            template_span,
            symbol,
            symbol_span,
            span: start.merge(right.span),
        })
    }

    fn parse_use_interval_decl(&mut self) -> Option<SourceIntervalDecl> {
        let start = self.previous().span;
        let (source, source_span) = self.expect_ident("expected source alias after `use`")?;
        let token = self.expect_kind(
            |kind| matches!(kind, TokenKind::Interval(_)),
            "expected interval literal after source alias",
        )?;
        let TokenKind::Interval(interval) = token.kind else {
            unreachable!();
        };
        Some(SourceIntervalDecl {
            source,
            source_span,
            interval,
            span: start.merge(token.span),
        })
    }

    fn parse_output_stmt(&mut self, export: bool) -> Option<Stmt> {
        let start = self.previous().span;
        let (name, name_span) = self.expect_ident(if export {
            "expected identifier after `export`"
        } else {
            "expected identifier after `trigger`"
        })?;
        self.expect_assign()?;
        let expr = self.parse_expr(0)?;
        let span = start.merge(expr.span);
        Some(Stmt {
            id: self.alloc_id(),
            span,
            kind: if export {
                StmtKind::Export {
                    name,
                    name_span,
                    expr,
                }
            } else {
                StmtKind::Trigger {
                    name,
                    name_span,
                    expr,
                }
            },
        })
    }

    fn parse_signal_stmt(&mut self, entry: bool) -> Option<Stmt> {
        let start = self.previous().span;
        let side = self.advance()?.clone();
        let role = match side.kind {
            TokenKind::Long if entry => SignalRole::LongEntry,
            TokenKind::Long => SignalRole::LongExit,
            TokenKind::Short if entry => SignalRole::ShortEntry,
            TokenKind::Short => SignalRole::ShortExit,
            _ => {
                self.push_diagnostic(
                    if entry {
                        "expected `long` or `short` after `entry`"
                    } else {
                        "expected `long` or `short` after `exit`"
                    },
                    side.span,
                );
                return None;
            }
        };
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Assign),
            "expected `=` after signal side",
        )?;
        let expr = self.parse_expr(0)?;
        Some(Stmt {
            id: self.alloc_id(),
            span: start.merge(expr.span),
            kind: StmtKind::Signal { role, expr },
        })
    }

    fn parse_if_stmt(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        let condition = self.parse_expr(0)?;
        let then_block = self.parse_block()?;
        if !self.matches_keyword(&TokenKind::Else) {
            self.push_diagnostic("expected `else` after `if` block", then_block.span);
            return None;
        }
        let else_block = self.parse_else_block()?;
        let span = start.merge(else_block.span);
        Some(Stmt {
            id: self.alloc_id(),
            span,
            kind: StmtKind::If {
                condition,
                then_block,
                else_block,
            },
        })
    }

    fn parse_else_block(&mut self) -> Option<Block> {
        self.skip_separators();
        if self.matches_keyword(&TokenKind::If) {
            let nested_if = self.parse_if_stmt()?;
            return Some(Block {
                span: nested_if.span,
                statements: vec![nested_if],
            });
        }
        self.parse_block()
    }

    fn parse_block(&mut self) -> Option<Block> {
        let left = self.expect_kind(
            |kind| matches!(kind, TokenKind::LeftBrace),
            "expected `{` to start block",
        )?;
        let mut statements = Vec::new();
        self.block_depth += 1;
        self.skip_separators();
        while !matches!(self.peek_kind(), TokenKind::RightBrace | TokenKind::Eof) {
            match self.parse_stmt() {
                Some(stmt) => statements.push(stmt),
                None => self.synchronize(),
            }
            self.skip_separators();
        }
        self.block_depth -= 1;
        let right = self.expect_kind(
            |kind| matches!(kind, TokenKind::RightBrace),
            "expected `}` to end block",
        )?;
        Some(Block {
            statements,
            span: left.span.merge(right.span),
        })
    }

    fn parse_expr(&mut self, min_bp: u8) -> Option<Expr> {
        let mut lhs = self.parse_prefix()?;

        loop {
            lhs = match self.peek_kind() {
                TokenKind::LeftParen => self.parse_call(lhs)?,
                TokenKind::LeftBracket => self.parse_index(lhs)?,
                TokenKind::Dot if matches!(lhs.kind, ExprKind::Ident(_)) => {
                    self.parse_dotted_ident(lhs)?
                }
                TokenKind::Question => {
                    let (left_bp, right_bp) = (4, 4);
                    if left_bp < min_bp {
                        break;
                    }
                    self.advance();
                    let when_true = self.parse_expr(0)?;
                    self.expect_kind(
                        |kind| matches!(kind, TokenKind::Colon),
                        "expected `:` in conditional expression",
                    )?;
                    let when_false = self.parse_expr(right_bp)?;
                    let span = lhs.span.merge(when_false.span);
                    Expr {
                        id: self.alloc_id(),
                        span,
                        kind: ExprKind::Conditional {
                            condition: Box::new(lhs),
                            when_true: Box::new(when_true),
                            when_false: Box::new(when_false),
                        },
                    }
                }
                _ => {
                    let Some((left_bp, right_bp, op)) = self.infix_binding_power() else {
                        break;
                    };
                    if left_bp < min_bp {
                        break;
                    }
                    self.advance();
                    let rhs = self.parse_expr(right_bp)?;
                    let span = lhs.span.merge(rhs.span);
                    Expr {
                        id: self.alloc_id(),
                        span,
                        kind: ExprKind::Binary {
                            op,
                            left: Box::new(lhs),
                            right: Box::new(rhs),
                        },
                    }
                }
            };
        }

        Some(lhs)
    }

    fn parse_prefix(&mut self) -> Option<Expr> {
        let token = self.advance()?.clone();
        match token.kind {
            TokenKind::Number(text) => {
                let value = text.parse::<f64>().ok()?;
                Some(Expr {
                    id: self.alloc_id(),
                    span: token.span,
                    kind: ExprKind::Number(value),
                })
            }
            TokenKind::True => Some(Expr {
                id: self.alloc_id(),
                span: token.span,
                kind: ExprKind::Bool(true),
            }),
            TokenKind::False => Some(Expr {
                id: self.alloc_id(),
                span: token.span,
                kind: ExprKind::Bool(false),
            }),
            TokenKind::Na => Some(Expr {
                id: self.alloc_id(),
                span: token.span,
                kind: if matches!(self.peek_kind(), TokenKind::LeftParen) {
                    ExprKind::Ident("na".to_string())
                } else {
                    ExprKind::Na
                },
            }),
            TokenKind::String(value) => Some(Expr {
                id: self.alloc_id(),
                span: token.span,
                kind: ExprKind::String(value),
            }),
            TokenKind::Ident(name) => Some(Expr {
                id: self.alloc_id(),
                span: token.span,
                kind: ExprKind::Ident(name),
            }),
            TokenKind::Interval(_) => {
                self.push_diagnostic(
                    "global interval-qualified series are not supported; use `<alias>.<interval>.<field>`",
                    token.span,
                );
                None
            }
            TokenKind::Minus => {
                let expr = self.parse_expr(50)?;
                let span = token.span.merge(expr.span);
                Some(Expr {
                    id: self.alloc_id(),
                    span,
                    kind: ExprKind::Unary {
                        op: UnaryOp::Neg,
                        expr: Box::new(expr),
                    },
                })
            }
            TokenKind::Bang => {
                let expr = self.parse_expr(50)?;
                let span = token.span.merge(expr.span);
                Some(Expr {
                    id: self.alloc_id(),
                    span,
                    kind: ExprKind::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(expr),
                    },
                })
            }
            TokenKind::LeftParen => {
                let expr = self.parse_expr(0)?;
                self.expect_kind(
                    |kind| matches!(kind, TokenKind::RightParen),
                    "expected `)` after expression",
                )?;
                Some(expr)
            }
            _ => {
                self.push_diagnostic("expected expression", token.span);
                None
            }
        }
    }

    fn parse_call(&mut self, callee: Expr) -> Option<Expr> {
        let left = self.expect_kind(|kind| matches!(kind, TokenKind::LeftParen), "expected `(`")?;
        let (name, callee_span) = match callee.kind {
            ExprKind::Ident(name) => (name, callee.span),
            _ => {
                self.push_diagnostic("only identifiers can be called in v0.1", callee.span);
                return None;
            }
        };
        let mut args = Vec::new();
        if !matches!(self.peek_kind(), TokenKind::RightParen) {
            loop {
                args.push(self.parse_expr(0)?);
                if !matches!(self.peek_kind(), TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
        }
        let right = self.expect_kind(
            |kind| matches!(kind, TokenKind::RightParen),
            "expected `)` after arguments",
        )?;
        Some(Expr {
            id: self.alloc_id(),
            span: left.span.merge(right.span),
            kind: ExprKind::Call {
                callee: name,
                callee_span,
                args,
            },
        })
    }

    fn parse_dotted_ident(&mut self, source: Expr) -> Option<Expr> {
        let (source_name, source_span) = match source.kind {
            ExprKind::Ident(name) => (name, source.span),
            _ => unreachable!(),
        };
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Dot),
            "expected `.` after source alias",
        )?;
        match self.advance()?.clone() {
            Token {
                kind: TokenKind::Ident(name),
                span,
            } => {
                if let Some(field) = MarketField::parse(&name) {
                    Some(Expr {
                        id: self.alloc_id(),
                        span: source_span.merge(span),
                        kind: ExprKind::SourceSeries {
                            source: source_name,
                            source_span,
                            interval: None,
                            field,
                        },
                    })
                } else {
                    Some(Expr {
                        id: self.alloc_id(),
                        span: source_span.merge(span),
                        kind: ExprKind::EnumVariant {
                            namespace: source_name,
                            namespace_span: source_span,
                            variant: name,
                            variant_span: span,
                        },
                    })
                }
            }
            Token {
                kind: TokenKind::Interval(interval),
                span: _interval_span,
            } => {
                self.expect_kind(
                    |kind| matches!(kind, TokenKind::Dot),
                    "expected `.` after interval literal",
                )?;
                let token = self.expect_kind(
                    |kind| matches!(kind, TokenKind::Ident(_)),
                    "expected market field after `.`",
                )?;
                let TokenKind::Ident(name) = token.kind else {
                    unreachable!();
                };
                let Some(field) = MarketField::parse(&name) else {
                    self.push_diagnostic("expected market field after `.`", token.span);
                    return None;
                };
                Some(Expr {
                    id: self.alloc_id(),
                    span: source_span.merge(token.span),
                    kind: ExprKind::SourceSeries {
                        source: source_name,
                        source_span,
                        interval: Some(interval),
                        field,
                    },
                })
            }
            token => {
                self.push_diagnostic("expected market field or interval after `.`", token.span);
                None
            }
        }
    }

    fn parse_index(&mut self, target: Expr) -> Option<Expr> {
        self.expect_kind(
            |kind| matches!(kind, TokenKind::LeftBracket),
            "expected `[`",
        )?;
        let index = self.parse_expr(0)?;
        let right = self.expect_kind(
            |kind| matches!(kind, TokenKind::RightBracket),
            "expected `]` after index",
        )?;
        Some(Expr {
            id: self.alloc_id(),
            span: target.span.merge(right.span),
            kind: ExprKind::Index {
                target: Box::new(target),
                index: Box::new(index),
            },
        })
    }

    fn infix_binding_power(&self) -> Option<(u8, u8, BinaryOp)> {
        match self.peek_kind() {
            TokenKind::Or => Some((5, 6, BinaryOp::Or)),
            TokenKind::And => Some((7, 8, BinaryOp::And)),
            TokenKind::EqualEqual => Some((10, 11, BinaryOp::Eq)),
            TokenKind::BangEqual => Some((10, 11, BinaryOp::Ne)),
            TokenKind::Less => Some((10, 11, BinaryOp::Lt)),
            TokenKind::LessEqual => Some((10, 11, BinaryOp::Le)),
            TokenKind::Greater => Some((10, 11, BinaryOp::Gt)),
            TokenKind::GreaterEqual => Some((10, 11, BinaryOp::Ge)),
            TokenKind::Plus => Some((20, 21, BinaryOp::Add)),
            TokenKind::Minus => Some((20, 21, BinaryOp::Sub)),
            TokenKind::Star => Some((30, 31, BinaryOp::Mul)),
            TokenKind::Slash => Some((30, 31, BinaryOp::Div)),
            _ => None,
        }
    }

    fn expect_assign(&mut self) -> Option<Token> {
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Assign),
            "expected `=` after identifier",
        )
    }

    fn expect_ident(&mut self, message: &'static str) -> Option<(String, Span)> {
        let token = self.expect_kind(|kind| matches!(kind, TokenKind::Ident(_)), message)?;
        let TokenKind::Ident(name) = token.kind else {
            unreachable!();
        };
        Some((name, token.span))
    }

    fn expect_string(&mut self, message: &'static str) -> Option<(String, Span)> {
        let token = self.expect_kind(|kind| matches!(kind, TokenKind::String(_)), message)?;
        let TokenKind::String(value) = token.kind else {
            unreachable!();
        };
        Some((value, token.span))
    }

    fn expect_kind(
        &mut self,
        predicate: impl Fn(&TokenKind) -> bool,
        message: &'static str,
    ) -> Option<Token> {
        let token = self.advance()?.clone();
        if predicate(&token.kind) {
            Some(token)
        } else {
            self.push_diagnostic(message, token.span);
            None
        }
    }

    fn matches_keyword(&mut self, expected: &TokenKind) -> bool {
        let matches = std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(expected);
        if matches {
            self.advance();
        }
        matches
    }

    fn skip_separators(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline) {
            self.advance();
        }
    }

    fn synchronize(&mut self) {
        while !self.is_eof() {
            if matches!(self.peek_kind(), TokenKind::Newline | TokenKind::RightBrace) {
                break;
            }
            self.advance();
        }
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.cursor.saturating_sub(1)]
    }

    fn advance(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.cursor)?;
        self.cursor += 1;
        Some(token)
    }

    fn peek_kind(&self) -> &TokenKind {
        &self
            .tokens
            .get(self.cursor)
            .unwrap_or_else(|| self.tokens.last().expect("parser requires EOF token"))
            .kind
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn push_diagnostic(&mut self, message: &'static str, span: Span) {
        self.diagnostics
            .push(Diagnostic::new(DiagnosticKind::Parse, message, span));
    }

    fn alloc_id(&mut self) -> NodeId {
        let id = self.next_node_id;
        self.next_node_id += 1;
        id
    }
}

enum ParsedItem {
    BaseInterval(IntervalDecl),
    Source(SourceDecl),
    UseInterval(SourceIntervalDecl),
    Function(FunctionDecl),
    Stmt(Stmt),
}
