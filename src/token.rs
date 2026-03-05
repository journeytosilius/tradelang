//! Token kinds emitted by the lexer.
//!
//! Tokens retain source spans and represent the stable boundary between source
//! text and the parser.

use crate::span::Span;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TokenKind {
    Let,
    If,
    Else,
    And,
    Or,
    True,
    False,
    Na,
    Ident(String),
    Number(String),
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Comma,
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
