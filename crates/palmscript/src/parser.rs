//! Parsing from tokens into the PalmScript AST.
//!
//! The parser builds typed source-level nodes, preserves spans, and accumulates
//! parse diagnostics instead of emitting bytecode directly.

use crate::ast::{
    Ast, BinaryOp, BindingName, Block, ExecutionDecl, Expr, ExprKind, FunctionDecl, FunctionParam,
    InputOptimization, InputOptimizationKind, IntervalDecl, NodeId, OrderSpec, OrderSpecKind,
    PortfolioControlKind, PortfolioGroupDecl, RiskControlKind, SignalModuleDecl, SignalRole,
    SourceDecl, SourceIntervalDecl, Stmt, StmtKind, StrategyIntervals, UnaryOp,
};
use crate::diagnostic::{CompileError, Diagnostic, DiagnosticKind};
use crate::span::Span;
use crate::token::{Token, TokenKind};
use crate::{MarketField, PositionSide, SourceTemplate};

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
                Some(ParsedItem::Execution(decl)) => strategy_intervals.executions.push(decl),
                Some(ParsedItem::UseInterval(decl)) => strategy_intervals.supplemental.push(decl),
                Some(ParsedItem::Function(function)) => functions.push(function),
                Some(ParsedItem::Stmt(stmt)) => statements.push(*stmt),
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
        if self.matches_keyword(&TokenKind::Execution) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "interval directives are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_execution_decl().map(ParsedItem::Execution);
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
        self.parse_stmt()
            .map(|stmt| ParsedItem::Stmt(Box::new(stmt)))
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
            || self.matches_keyword(&TokenKind::Execution)
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
            return self.parse_output_stmt(OutputStmtKind::Export);
        }
        if self.matches_keyword(&TokenKind::Regime) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "regime statements are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_output_stmt(OutputStmtKind::Regime);
        }
        if self.matches_keyword(&TokenKind::Trigger) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "trigger statements are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_output_stmt(OutputStmtKind::Trigger);
        }
        if self.matches_keyword(&TokenKind::Cooldown) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "risk control declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_risk_control_stmt(RiskControlKind::Cooldown);
        }
        if self.matches_keyword(&TokenKind::MaxBarsInTrade) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "risk control declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_risk_control_stmt(RiskControlKind::MaxBarsInTrade);
        }
        if self.matches_keyword(&TokenKind::PortfolioGroup) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "portfolio declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_portfolio_group_stmt();
        }
        if self.matches_keyword(&TokenKind::Module) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "module declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_module_stmt();
        }
        if self.matches_keyword(&TokenKind::MaxPositions) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "portfolio declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_portfolio_control_stmt(PortfolioControlKind::MaxPositions);
        }
        if self.matches_keyword(&TokenKind::MaxLongPositions) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "portfolio declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_portfolio_control_stmt(PortfolioControlKind::MaxLongPositions);
        }
        if self.matches_keyword(&TokenKind::MaxShortPositions) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "portfolio declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_portfolio_control_stmt(PortfolioControlKind::MaxShortPositions);
        }
        if self.matches_keyword(&TokenKind::MaxGrossExposurePct) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "portfolio declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_portfolio_control_stmt(PortfolioControlKind::MaxGrossExposurePct);
        }
        if self.matches_keyword(&TokenKind::MaxNetExposurePct) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "portfolio declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_portfolio_control_stmt(PortfolioControlKind::MaxNetExposurePct);
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
        if self.matches_keyword(&TokenKind::Protect) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "attached exit declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_attached_order_stmt(true);
        }
        if self.matches_keyword(&TokenKind::Target) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "attached exit declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_attached_order_stmt(false);
        }
        if self.matches_keyword(&TokenKind::Size) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "order size declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_order_size_stmt();
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
        if self.matches_keyword(&TokenKind::Order) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "order declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_order_stmt();
        }
        if self.matches_keyword(&TokenKind::OrderTemplate) {
            if self.block_depth > 0 {
                self.push_diagnostic(
                    "order template declarations are only allowed at the top level",
                    self.previous().span,
                );
                return None;
            }
            return self.parse_order_template_stmt();
        }
        if self.matches_keyword(&TokenKind::If) {
            return self.parse_if_stmt();
        }
        if let Some(stmt) = self.parse_staged_top_level_stmt() {
            return Some(stmt);
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
        let optimization = if !is_const && self.matches_keyword(&TokenKind::Optimize) {
            Some(self.parse_input_optimization(self.previous().span)?)
        } else {
            None
        };
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
                    optimization,
                }
            },
        })
    }

    fn parse_input_optimization(&mut self, start: Span) -> Option<InputOptimization> {
        self.expect_kind(
            |kind| matches!(kind, TokenKind::LeftParen),
            "expected `(` after `optimize`",
        )?;
        let (kind_name, kind_span) =
            self.expect_ident("expected optimization kind after `optimize(`")?;
        let kind = match kind_name.as_str() {
            "int" => self.parse_integer_input_optimization(kind_span)?,
            "float" => self.parse_float_input_optimization()?,
            "choice" => self.parse_choice_input_optimization(kind_span)?,
            _ => {
                self.push_diagnostic(
                    "input optimization kind must be `int`, `float`, or `choice`",
                    kind_span,
                );
                return None;
            }
        };
        let right = self.expect_kind(
            |kind| matches!(kind, TokenKind::RightParen),
            "expected `)` after input optimization metadata",
        )?;
        Some(InputOptimization {
            span: start.merge(right.span),
            kind,
        })
    }

    fn parse_integer_input_optimization(
        &mut self,
        kind_span: Span,
    ) -> Option<InputOptimizationKind> {
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Comma),
            "expected `,` after `int` in input optimization metadata",
        )?;
        let low = self.parse_integer_metadata_value(
            "expected integer low bound in `optimize(int, low, high[, step])`",
        )?;
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Comma),
            "expected `,` after integer low bound",
        )?;
        let high = self.parse_integer_metadata_value(
            "expected integer high bound in `optimize(int, low, high[, step])`",
        )?;
        let step = if matches!(self.peek_kind(), TokenKind::Comma) {
            self.advance();
            self.parse_integer_metadata_value(
                "expected integer step in `optimize(int, low, high, step)`",
            )?
        } else {
            1
        };
        if step <= 0 {
            self.push_diagnostic("input optimization integer step must be > 0", kind_span);
            return None;
        }
        Some(InputOptimizationKind::IntegerRange { low, high, step })
    }

    fn parse_float_input_optimization(&mut self) -> Option<InputOptimizationKind> {
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Comma),
            "expected `,` after `float` in input optimization metadata",
        )?;
        let low = self.parse_float_metadata_value(
            "expected float low bound in `optimize(float, low, high[, step])`",
        )?;
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Comma),
            "expected `,` after float low bound",
        )?;
        let high = self.parse_float_metadata_value(
            "expected float high bound in `optimize(float, low, high[, step])`",
        )?;
        let step = if matches!(self.peek_kind(), TokenKind::Comma) {
            self.advance();
            Some(self.parse_float_metadata_value(
                "expected float step in `optimize(float, low, high, step)`",
            )?)
        } else {
            None
        };
        Some(InputOptimizationKind::FloatRange { low, high, step })
    }

    fn parse_choice_input_optimization(
        &mut self,
        kind_span: Span,
    ) -> Option<InputOptimizationKind> {
        let mut values = Vec::new();
        while matches!(self.peek_kind(), TokenKind::Comma) {
            self.advance();
            values.push(self.parse_float_metadata_value(
                "expected numeric choice in `optimize(choice, v1, v2, ...)`",
            )?);
        }
        if values.is_empty() {
            self.push_diagnostic(
                "`optimize(choice, ...)` requires at least one numeric choice",
                kind_span,
            );
            return None;
        }
        Some(InputOptimizationKind::Choice { values })
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
        let parsed = self.parse_market_binding(MarketBindingKind::Source)?;
        Some(SourceDecl {
            alias,
            alias_span,
            template: parsed.template,
            template_span: parsed.template_span,
            symbol: parsed.symbol,
            symbol_span: parsed.symbol_span,
            span: start.merge(parsed.span),
        })
    }

    fn parse_execution_decl(&mut self) -> Option<ExecutionDecl> {
        let start = self.previous().span;
        let (alias, alias_span) = self.expect_ident("expected identifier after `execution`")?;
        let parsed = self.parse_market_binding(MarketBindingKind::Execution)?;
        Some(ExecutionDecl {
            alias,
            alias_span,
            template: parsed.template,
            template_span: parsed.template_span,
            symbol: parsed.symbol,
            symbol_span: parsed.symbol_span,
            span: start.merge(parsed.span),
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

    fn parse_market_binding(&mut self, kind: MarketBindingKind) -> Option<ParsedMarketBinding> {
        self.expect_assign()?;
        let (exchange, exchange_span) = self.expect_ident(kind.exchange_error())?;
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
            kind.left_paren_error(),
        )?;
        let (symbol, symbol_span) = self.expect_string(kind.symbol_error())?;
        let right = self.expect_kind(
            |kind| matches!(kind, TokenKind::RightParen),
            kind.right_paren_error(),
        )?;
        Some(ParsedMarketBinding {
            template,
            template_span,
            symbol,
            symbol_span,
            span: template_span.merge(right.span),
        })
    }

    fn parse_output_stmt(&mut self, kind: OutputStmtKind) -> Option<Stmt> {
        let start = self.previous().span;
        let (name, name_span) = self.expect_ident(kind.ident_error())?;
        self.expect_assign()?;
        let expr = self.parse_expr(0)?;
        let span = start.merge(expr.span);
        Some(Stmt {
            id: self.alloc_id(),
            span,
            kind: match kind {
                OutputStmtKind::Export => StmtKind::Export {
                    name,
                    name_span,
                    expr,
                },
                OutputStmtKind::Regime => StmtKind::Regime {
                    name,
                    name_span,
                    expr,
                },
                OutputStmtKind::Trigger => StmtKind::Trigger {
                    name,
                    name_span,
                    expr,
                },
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

    fn parse_risk_control_stmt(&mut self, kind: RiskControlKind) -> Option<Stmt> {
        let start = self.previous().span;
        let side = self.parse_position_side(match kind {
            RiskControlKind::Cooldown => "expected `long` or `short` after `cooldown`",
            RiskControlKind::MaxBarsInTrade => {
                "expected `long` or `short` after `max_bars_in_trade`"
            }
        })?;
        self.expect_kind(
            |token| matches!(token, TokenKind::Assign),
            "expected `=` after risk control side",
        )?;
        let expr = self.parse_expr(0)?;
        Some(Stmt {
            id: self.alloc_id(),
            span: start.merge(expr.span),
            kind: StmtKind::RiskControl { kind, side, expr },
        })
    }

    fn parse_portfolio_control_stmt(&mut self, kind: PortfolioControlKind) -> Option<Stmt> {
        let start = self.previous().span;
        self.expect_kind(
            |token| matches!(token, TokenKind::Assign),
            "expected `=` after portfolio control name",
        )?;
        let expr = self.parse_expr(0)?;
        Some(Stmt {
            id: self.alloc_id(),
            span: start.merge(expr.span),
            kind: StmtKind::PortfolioControl { kind, expr },
        })
    }

    fn parse_portfolio_group_stmt(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        let name = match self.peek_kind() {
            TokenKind::String(value) => {
                let value = value.clone();
                self.advance();
                value
            }
            _ => {
                self.push_diagnostic(
                    "expected string name after `portfolio_group`",
                    self.tokens
                        .get(self.cursor)
                        .map(|token| token.span)
                        .unwrap_or_else(|| self.previous().span),
                );
                return None;
            }
        };
        let name_span = self.previous().span;
        self.expect_kind(
            |token| matches!(token, TokenKind::Assign),
            "expected `=` after portfolio group name",
        )?;
        self.expect_kind(
            |token| matches!(token, TokenKind::LeftBracket),
            "expected `[` after portfolio group assignment",
        )?;
        let mut aliases = Vec::new();
        if !matches!(self.peek_kind(), TokenKind::RightBracket) {
            loop {
                let (alias, span) =
                    self.expect_ident("expected source alias in portfolio group")?;
                aliases.push(BindingName { name: alias, span });
                if !matches!(self.peek_kind(), TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
        }
        self.expect_kind(
            |token| matches!(token, TokenKind::RightBracket),
            "expected `]` after portfolio group aliases",
        )?;
        let group = PortfolioGroupDecl {
            name,
            name_span,
            aliases,
            span: start.merge(self.previous().span),
        };
        Some(Stmt {
            id: self.alloc_id(),
            span: group.span,
            kind: StmtKind::PortfolioGroup { group },
        })
    }

    fn parse_module_stmt(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        let (name, name_span) = self.expect_ident("expected identifier after `module`")?;
        self.expect_kind(
            |token| matches!(token, TokenKind::Assign),
            "expected `=` after module name",
        )?;
        let role = self.parse_module_entry_role()?;
        let span = start.merge(self.previous().span);
        Some(Stmt {
            id: self.alloc_id(),
            span,
            kind: StmtKind::Module {
                module: SignalModuleDecl {
                    name,
                    name_span,
                    role,
                    span,
                },
            },
        })
    }

    fn parse_staged_top_level_stmt(&mut self) -> Option<Stmt> {
        let token = match self.tokens.get(self.cursor).cloned() {
            Some(
                token @ Token {
                    kind: TokenKind::Ident(_),
                    ..
                },
            ) => token,
            _ => return None,
        };
        let TokenKind::Ident(name) = token.kind.clone() else {
            return None;
        };
        if self.block_depth > 0 {
            if staged_signal_role_for_ident(&name).is_some()
                || staged_attached_role_for_ident(&name).is_some()
                || staged_size_role_for_ident(&name).is_some()
            {
                self.push_diagnostic(
                    "staged signal and order declarations are only allowed at the top level",
                    token.span,
                );
                self.advance();
                return None;
            }
            return None;
        }

        if let Some(role) = staged_signal_role_for_ident(&name) {
            self.advance();
            return self.parse_staged_signal_stmt(token.span, role);
        }
        if let Some(role) = staged_attached_role_for_ident(&name) {
            self.advance();
            return self.parse_staged_attached_order_stmt(token.span, role);
        }
        if let Some(role) = staged_size_role_for_ident(&name) {
            self.advance();
            return self.parse_staged_order_size_stmt(token.span, role);
        }
        None
    }

    fn parse_staged_signal_stmt(&mut self, start: Span, role: SignalRole) -> Option<Stmt> {
        let side = self.advance()?.clone();
        let role = match side.kind {
            TokenKind::Long => long_role(role),
            TokenKind::Short => short_role(role),
            _ => {
                self.push_diagnostic(
                    "expected `long` or `short` after staged signal role",
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

    fn parse_staged_attached_order_stmt(&mut self, start: Span, role: SignalRole) -> Option<Stmt> {
        let side = self.advance()?.clone();
        let role = match side.kind {
            TokenKind::Long => long_role(role),
            TokenKind::Short => short_role(role),
            _ => {
                self.push_diagnostic(
                    "expected `long` or `short` after staged attached exit role",
                    side.span,
                );
                return None;
            }
        };
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Assign),
            "expected `=` after attached exit side",
        )?;
        let spec = self.parse_order_spec()?;
        Some(Stmt {
            id: self.alloc_id(),
            span: start.merge(spec.span),
            kind: StmtKind::Order {
                role,
                spec: Box::new(spec),
            },
        })
    }

    fn parse_staged_order_size_stmt(&mut self, start: Span, role: SignalRole) -> Option<Stmt> {
        let side = self.advance()?.clone();
        let role = match side.kind {
            TokenKind::Long => long_role(role),
            TokenKind::Short => short_role(role),
            _ => {
                self.push_diagnostic(
                    "expected `long` or `short` after staged order size role",
                    side.span,
                );
                return None;
            }
        };
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Assign),
            "expected `=` after order size side",
        )?;
        let expr = self.parse_expr(0)?;
        Some(Stmt {
            id: self.alloc_id(),
            span: start.merge(expr.span),
            kind: StmtKind::OrderSize { role, expr },
        })
    }

    fn parse_order_stmt(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        let role = if self.matches_keyword(&TokenKind::Entry) {
            self.parse_side_role(
                "expected `long` or `short` after `order entry`",
                |is_long| {
                    if is_long {
                        SignalRole::LongEntry
                    } else {
                        SignalRole::ShortEntry
                    }
                },
            )?
        } else if self.matches_keyword(&TokenKind::Exit) {
            self.parse_side_role("expected `long` or `short` after `order exit`", |is_long| {
                if is_long {
                    SignalRole::LongExit
                } else {
                    SignalRole::ShortExit
                }
            })?
        } else if let TokenKind::Ident(name) = self.peek_kind().clone() {
            let Some(base_role) = staged_signal_role_for_ident(&name) else {
                self.push_diagnostic(
                    "expected `entry`, `exit`, or `entry1..3` after `order`",
                    self.tokens[self.cursor].span,
                );
                return None;
            };
            let start_token = self.advance()?.clone();
            match self.advance()?.kind.clone() {
                TokenKind::Long => long_role(base_role),
                TokenKind::Short => short_role(base_role),
                _ => {
                    self.push_diagnostic(
                        "expected `long` or `short` after staged order role",
                        start_token.span,
                    );
                    return None;
                }
            }
        } else {
            self.push_diagnostic(
                "expected `entry`, `exit`, or `entry1..3` after `order`",
                self.tokens[self.cursor].span,
            );
            return None;
        };
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Assign),
            "expected `=` after order side",
        )?;
        let spec = self.parse_order_spec()?;
        Some(Stmt {
            id: self.alloc_id(),
            span: start.merge(spec.span),
            kind: StmtKind::Order {
                role,
                spec: Box::new(spec),
            },
        })
    }

    fn parse_order_template_stmt(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        let (name, name_span) = self.expect_ident("expected identifier after `order_template`")?;
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Assign),
            "expected `=` after order template name",
        )?;
        let spec = self.parse_order_spec()?;
        Some(Stmt {
            id: self.alloc_id(),
            span: start.merge(spec.span),
            kind: StmtKind::OrderTemplate {
                name,
                name_span,
                spec: Box::new(spec),
            },
        })
    }

    fn parse_attached_order_stmt(&mut self, protect: bool) -> Option<Stmt> {
        let start = self.previous().span;
        let role = if protect {
            self.parse_side_role("expected `long` or `short` after `protect`", |is_long| {
                if is_long {
                    SignalRole::ProtectLong
                } else {
                    SignalRole::ProtectShort
                }
            })?
        } else {
            self.parse_side_role("expected `long` or `short` after `target`", |is_long| {
                if is_long {
                    SignalRole::TargetLong
                } else {
                    SignalRole::TargetShort
                }
            })?
        };
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Assign),
            "expected `=` after attached exit side",
        )?;
        let spec = self.parse_order_spec()?;
        Some(Stmt {
            id: self.alloc_id(),
            span: start.merge(spec.span),
            kind: StmtKind::Order {
                role,
                spec: Box::new(spec),
            },
        })
    }

    fn parse_order_size_stmt(&mut self) -> Option<Stmt> {
        let start = self.previous().span;
        let role = if self.matches_keyword(&TokenKind::Target) {
            self.parse_side_role(
                "expected `long` or `short` after `size target`",
                |is_long| {
                    if is_long {
                        SignalRole::TargetLong
                    } else {
                        SignalRole::TargetShort
                    }
                },
            )?
        } else if self.matches_keyword(&TokenKind::Entry) {
            self.parse_side_role("expected `long` or `short` after `size entry`", |is_long| {
                if is_long {
                    SignalRole::LongEntry
                } else {
                    SignalRole::ShortEntry
                }
            })?
        } else if let TokenKind::Ident(name) = self.peek_kind().clone() {
            let Some(base_role) = staged_size_role_for_ident(&name) else {
                self.push_diagnostic(
                    "expected `entry`, `target`, `entry1..3`, or `target1..3` after `size`",
                    self.tokens[self.cursor].span,
                );
                return None;
            };
            let start_token = self.advance()?.clone();
            let role = match self.advance()?.kind.clone() {
                TokenKind::Long => long_role(base_role),
                TokenKind::Short => short_role(base_role),
                _ => {
                    self.push_diagnostic(
                        "expected `long` or `short` after staged order size role",
                        start_token.span,
                    );
                    return None;
                }
            };
            role
        } else {
            self.push_diagnostic(
                "expected `entry`, `target`, `entry1..3`, or `target1..3` after `size`",
                self.tokens[self.cursor].span,
            );
            return None;
        };
        self.expect_kind(
            |kind| matches!(kind, TokenKind::Assign),
            "expected `=` after order size side",
        )?;
        let expr = self.parse_expr(0)?;
        Some(Stmt {
            id: self.alloc_id(),
            span: start.merge(expr.span),
            kind: StmtKind::OrderSize { role, expr },
        })
    }

    fn parse_order_spec(&mut self) -> Option<OrderSpec> {
        let token = self.advance()?.clone();
        let (callee, callee_span) = match token.kind {
            TokenKind::Ident(name) => (name, token.span),
            _ => {
                self.push_diagnostic("expected order constructor name", token.span);
                return None;
            }
        };
        if !matches!(self.peek_kind(), TokenKind::LeftParen) {
            return Some(OrderSpec {
                span: callee_span,
                execution: None,
                kind: OrderSpecKind::TemplateRef(BindingName {
                    name: callee,
                    span: callee_span,
                }),
            });
        }
        self.expect_kind(
            |kind| matches!(kind, TokenKind::LeftParen),
            "expected `(` after order constructor",
        )?;

        let mut args = Vec::new();
        if !matches!(self.peek_kind(), TokenKind::RightParen) {
            loop {
                args.push(self.parse_order_arg()?);
                if !matches!(self.peek_kind(), TokenKind::Comma) {
                    break;
                }
                self.advance();
            }
        }
        let right = self.expect_kind(
            |kind| matches!(kind, TokenKind::RightParen),
            "expected `)` after order constructor arguments",
        )?;
        let span = callee_span.merge(right.span);
        let (execution, kind) = self.build_order_spec(&callee, &args, span)?;
        Some(OrderSpec {
            span,
            execution,
            kind,
        })
    }

    fn parse_order_arg(&mut self) -> Option<ParsedOrderArg> {
        if let (
            Some(Token {
                kind: TokenKind::Ident(name),
                span,
            }),
            Some(Token {
                kind: TokenKind::Assign,
                ..
            }),
        ) = (
            self.tokens.get(self.cursor),
            self.tokens.get(self.cursor + 1),
        ) {
            let name = name.clone();
            let name_span = *span;
            self.advance();
            self.advance();
            let value = self.parse_expr(0)?;
            return Some(ParsedOrderArg::Named {
                name,
                name_span,
                value,
            });
        }
        self.parse_expr(0).map(ParsedOrderArg::Positional)
    }

    fn build_order_spec(
        &mut self,
        callee: &str,
        args: &[ParsedOrderArg],
        span: Span,
    ) -> Option<(Option<BindingName>, OrderSpecKind)> {
        let uses_named = args
            .iter()
            .any(|arg| matches!(arg, ParsedOrderArg::Named { .. }));
        let uses_positional = args
            .iter()
            .any(|arg| matches!(arg, ParsedOrderArg::Positional(_)));
        if uses_named && uses_positional {
            self.push_diagnostic(
                "order constructors must use either positional arguments or named arguments",
                span,
            );
            return None;
        }
        if !uses_named {
            return self.build_positional_order_spec(callee, args, span);
        }
        self.build_named_order_spec(callee, args, span)
    }

    fn build_positional_order_spec(
        &mut self,
        callee: &str,
        args: &[ParsedOrderArg],
        span: Span,
    ) -> Option<(Option<BindingName>, OrderSpecKind)> {
        let positional: Option<Vec<Expr>> = args
            .iter()
            .map(|arg| match arg {
                ParsedOrderArg::Positional(expr) => Some(expr.clone()),
                ParsedOrderArg::Named { .. } => None,
            })
            .collect();
        let positional = positional?;
        let kind = match (callee, positional.as_slice()) {
            ("market", []) => OrderSpecKind::Market,
            ("limit", [price, tif, post_only]) => OrderSpecKind::Limit {
                price: price.clone(),
                tif: tif.clone(),
                post_only: post_only.clone(),
            },
            ("stop_market", [trigger_price, trigger_ref]) => OrderSpecKind::StopMarket {
                trigger_price: trigger_price.clone(),
                trigger_ref: trigger_ref.clone(),
            },
            (
                "stop_limit",
                [trigger_price, limit_price, tif, post_only, trigger_ref, expire_time_ms],
            ) => OrderSpecKind::StopLimit {
                trigger_price: trigger_price.clone(),
                limit_price: limit_price.clone(),
                tif: tif.clone(),
                post_only: post_only.clone(),
                trigger_ref: trigger_ref.clone(),
                expire_time_ms: expire_time_ms.clone(),
            },
            ("take_profit_market", [trigger_price, trigger_ref]) => {
                OrderSpecKind::TakeProfitMarket {
                    trigger_price: trigger_price.clone(),
                    trigger_ref: trigger_ref.clone(),
                }
            }
            (
                "take_profit_limit",
                [trigger_price, limit_price, tif, post_only, trigger_ref, expire_time_ms],
            ) => OrderSpecKind::TakeProfitLimit {
                trigger_price: trigger_price.clone(),
                limit_price: limit_price.clone(),
                tif: tif.clone(),
                post_only: post_only.clone(),
                trigger_ref: trigger_ref.clone(),
                expire_time_ms: expire_time_ms.clone(),
            },
            _ => {
                self.push_diagnostic("invalid order constructor or arity", span);
                return None;
            }
        };
        Some((None, kind))
    }

    fn build_named_order_spec(
        &mut self,
        callee: &str,
        args: &[ParsedOrderArg],
        span: Span,
    ) -> Option<(Option<BindingName>, OrderSpecKind)> {
        let mut fields = std::collections::BTreeMap::<String, (Span, Expr)>::new();
        let mut execution = None;
        for arg in args {
            let ParsedOrderArg::Named {
                name,
                name_span,
                value,
            } = arg
            else {
                continue;
            };
            if name == "venue" {
                let ExprKind::Ident(alias) = &value.kind else {
                    self.push_diagnostic(
                        "`venue` must reference an execution alias identifier",
                        value.span,
                    );
                    return None;
                };
                if execution.is_some() {
                    self.push_diagnostic("duplicate `venue` order argument", *name_span);
                    return None;
                }
                execution = Some(BindingName {
                    name: alias.clone(),
                    span: value.span,
                });
                continue;
            }
            if fields
                .insert(name.clone(), (*name_span, value.clone()))
                .is_some()
            {
                self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Parse,
                    format!("duplicate `{name}` order argument"),
                    *name_span,
                ));
                return None;
            }
        }
        let take = |name: &str, fields: &mut std::collections::BTreeMap<String, (Span, Expr)>| {
            fields.remove(name).map(|(_, expr)| expr)
        };
        let kind = match callee {
            "market" => OrderSpecKind::Market,
            "limit" => OrderSpecKind::Limit {
                price: take("price", &mut fields)
                    .or_else(|| take("limit_price", &mut fields))
                    .unwrap_or_else(|| {
                        self.push_diagnostic("`limit` requires `price = ...`", span);
                        self.synthetic_na_expr(span)
                    }),
                tif: take("tif", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic("`limit` requires `tif = tif.<variant>`", span);
                    self.synthetic_na_expr(span)
                }),
                post_only: take("post_only", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic("`limit` requires `post_only = <bool>`", span);
                    self.synthetic_na_expr(span)
                }),
            },
            "stop_market" => OrderSpecKind::StopMarket {
                trigger_price: take("trigger_price", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic("`stop_market` requires `trigger_price = ...`", span);
                    self.synthetic_na_expr(span)
                }),
                trigger_ref: take("trigger_ref", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic(
                        "`stop_market` requires `trigger_ref = trigger_ref.<variant>`",
                        span,
                    );
                    self.synthetic_na_expr(span)
                }),
            },
            "stop_limit" => OrderSpecKind::StopLimit {
                trigger_price: take("trigger_price", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic("`stop_limit` requires `trigger_price = ...`", span);
                    self.synthetic_na_expr(span)
                }),
                limit_price: take("limit_price", &mut fields)
                    .or_else(|| take("price", &mut fields))
                    .unwrap_or_else(|| {
                        self.push_diagnostic("`stop_limit` requires `limit_price = ...`", span);
                        self.synthetic_na_expr(span)
                    }),
                tif: take("tif", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic("`stop_limit` requires `tif = tif.<variant>`", span);
                    self.synthetic_na_expr(span)
                }),
                post_only: take("post_only", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic("`stop_limit` requires `post_only = <bool>`", span);
                    self.synthetic_na_expr(span)
                }),
                trigger_ref: take("trigger_ref", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic(
                        "`stop_limit` requires `trigger_ref = trigger_ref.<variant>`",
                        span,
                    );
                    self.synthetic_na_expr(span)
                }),
                expire_time_ms: take("expire_time_ms", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic("`stop_limit` requires `expire_time_ms = ...`", span);
                    self.synthetic_na_expr(span)
                }),
            },
            "take_profit_market" => OrderSpecKind::TakeProfitMarket {
                trigger_price: take("trigger_price", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic(
                        "`take_profit_market` requires `trigger_price = ...`",
                        span,
                    );
                    self.synthetic_na_expr(span)
                }),
                trigger_ref: take("trigger_ref", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic(
                        "`take_profit_market` requires `trigger_ref = trigger_ref.<variant>`",
                        span,
                    );
                    self.synthetic_na_expr(span)
                }),
            },
            "take_profit_limit" => OrderSpecKind::TakeProfitLimit {
                trigger_price: take("trigger_price", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic(
                        "`take_profit_limit` requires `trigger_price = ...`",
                        span,
                    );
                    self.synthetic_na_expr(span)
                }),
                limit_price: take("limit_price", &mut fields)
                    .or_else(|| take("price", &mut fields))
                    .unwrap_or_else(|| {
                        self.push_diagnostic(
                            "`take_profit_limit` requires `limit_price = ...`",
                            span,
                        );
                        self.synthetic_na_expr(span)
                    }),
                tif: take("tif", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic(
                        "`take_profit_limit` requires `tif = tif.<variant>`",
                        span,
                    );
                    self.synthetic_na_expr(span)
                }),
                post_only: take("post_only", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic("`take_profit_limit` requires `post_only = <bool>`", span);
                    self.synthetic_na_expr(span)
                }),
                trigger_ref: take("trigger_ref", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic(
                        "`take_profit_limit` requires `trigger_ref = trigger_ref.<variant>`",
                        span,
                    );
                    self.synthetic_na_expr(span)
                }),
                expire_time_ms: take("expire_time_ms", &mut fields).unwrap_or_else(|| {
                    self.push_diagnostic(
                        "`take_profit_limit` requires `expire_time_ms = ...`",
                        span,
                    );
                    self.synthetic_na_expr(span)
                }),
            },
            _ => {
                self.push_diagnostic("invalid order constructor or arity", span);
                return None;
            }
        };
        if let Some((name, (field_span, _))) = fields.into_iter().next() {
            self.diagnostics.push(Diagnostic::new(
                DiagnosticKind::Parse,
                format!("unexpected `{name}` order argument for `{callee}`"),
                field_span,
            ));
            return None;
        }
        Some((execution, kind))
    }

    fn synthetic_na_expr(&mut self, span: Span) -> Expr {
        Expr {
            id: self.alloc_id(),
            span,
            kind: ExprKind::Na,
        }
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
                if source_name == "position" {
                    let Some(field) = crate::position::PositionField::parse(&name) else {
                        self.push_diagnostic("unknown `position` field", span);
                        return None;
                    };
                    return Some(Expr {
                        id: self.alloc_id(),
                        span: source_span.merge(span),
                        kind: ExprKind::PositionField {
                            field,
                            field_span: span,
                        },
                    });
                }
                if source_name == "position_event" {
                    let Some(field) = crate::position::PositionEventField::parse(&name) else {
                        self.push_diagnostic("unknown `position_event` field", span);
                        return None;
                    };
                    return Some(Expr {
                        id: self.alloc_id(),
                        span: source_span.merge(span),
                        kind: ExprKind::PositionEventField {
                            field,
                            field_span: span,
                        },
                    });
                }
                if let Some(scope) = crate::position::LastExitScope::from_namespace(&source_name) {
                    let Some(field) = crate::position::LastExitField::parse(&name) else {
                        let _ = scope;
                        self.push_diagnostic("unknown last-exit field", span);
                        return None;
                    };
                    return Some(Expr {
                        id: self.alloc_id(),
                        span: source_span.merge(span),
                        kind: ExprKind::LastExitField {
                            scope,
                            field,
                            field_span: span,
                        },
                    });
                }
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
                kind: TokenKind::Long,
                span,
            }
            | Token {
                kind: TokenKind::Short,
                span,
            }
            | Token {
                kind: TokenKind::Protect,
                span,
            }
            | Token {
                kind: TokenKind::Target,
                span,
            } => {
                let name = match self.previous().kind {
                    TokenKind::Long => "long".to_string(),
                    TokenKind::Short => "short".to_string(),
                    TokenKind::Protect => "protect".to_string(),
                    TokenKind::Target => "target".to_string(),
                    _ => unreachable!(),
                };
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

    fn expect_number_literal(&mut self, message: &'static str) -> Option<(String, Span)> {
        let token = self.expect_kind(|kind| matches!(kind, TokenKind::Number(_)), message)?;
        let TokenKind::Number(raw) = token.kind else {
            unreachable!();
        };
        Some((raw, token.span))
    }

    fn parse_signed_number_literal(&mut self, message: &'static str) -> Option<(String, Span)> {
        if matches!(self.peek_kind(), TokenKind::Minus) {
            let minus_span = self.advance().expect("peeked minus token").span;
            let (raw, span) = self.expect_number_literal(message)?;
            return Some((format!("-{raw}"), minus_span.merge(span)));
        }
        self.expect_number_literal(message)
    }

    fn parse_integer_metadata_value(&mut self, message: &'static str) -> Option<i64> {
        let (raw, span) = self.parse_signed_number_literal(message)?;
        let Ok(value) = raw.parse::<f64>() else {
            self.push_diagnostic("failed to parse integer input optimization value", span);
            return None;
        };
        if !value.is_finite() || value.fract() != 0.0 {
            self.push_diagnostic(
                "input optimization integer values must be whole finite numbers",
                span,
            );
            return None;
        }
        Some(value as i64)
    }

    fn parse_float_metadata_value(&mut self, message: &'static str) -> Option<f64> {
        let (raw, span) = self.parse_signed_number_literal(message)?;
        let Ok(value) = raw.parse::<f64>() else {
            self.push_diagnostic("failed to parse input optimization value", span);
            return None;
        };
        if !value.is_finite() {
            self.push_diagnostic("input optimization values must be finite numbers", span);
            return None;
        }
        Some(value)
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

    fn parse_side_role<F>(&mut self, message: &'static str, map: F) -> Option<SignalRole>
    where
        F: FnOnce(bool) -> SignalRole,
    {
        let side = self.advance()?.clone();
        match side.kind {
            TokenKind::Long => Some(map(true)),
            TokenKind::Short => Some(map(false)),
            _ => {
                self.push_diagnostic(message, side.span);
                None
            }
        }
    }

    fn parse_module_entry_role(&mut self) -> Option<SignalRole> {
        if self.matches_keyword(&TokenKind::Entry) {
            return self.parse_side_role(
                "expected `long` or `short` after `entry` in module declaration",
                |is_long| {
                    if is_long {
                        SignalRole::LongEntry
                    } else {
                        SignalRole::ShortEntry
                    }
                },
            );
        }

        let token = self.advance()?.clone();
        let TokenKind::Ident(name) = token.kind else {
            self.push_diagnostic(
                "expected `entry`, `entry2`, or `entry3` after `=` in module declaration",
                token.span,
            );
            return None;
        };
        let Some(role) = staged_signal_role_for_ident(&name) else {
            self.push_diagnostic(
                "module declarations only support `entry`, `entry2`, or `entry3` roles",
                token.span,
            );
            return None;
        };
        let side = self.advance()?.clone();
        match side.kind {
            TokenKind::Long => Some(long_role(role)),
            TokenKind::Short => Some(short_role(role)),
            _ => {
                self.push_diagnostic(
                    "expected `long` or `short` after module entry role",
                    side.span,
                );
                None
            }
        }
    }

    fn parse_position_side(&mut self, message: &'static str) -> Option<PositionSide> {
        let side = self.advance()?.clone();
        match side.kind {
            TokenKind::Long => Some(PositionSide::Long),
            TokenKind::Short => Some(PositionSide::Short),
            _ => {
                self.push_diagnostic(message, side.span);
                None
            }
        }
    }
}

fn staged_signal_role_for_ident(name: &str) -> Option<SignalRole> {
    match name {
        "entry1" => Some(SignalRole::LongEntry),
        "entry2" => Some(SignalRole::LongEntry2),
        "entry3" => Some(SignalRole::LongEntry3),
        _ => None,
    }
}

fn staged_attached_role_for_ident(name: &str) -> Option<SignalRole> {
    match name {
        "target1" => Some(SignalRole::TargetLong),
        "target2" => Some(SignalRole::TargetLong2),
        "target3" => Some(SignalRole::TargetLong3),
        "protect_after_target1" => Some(SignalRole::ProtectAfterTarget1Long),
        "protect_after_target2" => Some(SignalRole::ProtectAfterTarget2Long),
        "protect_after_target3" => Some(SignalRole::ProtectAfterTarget3Long),
        _ => None,
    }
}

fn staged_size_role_for_ident(name: &str) -> Option<SignalRole> {
    match name {
        "entry1" => Some(SignalRole::LongEntry),
        "entry2" => Some(SignalRole::LongEntry2),
        "entry3" => Some(SignalRole::LongEntry3),
        "target1" => Some(SignalRole::TargetLong),
        "target2" => Some(SignalRole::TargetLong2),
        "target3" => Some(SignalRole::TargetLong3),
        _ => None,
    }
}

fn long_role(role: SignalRole) -> SignalRole {
    role
}

fn short_role(role: SignalRole) -> SignalRole {
    match role {
        SignalRole::LongEntry => SignalRole::ShortEntry,
        SignalRole::LongEntry2 => SignalRole::ShortEntry2,
        SignalRole::LongEntry3 => SignalRole::ShortEntry3,
        SignalRole::TargetLong => SignalRole::TargetShort,
        SignalRole::TargetLong2 => SignalRole::TargetShort2,
        SignalRole::TargetLong3 => SignalRole::TargetShort3,
        SignalRole::ProtectAfterTarget1Long => SignalRole::ProtectAfterTarget1Short,
        SignalRole::ProtectAfterTarget2Long => SignalRole::ProtectAfterTarget2Short,
        SignalRole::ProtectAfterTarget3Long => SignalRole::ProtectAfterTarget3Short,
        other => other,
    }
}

enum ParsedItem {
    BaseInterval(IntervalDecl),
    Source(SourceDecl),
    Execution(ExecutionDecl),
    UseInterval(SourceIntervalDecl),
    Function(FunctionDecl),
    Stmt(Box<Stmt>),
}

enum ParsedOrderArg {
    Positional(Expr),
    Named {
        name: String,
        name_span: Span,
        value: Expr,
    },
}

struct ParsedMarketBinding {
    template: SourceTemplate,
    template_span: Span,
    symbol: String,
    symbol_span: Span,
    span: Span,
}

#[derive(Clone, Copy)]
enum MarketBindingKind {
    Source,
    Execution,
}

impl MarketBindingKind {
    fn exchange_error(self) -> &'static str {
        match self {
            Self::Source => "expected exchange name after `=`",
            Self::Execution => "expected exchange name after `=`",
        }
    }

    fn symbol_error(self) -> &'static str {
        match self {
            Self::Source => "expected string literal source symbol",
            Self::Execution => "expected string literal execution symbol",
        }
    }

    fn left_paren_error(self) -> &'static str {
        match self {
            Self::Source => "expected `(` after source template",
            Self::Execution => "expected `(` after execution template",
        }
    }

    fn right_paren_error(self) -> &'static str {
        match self {
            Self::Source => "expected `)` after source symbol",
            Self::Execution => "expected `)` after execution symbol",
        }
    }
}

#[derive(Clone, Copy)]
enum OutputStmtKind {
    Export,
    Regime,
    Trigger,
}

impl OutputStmtKind {
    const fn ident_error(self) -> &'static str {
        match self {
            Self::Export => "expected identifier after `export`",
            Self::Regime => "expected identifier after `regime`",
            Self::Trigger => "expected identifier after `trigger`",
        }
    }
}
