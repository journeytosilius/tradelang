//! Parsing from tokens into the TradeLang AST.
//!
//! The parser builds typed source-level nodes, preserves spans, and accumulates
//! parse diagnostics instead of emitting bytecode directly.

use crate::ast::{Ast, BinaryOp, Block, Expr, ExprKind, NodeId, Stmt, StmtKind, UnaryOp};
use crate::diagnostic::{CompileError, Diagnostic, DiagnosticKind};
use crate::span::Span;
use crate::token::{Token, TokenKind};

pub fn parse(tokens: &[Token]) -> Result<Ast, CompileError> {
    Parser::new(tokens).parse()
}

struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
    next_node_id: NodeId,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            cursor: 0,
            next_node_id: 1,
            diagnostics: Vec::new(),
        }
    }

    fn parse(mut self) -> Result<Ast, CompileError> {
        let mut statements = Vec::new();
        self.skip_separators();
        while !self.is_eof() {
            match self.parse_stmt() {
                Some(stmt) => statements.push(stmt),
                None => self.synchronize(),
            }
            self.skip_separators();
        }

        if self.diagnostics.is_empty() {
            Ok(Ast { statements })
        } else {
            Err(CompileError::new(self.diagnostics))
        }
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
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

    fn parse_let_stmt(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        let name = match self.advance().map(|token| &token.kind) {
            Some(TokenKind::Ident(name)) => name.clone(),
            _ => {
                self.error_here("expected identifier after `let`");
                return None;
            }
        };
        self.expect_assign()?;
        let expr = self.parse_expr(0)?;
        let span = start.merge(expr.span);
        Some(Stmt {
            id: self.alloc_id(),
            span,
            kind: StmtKind::Let { name, expr },
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
        self.skip_separators();
        while !matches!(self.peek_kind(), TokenKind::RightBrace | TokenKind::Eof) {
            match self.parse_stmt() {
                Some(stmt) => statements.push(stmt),
                None => self.synchronize(),
            }
            self.skip_separators();
        }
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
                kind: ExprKind::Na,
            }),
            TokenKind::Ident(name) => Some(Expr {
                id: self.alloc_id(),
                span: token.span,
                kind: ExprKind::Ident(name),
            }),
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
        let name = match callee.kind {
            ExprKind::Ident(name) => name,
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
            kind: ExprKind::Call { callee: name, args },
        })
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
        &self.tokens[self.cursor].kind
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn error_here(&mut self, message: &'static str) {
        let span = self.tokens[self.cursor.saturating_sub(1)].span;
        self.push_diagnostic(message, span);
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
