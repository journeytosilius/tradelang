//! Lexical analysis for TradeLang source text.
//!
//! The lexer converts source text into a token stream with spans, preserving
//! statement separators and reporting lexical diagnostics on invalid input.

use crate::diagnostic::{CompileError, Diagnostic, DiagnosticKind};
use crate::span::{Position, Span};
use crate::token::{Token, TokenKind};
use crate::Interval;

pub trait Lexer {
    fn lex(&self, source: &str) -> Result<Vec<Token>, CompileError>;
}

#[derive(Default)]
pub struct DefaultLexer;

impl Lexer for DefaultLexer {
    fn lex(&self, source: &str) -> Result<Vec<Token>, CompileError> {
        lex(source)
    }
}

pub fn lex(source: &str) -> Result<Vec<Token>, CompileError> {
    let mut state = LexerState::new(source);
    state.lex()
}

struct LexerState<'a> {
    source: &'a str,
    cursor: usize,
    line: usize,
    column: usize,
    paren_depth: usize,
    bracket_depth: usize,
    diagnostics: Vec<Diagnostic>,
    tokens: Vec<Token>,
}

impl<'a> LexerState<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            cursor: 0,
            line: 1,
            column: 1,
            paren_depth: 0,
            bracket_depth: 0,
            diagnostics: Vec::new(),
            tokens: Vec::new(),
        }
    }

    fn lex(&mut self) -> Result<Vec<Token>, CompileError> {
        while let Some(ch) = self.peek_char() {
            match ch {
                ' ' | '\t' | '\r' => {
                    self.bump_char();
                }
                '\n' => {
                    let start = self.position();
                    self.bump_char();
                    if self.paren_depth == 0 && self.bracket_depth == 0 {
                        self.tokens.push(Token::new(
                            TokenKind::Newline,
                            Span::new(start, self.position()),
                        ));
                    }
                }
                ';' => {
                    let start = self.position();
                    self.bump_char();
                    self.tokens.push(Token::new(
                        TokenKind::Newline,
                        Span::new(start, self.position()),
                    ));
                }
                '/' if self.peek_next_char() == Some('/') => {
                    self.bump_char();
                    self.bump_char();
                    while let Some(next) = self.peek_char() {
                        if next == '\n' {
                            break;
                        }
                        self.bump_char();
                    }
                }
                '0'..='9' => self.lex_number_or_interval(),
                'a'..='z' | 'A'..='Z' | '_' => self.lex_ident(),
                '(' => self.push_single(TokenKind::LeftParen, true, false),
                ')' => self.push_single(TokenKind::RightParen, false, false),
                '{' => self.push_single(TokenKind::LeftBrace, false, false),
                '}' => self.push_single(TokenKind::RightBrace, false, false),
                '[' => self.push_single(TokenKind::LeftBracket, false, true),
                ']' => self.push_single(TokenKind::RightBracket, false, false),
                ',' => self.push_single(TokenKind::Comma, false, false),
                '.' => self.push_single(TokenKind::Dot, false, false),
                '+' => self.push_single(TokenKind::Plus, false, false),
                '-' => self.push_single(TokenKind::Minus, false, false),
                '*' => self.push_single(TokenKind::Star, false, false),
                '!' => {
                    if self.peek_next_char() == Some('=') {
                        self.push_double(TokenKind::BangEqual);
                    } else {
                        self.push_single(TokenKind::Bang, false, false);
                    }
                }
                '=' => {
                    if self.peek_next_char() == Some('=') {
                        self.push_double(TokenKind::EqualEqual);
                    } else {
                        self.push_single(TokenKind::Assign, false, false);
                    }
                }
                '<' => {
                    if self.peek_next_char() == Some('=') {
                        self.push_double(TokenKind::LessEqual);
                    } else {
                        self.push_single(TokenKind::Less, false, false);
                    }
                }
                '>' => {
                    if self.peek_next_char() == Some('=') {
                        self.push_double(TokenKind::GreaterEqual);
                    } else {
                        self.push_single(TokenKind::Greater, false, false);
                    }
                }
                _ => {
                    let start = self.position();
                    self.bump_char();
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticKind::Lex,
                        format!("unexpected character `{ch}`"),
                        Span::new(start, self.position()),
                    ));
                }
            }
        }

        let pos = self.position();
        self.tokens
            .push(Token::new(TokenKind::Eof, Span::new(pos, pos)));

        if self.diagnostics.is_empty() {
            Ok(std::mem::take(&mut self.tokens))
        } else {
            Err(CompileError::new(std::mem::take(&mut self.diagnostics)))
        }
    }

    fn lex_number_or_interval(&mut self) {
        let start = self.position();
        let mut text = String::new();
        while matches!(self.peek_char(), Some('0'..='9')) {
            text.push(self.bump_char().unwrap());
        }
        if matches!(self.peek_char(), Some('a'..='z' | 'A'..='Z')) {
            while matches!(self.peek_char(), Some('a'..='z' | 'A'..='Z')) {
                text.push(self.bump_char().unwrap());
            }
            match Interval::parse(&text) {
                Some(interval) => self.tokens.push(Token::new(
                    TokenKind::Interval(interval),
                    Span::new(start, self.position()),
                )),
                None => self.diagnostics.push(Diagnostic::new(
                    DiagnosticKind::Lex,
                    format!("unknown interval literal `{text}`"),
                    Span::new(start, self.position()),
                )),
            }
            return;
        }
        if self.peek_char() == Some('.') && matches!(self.peek_next_char(), Some('0'..='9')) {
            text.push(self.bump_char().unwrap());
            while matches!(self.peek_char(), Some('0'..='9')) {
                text.push(self.bump_char().unwrap());
            }
        }
        self.tokens.push(Token::new(
            TokenKind::Number(text),
            Span::new(start, self.position()),
        ));
    }

    fn lex_ident(&mut self) {
        let start = self.position();
        let mut text = String::new();
        while matches!(
            self.peek_char(),
            Some('a'..='z' | 'A'..='Z' | '0'..='9' | '_')
        ) {
            text.push(self.bump_char().unwrap());
        }
        let kind = match text.as_str() {
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "export" => TokenKind::Export,
            "trigger" => TokenKind::Trigger,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "na" => TokenKind::Na,
            _ => TokenKind::Ident(text),
        };
        self.tokens
            .push(Token::new(kind, Span::new(start, self.position())));
    }

    fn push_single(&mut self, kind: TokenKind, inc_paren: bool, inc_bracket: bool) {
        let start = self.position();
        let ch = self.bump_char();
        if ch.is_none() {
            return;
        }
        match kind {
            TokenKind::LeftParen => self.paren_depth += 1,
            TokenKind::RightParen => {
                self.paren_depth = self.paren_depth.saturating_sub(1);
            }
            TokenKind::LeftBracket => self.bracket_depth += 1,
            TokenKind::RightBracket => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
            }
            _ => {
                if inc_paren {
                    self.paren_depth += 1;
                }
                if inc_bracket {
                    self.bracket_depth += 1;
                }
            }
        }
        self.tokens
            .push(Token::new(kind, Span::new(start, self.position())));
    }

    fn push_double(&mut self, kind: TokenKind) {
        let start = self.position();
        self.bump_char();
        self.bump_char();
        self.tokens
            .push(Token::new(kind, Span::new(start, self.position())));
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.cursor..].chars().next()
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.source[self.cursor..].chars();
        chars.next()?;
        chars.next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.cursor += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    fn position(&self) -> Position {
        Position::new(self.cursor, self.line, self.column)
    }
}
