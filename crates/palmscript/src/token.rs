//! Token kinds emitted by the lexer.
//!
//! Tokens retain source spans and represent the stable boundary between source
//! text and the parser.

use crate::span::Span;
use crate::Interval;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TokenKind {
    Fn,
    Let,
    Const,
    Input,
    Order,
    OrderTemplate,
    IntervalKw,
    Source,
    Execution,
    Use,
    Export,
    Optimize,
    Regime,
    Trigger,
    Cooldown,
    MaxBarsInTrade,
    PortfolioGroup,
    MaxPositions,
    MaxLongPositions,
    MaxShortPositions,
    MaxGrossExposurePct,
    MaxNetExposurePct,
    Entry,
    Exit,
    Protect,
    Target,
    Size,
    Long,
    Short,
    If,
    Else,
    And,
    Or,
    True,
    False,
    Na,
    Ident(String),
    Interval(Interval),
    String(String),
    Number(String),
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Comma,
    Colon,
    Question,
    Dot,
    Assign,
    Plus,
    Minus,
    Star,
    Slash,
    Bang,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Newline,
    Eof,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[cfg(test)]
mod tests {
    use super::{Token, TokenKind};
    use crate::span::{Position, Span};

    #[test]
    fn token_new_preserves_kind_and_span() {
        let span = Span::new(Position::new(0, 1, 1), Position::new(3, 1, 4));
        let token = Token::new(TokenKind::Ident("trend".to_string()), span);
        assert_eq!(token.kind, TokenKind::Ident("trend".to_string()));
        assert_eq!(token.span, span);
    }
}
