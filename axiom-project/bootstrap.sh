#!/bin/bash
set -euo pipefail

# AXIOM Language Project Bootstrap
# Run: chmod +x bootstrap.sh && ./bootstrap.sh

PROJECT_ROOT="$(cd "$(dirname "$0")" && pwd)"
echo "🔷 Bootstrapping AXIOM project at: $PROJECT_ROOT"

# ── Create workspace Cargo.toml ──────────────────────────────────────
cat > "$PROJECT_ROOT/Cargo.toml" << 'EOF'
[workspace]
resolver = "2"
members = [
    "crates/axiom-lexer",
    "crates/axiom-parser",
    "crates/axiom-hir",
    "crates/axiom-mir",
    "crates/axiom-codegen",
    "crates/axiom-optimize",
    "crates/axiom-driver",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
repository = "https://github.com/anthropics/axiom"
description = "AXIOM — AI eXchange Intermediate Optimization Medium"

[workspace.dependencies]
thiserror = "2"
miette = { version = "7", features = ["fancy"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
clap = { version = "4", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
insta = { version = "1", features = ["yaml"] }
EOF

# ── Create directory structure ───────────────────────────────────────
mkdir -p "$PROJECT_ROOT/crates"
mkdir -p "$PROJECT_ROOT/tests/samples"
mkdir -p "$PROJECT_ROOT/tests/lit"/{parse,hir,mir,codegen}
mkdir -p "$PROJECT_ROOT/spec"
mkdir -p "$PROJECT_ROOT/examples"/{matmul,sort,nbody}

# ── Crate: axiom-lexer ──────────────────────────────────────────────
CRATE="$PROJECT_ROOT/crates/axiom-lexer"
mkdir -p "$CRATE/src"

cat > "$CRATE/Cargo.toml" << 'EOF'
[package]
name = "axiom-lexer"
version.workspace = true
edition.workspace = true
description = "Lexer/tokenizer for the AXIOM language"

[dependencies]
thiserror.workspace = true
miette.workspace = true

[dev-dependencies]
insta.workspace = true
EOF

cat > "$CRATE/src/lib.rs" << 'RUST'
//! AXIOM Lexer — tokenizes `.axm` source into a stream of typed tokens.
//!
//! Design principles:
//! - Every token carries a Span (byte offset range)
//! - Error recovery: invalid chars produce Error tokens, lexing continues
//! - Annotations (`@name`) and optimization holes (`?name`) are first-class tokens

pub mod token;
pub mod lexer;

pub use token::{Token, TokenKind, Span};
pub use lexer::Lexer;
RUST

cat > "$CRATE/src/token.rs" << 'RUST'
//! Token types for the AXIOM language.

/// Byte offset span in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// A token with its kind and location in source.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ── Literals ────────────────────────────────────────
    IntLiteral(i128),
    FloatLiteral(f64),
    StringLiteral(String),
    BoolLiteral(bool),

    // ── Identifiers & Special Prefixes ──────────────────
    Ident(String),
    /// @annotation_name
    Annotation(String),
    /// ?optimization_hole
    OptHole(String),

    // ── Keywords ────────────────────────────────────────
    Fn,
    Let,
    Mut,
    Return,
    If,
    Else,
    For,
    While,
    In,
    Struct,
    Type,
    Module,
    Import,
    Pub,
    Unsafe,
    And,
    Or,
    Not,

    // ── Type keywords ───────────────────────────────────
    I8, I16, I32, I64, I128,
    U8, U16, U32, U64, U128,
    F16, Bf16, F32, F64,
    Bool,
    Tensor,
    Array,
    Slice,
    Ptr,

    // ── Conversion keywords ─────────────────────────────
    Widen,
    Narrow,
    Truncate,

    // ── Operators ───────────────────────────────────────
    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /
    Percent,        // %
    PlusWrap,       // +%  (wrapping add)
    PlusSat,        // +|  (saturating add)
    MinusWrap,      // -%
    MinusSat,       // -|
    StarWrap,       // *%
    Eq,             // ==
    NotEq,          // !=
    Lt,             // <
    Gt,             // >
    LtEq,          // <=
    GtEq,          // >=
    Assign,         // =
    Arrow,          // ->
    FatArrow,       // =>
    Dot,            // .
    DotDot,         // ..
    Colon,          // :
    ColonColon,     // ::
    Comma,          // ,
    Semicolon,      // ;
    Pipe,           // |  (used in sum types)

    // ── Delimiters ──────────────────────────────────────
    LParen,         // (
    RParen,         // )
    LBracket,       // [
    RBracket,       // ]
    LBrace,         // {
    RBrace,         // }

    // ── Special ─────────────────────────────────────────
    Eof,
    /// Error token — lexer recovered and continued
    Error(String),
}

impl TokenKind {
    /// Try to match an identifier string to a keyword.
    pub fn keyword_from_str(s: &str) -> Option<TokenKind> {
        match s {
            "fn" => Some(TokenKind::Fn),
            "let" => Some(TokenKind::Let),
            "mut" => Some(TokenKind::Mut),
            "return" => Some(TokenKind::Return),
            "if" => Some(TokenKind::If),
            "else" => Some(TokenKind::Else),
            "for" => Some(TokenKind::For),
            "while" => Some(TokenKind::While),
            "in" => Some(TokenKind::In),
            "struct" => Some(TokenKind::Struct),
            "type" => Some(TokenKind::Type),
            "module" => Some(TokenKind::Module),
            "import" => Some(TokenKind::Import),
            "pub" => Some(TokenKind::Pub),
            "unsafe" => Some(TokenKind::Unsafe),
            "and" => Some(TokenKind::And),
            "or" => Some(TokenKind::Or),
            "not" => Some(TokenKind::Not),
            "true" => Some(TokenKind::BoolLiteral(true)),
            "false" => Some(TokenKind::BoolLiteral(false)),
            "i8" => Some(TokenKind::I8),
            "i16" => Some(TokenKind::I16),
            "i32" => Some(TokenKind::I32),
            "i64" => Some(TokenKind::I64),
            "i128" => Some(TokenKind::I128),
            "u8" => Some(TokenKind::U8),
            "u16" => Some(TokenKind::U16),
            "u32" => Some(TokenKind::U32),
            "u64" => Some(TokenKind::U64),
            "u128" => Some(TokenKind::U128),
            "f16" => Some(TokenKind::F16),
            "bf16" => Some(TokenKind::Bf16),
            "f32" => Some(TokenKind::F32),
            "f64" => Some(TokenKind::F64),
            "bool" => Some(TokenKind::Bool),
            "tensor" => Some(TokenKind::Tensor),
            "array" => Some(TokenKind::Array),
            "slice" => Some(TokenKind::Slice),
            "ptr" => Some(TokenKind::Ptr),
            "widen" => Some(TokenKind::Widen),
            "narrow" => Some(TokenKind::Narrow),
            "truncate" => Some(TokenKind::Truncate),
            _ => None,
        }
    }
}
RUST

cat > "$CRATE/src/lexer.rs" << 'RUST'
//! Lexer implementation for AXIOM.
//!
//! Hand-written lexer with full error recovery.
//! Invalid characters produce Error tokens; lexing always continues.

use crate::token::{Token, TokenKind, Span};

pub struct Lexer<'src> {
    source: &'src [u8],
    pos: u32,
    errors: Vec<Token>,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str) -> Self {
        Self {
            source: source.as_bytes(),
            pos: 0,
            errors: Vec::new(),
        }
    }

    /// Tokenize the entire source, returning all tokens including Eof.
    pub fn tokenize(mut self) -> (Vec<Token>, Vec<Token>) {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token();
            let is_eof = tok.kind == TokenKind::Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        let errors = self.errors;
        (tokens, errors)
    }

    fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();

        if self.is_at_end() {
            return Token::new(TokenKind::Eof, Span::new(self.pos, self.pos));
        }

        let start = self.pos;
        let ch = self.advance();

        match ch {
            // Single-char tokens
            b'(' => Token::new(TokenKind::LParen, Span::new(start, self.pos)),
            b')' => Token::new(TokenKind::RParen, Span::new(start, self.pos)),
            b'[' => Token::new(TokenKind::LBracket, Span::new(start, self.pos)),
            b']' => Token::new(TokenKind::RBracket, Span::new(start, self.pos)),
            b'{' => Token::new(TokenKind::LBrace, Span::new(start, self.pos)),
            b'}' => Token::new(TokenKind::RBrace, Span::new(start, self.pos)),
            b',' => Token::new(TokenKind::Comma, Span::new(start, self.pos)),
            b';' => Token::new(TokenKind::Semicolon, Span::new(start, self.pos)),

            // Operators that might be multi-char
            b'+' => {
                if self.match_byte(b'%') {
                    Token::new(TokenKind::PlusWrap, Span::new(start, self.pos))
                } else if self.match_byte(b'|') {
                    Token::new(TokenKind::PlusSat, Span::new(start, self.pos))
                } else {
                    Token::new(TokenKind::Plus, Span::new(start, self.pos))
                }
            }
            b'-' => {
                if self.match_byte(b'>') {
                    Token::new(TokenKind::Arrow, Span::new(start, self.pos))
                } else if self.match_byte(b'%') {
                    Token::new(TokenKind::MinusWrap, Span::new(start, self.pos))
                } else if self.match_byte(b'|') {
                    Token::new(TokenKind::MinusSat, Span::new(start, self.pos))
                } else {
                    Token::new(TokenKind::Minus, Span::new(start, self.pos))
                }
            }
            b'*' => {
                if self.match_byte(b'%') {
                    Token::new(TokenKind::StarWrap, Span::new(start, self.pos))
                } else {
                    Token::new(TokenKind::Star, Span::new(start, self.pos))
                }
            }
            b'/' => Token::new(TokenKind::Slash, Span::new(start, self.pos)),
            b'%' => Token::new(TokenKind::Percent, Span::new(start, self.pos)),
            b'=' => {
                if self.match_byte(b'=') {
                    Token::new(TokenKind::Eq, Span::new(start, self.pos))
                } else if self.match_byte(b'>') {
                    Token::new(TokenKind::FatArrow, Span::new(start, self.pos))
                } else {
                    Token::new(TokenKind::Assign, Span::new(start, self.pos))
                }
            }
            b'!' => {
                if self.match_byte(b'=') {
                    Token::new(TokenKind::NotEq, Span::new(start, self.pos))
                } else {
                    let err = Token::new(
                        TokenKind::Error("use 'not' instead of '!'".into()),
                        Span::new(start, self.pos),
                    );
                    self.errors.push(err.clone());
                    err
                }
            }
            b'<' => {
                if self.match_byte(b'=') {
                    Token::new(TokenKind::LtEq, Span::new(start, self.pos))
                } else {
                    Token::new(TokenKind::Lt, Span::new(start, self.pos))
                }
            }
            b'>' => {
                if self.match_byte(b'=') {
                    Token::new(TokenKind::GtEq, Span::new(start, self.pos))
                } else {
                    Token::new(TokenKind::Gt, Span::new(start, self.pos))
                }
            }
            b'.' => {
                if self.match_byte(b'.') {
                    Token::new(TokenKind::DotDot, Span::new(start, self.pos))
                } else {
                    Token::new(TokenKind::Dot, Span::new(start, self.pos))
                }
            }
            b':' => {
                if self.match_byte(b':') {
                    Token::new(TokenKind::ColonColon, Span::new(start, self.pos))
                } else {
                    Token::new(TokenKind::Colon, Span::new(start, self.pos))
                }
            }
            b'|' => Token::new(TokenKind::Pipe, Span::new(start, self.pos)),

            // Annotation: @name
            b'@' => self.lex_annotation(start),

            // Optimization hole: ?name
            b'?' => self.lex_opt_hole(start),

            // String literal
            b'"' => self.lex_string(start),

            // Number literal
            c if c.is_ascii_digit() => {
                self.pos = start; // back up to re-read first digit
                self.lex_number()
            }

            // Identifier or keyword
            c if c.is_ascii_alphabetic() || c == b'_' => {
                self.pos = start;
                self.lex_ident_or_keyword()
            }

            // Unknown character — error recovery
            _ => {
                let err = Token::new(
                    TokenKind::Error(format!("unexpected character: '{}'", ch as char)),
                    Span::new(start, self.pos),
                );
                self.errors.push(err.clone());
                err
            }
        }
    }

    fn lex_annotation(&mut self, start: u32) -> Token {
        let name_start = self.pos;
        while !self.is_at_end() && (self.peek().is_ascii_alphanumeric() || self.peek() == b'_') {
            self.advance();
        }
        let name = String::from_utf8_lossy(&self.source[name_start as usize..self.pos as usize]).into_owned();
        if name.is_empty() {
            let err = Token::new(
                TokenKind::Error("expected annotation name after '@'".into()),
                Span::new(start, self.pos),
            );
            self.errors.push(err.clone());
            return err;
        }
        Token::new(TokenKind::Annotation(name), Span::new(start, self.pos))
    }

    fn lex_opt_hole(&mut self, start: u32) -> Token {
        let name_start = self.pos;
        while !self.is_at_end() && (self.peek().is_ascii_alphanumeric() || self.peek() == b'_') {
            self.advance();
        }
        let name = String::from_utf8_lossy(&self.source[name_start as usize..self.pos as usize]).into_owned();
        if name.is_empty() {
            let err = Token::new(
                TokenKind::Error("expected name after '?' for optimization hole".into()),
                Span::new(start, self.pos),
            );
            self.errors.push(err.clone());
            return err;
        }
        Token::new(TokenKind::OptHole(name), Span::new(start, self.pos))
    }

    fn lex_string(&mut self, start: u32) -> Token {
        let mut value = String::new();
        while !self.is_at_end() && self.peek() != b'"' {
            if self.peek() == b'\\' {
                self.advance(); // consume backslash
                if !self.is_at_end() {
                    match self.advance() {
                        b'n' => value.push('\n'),
                        b't' => value.push('\t'),
                        b'r' => value.push('\r'),
                        b'\\' => value.push('\\'),
                        b'"' => value.push('"'),
                        c => {
                            value.push('\\');
                            value.push(c as char);
                        }
                    }
                }
            } else {
                value.push(self.advance() as char);
            }
        }
        if self.is_at_end() {
            let err = Token::new(
                TokenKind::Error("unterminated string literal".into()),
                Span::new(start, self.pos),
            );
            self.errors.push(err.clone());
            return err;
        }
        self.advance(); // consume closing quote
        Token::new(TokenKind::StringLiteral(value), Span::new(start, self.pos))
    }

    fn lex_number(&mut self) -> Token {
        let start = self.pos;
        let mut is_float = false;

        while !self.is_at_end() && self.peek().is_ascii_digit() {
            self.advance();
        }

        // Check for decimal point (but not `..` range operator)
        if !self.is_at_end() && self.peek() == b'.'
            && self.pos + 1 < self.source.len() as u32
            && self.source[(self.pos + 1) as usize] != b'.'
        {
            is_float = true;
            self.advance(); // consume '.'
            while !self.is_at_end() && self.peek().is_ascii_digit() {
                self.advance();
            }
        }

        let text = &self.source[start as usize..self.pos as usize];
        let text_str = std::str::from_utf8(text).unwrap();

        if is_float {
            match text_str.parse::<f64>() {
                Ok(v) => Token::new(TokenKind::FloatLiteral(v), Span::new(start, self.pos)),
                Err(e) => {
                    let err = Token::new(
                        TokenKind::Error(format!("invalid float: {e}")),
                        Span::new(start, self.pos),
                    );
                    self.errors.push(err.clone());
                    err
                }
            }
        } else {
            match text_str.parse::<i128>() {
                Ok(v) => Token::new(TokenKind::IntLiteral(v), Span::new(start, self.pos)),
                Err(e) => {
                    let err = Token::new(
                        TokenKind::Error(format!("invalid integer: {e}")),
                        Span::new(start, self.pos),
                    );
                    self.errors.push(err.clone());
                    err
                }
            }
        }
    }

    fn lex_ident_or_keyword(&mut self) -> Token {
        let start = self.pos;
        while !self.is_at_end() && (self.peek().is_ascii_alphanumeric() || self.peek() == b'_') {
            self.advance();
        }
        let text = String::from_utf8_lossy(&self.source[start as usize..self.pos as usize]).into_owned();

        if let Some(keyword) = TokenKind::keyword_from_str(&text) {
            Token::new(keyword, Span::new(start, self.pos))
        } else {
            Token::new(TokenKind::Ident(text), Span::new(start, self.pos))
        }
    }

    // ── Helpers ─────────────────────────────────────────────────

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace
            while !self.is_at_end() && self.peek().is_ascii_whitespace() {
                self.advance();
            }

            // Skip line comments
            if self.remaining() >= 2 && self.peek() == b'/' && self.source[(self.pos + 1) as usize] == b'/' {
                while !self.is_at_end() && self.peek() != b'\n' {
                    self.advance();
                }
                continue;
            }

            // Skip block comments
            if self.remaining() >= 2 && self.peek() == b'/' && self.source[(self.pos + 1) as usize] == b'*' {
                self.advance(); // /
                self.advance(); // *
                let mut depth: u32 = 1;
                while !self.is_at_end() && depth > 0 {
                    if self.remaining() >= 2 && self.peek() == b'/' && self.source[(self.pos + 1) as usize] == b'*' {
                        self.advance();
                        self.advance();
                        depth += 1;
                    } else if self.remaining() >= 2 && self.peek() == b'*' && self.source[(self.pos + 1) as usize] == b'/' {
                        self.advance();
                        self.advance();
                        depth -= 1;
                    } else {
                        self.advance();
                    }
                }
                continue;
            }

            break;
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.source.len() as u32
    }

    fn remaining(&self) -> u32 {
        (self.source.len() as u32).saturating_sub(self.pos)
    }

    fn peek(&self) -> u8 {
        self.source[self.pos as usize]
    }

    fn advance(&mut self) -> u8 {
        let ch = self.source[self.pos as usize];
        self.pos += 1;
        ch
    }

    fn match_byte(&mut self, expected: u8) -> bool {
        if !self.is_at_end() && self.peek() == expected {
            self.advance();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Vec<TokenKind> {
        let (tokens, _) = Lexer::new(src).tokenize();
        tokens.into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn test_empty() {
        assert_eq!(lex(""), vec![TokenKind::Eof]);
    }

    #[test]
    fn test_basic_tokens() {
        let kinds = lex("( ) [ ] { } , ; + - * /");
        assert_eq!(kinds, vec![
            TokenKind::LParen, TokenKind::RParen,
            TokenKind::LBracket, TokenKind::RBracket,
            TokenKind::LBrace, TokenKind::RBrace,
            TokenKind::Comma, TokenKind::Semicolon,
            TokenKind::Plus, TokenKind::Minus,
            TokenKind::Star, TokenKind::Slash,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_keywords() {
        let kinds = lex("fn let return if else for while in and or not");
        assert_eq!(kinds, vec![
            TokenKind::Fn, TokenKind::Let, TokenKind::Return,
            TokenKind::If, TokenKind::Else, TokenKind::For,
            TokenKind::While, TokenKind::In,
            TokenKind::And, TokenKind::Or, TokenKind::Not,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_type_keywords() {
        let kinds = lex("i32 f64 bool tensor");
        assert_eq!(kinds, vec![
            TokenKind::I32, TokenKind::F64, TokenKind::Bool, TokenKind::Tensor,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_annotation() {
        let kinds = lex("@pure @strategy @intent");
        assert_eq!(kinds, vec![
            TokenKind::Annotation("pure".into()),
            TokenKind::Annotation("strategy".into()),
            TokenKind::Annotation("intent".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_optimization_hole() {
        let kinds = lex("?tile_m ?loop_order");
        assert_eq!(kinds, vec![
            TokenKind::OptHole("tile_m".into()),
            TokenKind::OptHole("loop_order".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_wrapping_operators() {
        let kinds = lex("+% +| -% -| *%");
        assert_eq!(kinds, vec![
            TokenKind::PlusWrap, TokenKind::PlusSat,
            TokenKind::MinusWrap, TokenKind::MinusSat,
            TokenKind::StarWrap,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_arrow_and_fat_arrow() {
        let kinds = lex("-> =>");
        assert_eq!(kinds, vec![
            TokenKind::Arrow, TokenKind::FatArrow,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_numbers() {
        let kinds = lex("42 3.14 0 100");
        assert_eq!(kinds, vec![
            TokenKind::IntLiteral(42),
            TokenKind::FloatLiteral(3.14),
            TokenKind::IntLiteral(0),
            TokenKind::IntLiteral(100),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_string() {
        let kinds = lex(r#""hello world" "escaped\n""#);
        assert_eq!(kinds, vec![
            TokenKind::StringLiteral("hello world".into()),
            TokenKind::StringLiteral("escaped\n".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_line_comment() {
        let kinds = lex("fn // this is a comment\nlet");
        assert_eq!(kinds, vec![TokenKind::Fn, TokenKind::Let, TokenKind::Eof]);
    }

    #[test]
    fn test_block_comment() {
        let kinds = lex("fn /* block comment */ let");
        assert_eq!(kinds, vec![TokenKind::Fn, TokenKind::Let, TokenKind::Eof]);
    }

    #[test]
    fn test_nested_block_comment() {
        let kinds = lex("fn /* outer /* inner */ still comment */ let");
        assert_eq!(kinds, vec![TokenKind::Fn, TokenKind::Let, TokenKind::Eof]);
    }

    #[test]
    fn test_function_signature() {
        let kinds = lex("fn fib(n: i32) -> i64 {");
        assert_eq!(kinds, vec![
            TokenKind::Fn,
            TokenKind::Ident("fib".into()),
            TokenKind::LParen,
            TokenKind::Ident("n".into()),
            TokenKind::Colon,
            TokenKind::I32,
            TokenKind::RParen,
            TokenKind::Arrow,
            TokenKind::I64,
            TokenKind::LBrace,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_annotated_function() {
        let kinds = lex("@pure\n@complexity\nfn matmul(");
        assert_eq!(kinds, vec![
            TokenKind::Annotation("pure".into()),
            TokenKind::Annotation("complexity".into()),
            TokenKind::Fn,
            TokenKind::Ident("matmul".into()),
            TokenKind::LParen,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_error_recovery() {
        // The `!` without `=` should produce an error but lexing continues
        let (tokens, errors) = Lexer::new("fn ! let").tokenize();
        assert_eq!(errors.len(), 1);
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert!(matches!(kinds[0], TokenKind::Fn));
        assert!(matches!(kinds[1], TokenKind::Error(_)));
        assert!(matches!(kinds[2], TokenKind::Let));
    }

    #[test]
    fn test_bool_literals() {
        let kinds = lex("true false");
        assert_eq!(kinds, vec![
            TokenKind::BoolLiteral(true),
            TokenKind::BoolLiteral(false),
            TokenKind::Eof,
        ]);
    }
}
RUST

# ── Crate: axiom-parser (stub) ──────────────────────────────────────
CRATE="$PROJECT_ROOT/crates/axiom-parser"
mkdir -p "$CRATE/src"

cat > "$CRATE/Cargo.toml" << 'EOF'
[package]
name = "axiom-parser"
version.workspace = true
edition.workspace = true
description = "Parser for the AXIOM language — produces typed AST"

[dependencies]
axiom-lexer = { path = "../axiom-lexer" }
thiserror.workspace = true
miette.workspace = true

[dev-dependencies]
insta.workspace = true
EOF

cat > "$CRATE/src/lib.rs" << 'RUST'
//! AXIOM Parser — transforms token stream into a typed AST.
//!
//! TODO(next): Implement AST types and recursive descent parser.
//! Priority: function declarations, let bindings, expressions, annotations.

pub mod ast;

pub use ast::*;
RUST

cat > "$CRATE/src/ast.rs" << 'RUST'
//! AST node definitions for AXIOM.
//!
//! Every node carries a Span. Annotations are first-class typed data.

use axiom_lexer::Span;

/// Top-level module
#[derive(Debug, Clone)]
pub struct Module {
    pub name: Option<Spanned<String>>,
    pub annotations: Vec<Spanned<Annotation>>,
    pub items: Vec<Spanned<Item>>,
}

/// A node with source location
#[derive(Debug, Clone)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}

/// Top-level items
#[derive(Debug, Clone)]
pub enum Item {
    Function(Function),
    Struct(StructDef),
    TypeAlias(TypeAlias),
    Import(ImportDecl),
}

/// Function definition
#[derive(Debug, Clone)]
pub struct Function {
    pub name: Spanned<String>,
    pub annotations: Vec<Spanned<Annotation>>,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    pub body: Block,
}

/// Function parameter
#[derive(Debug, Clone)]
pub struct Param {
    pub name: Spanned<String>,
    pub ty: TypeExpr,
    pub annotations: Vec<Spanned<Annotation>>,
}

/// Type expression
#[derive(Debug, Clone)]
pub enum TypeExpr {
    Named(String),                           // i32, MyStruct
    Tensor(Box<TypeExpr>, Vec<DimExpr>),     // tensor[f32, M, N]
    Array(Box<TypeExpr>, Box<Expr>),         // array[f32, 1024]
    Slice(Box<TypeExpr>),                    // slice[f32]
    Ptr(Box<TypeExpr>),                      // ptr[f32]
    Tuple(Vec<TypeExpr>),                    // (i32, f64)
    Fn(Vec<TypeExpr>, Box<TypeExpr>),        // fn(i32) -> i64
}

/// Dimension expression in tensor types
#[derive(Debug, Clone)]
pub enum DimExpr {
    Const(i64),
    Named(String),       // M, N — generic dimensions
    Dynamic,             // ? — runtime-determined
}

/// Block of statements
#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Spanned<Stmt>>,
}

/// Statements
#[derive(Debug, Clone)]
pub enum Stmt {
    Let {
        name: Spanned<String>,
        ty: TypeExpr,
        value: Expr,
        mutable: bool,
    },
    Assign {
        target: Expr,
        value: Expr,
    },
    Return(Expr),
    If {
        condition: Expr,
        then_block: Block,
        else_block: Option<Block>,
    },
    For {
        var: Spanned<String>,
        var_type: TypeExpr,
        iterable: Expr,
        body: Block,
    },
    While {
        condition: Expr,
        body: Block,
    },
    Expr(Expr),
}

/// Expressions
#[derive(Debug, Clone)]
pub enum Expr {
    IntLiteral(i128),
    FloatLiteral(f64),
    StringLiteral(String),
    BoolLiteral(bool),
    Ident(String),
    OptHole(String),          // ?param — optimization hole
    BinaryOp {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
    },
    Index {
        expr: Box<Expr>,
        indices: Vec<Expr>,
    },
    FieldAccess {
        expr: Box<Expr>,
        field: String,
    },
    MethodCall {
        expr: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    AddWrap, AddSat,
    SubWrap, SubSat,
    MulWrap,
    Eq, NotEq, Lt, Gt, LtEq, GtEq,
    And, Or,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
}

/// Structured annotations — NOT strings
#[derive(Debug, Clone)]
pub enum Annotation {
    Pure,
    Const,
    Inline(InlineHint),
    Complexity(String),          // O(n^2) as string for now, structured later
    Intent(String),
    Module(String),
    Constraint(Vec<(String, AnnotationValue)>),
    Target(Vec<String>),
    Strategy(StrategyBlock),
    Transfer(TransferBlock),
    Vectorizable(Vec<String>),
    Parallel(Vec<String>),
    Layout(LayoutKind),
    Align(u64),
    OptimizationLog(Vec<OptLogEntry>),
    Custom(String, Vec<AnnotationValue>),
}

#[derive(Debug, Clone)]
pub enum InlineHint {
    Always,
    Never,
    Hint,
}

#[derive(Debug, Clone)]
pub enum LayoutKind {
    RowMajor,
    ColMajor,
    Custom(String),
}

#[derive(Debug, Clone)]
pub enum AnnotationValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Ident(String),
    List(Vec<AnnotationValue>),
    Map(Vec<(String, AnnotationValue)>),
}

/// Strategy block — the optimization surface
#[derive(Debug, Clone)]
pub struct StrategyBlock {
    pub entries: Vec<(String, StrategyValue)>,
}

#[derive(Debug, Clone)]
pub enum StrategyValue {
    Hole(String),                           // ?param_name
    Map(Vec<(String, StrategyValue)>),      // { M: ?tile_m, N: ?tile_n }
    Concrete(AnnotationValue),              // Fixed value
}

/// Transfer block — inter-agent handoff
#[derive(Debug, Clone)]
pub struct TransferBlock {
    pub source_agent: Option<String>,
    pub target_agent: Option<String>,
    pub context: Option<String>,
    pub open_questions: Vec<String>,
    pub confidence: Option<(f64, f64)>,     // (correctness, optimality)
}

/// Optimization log entry
#[derive(Debug, Clone)]
pub struct OptLogEntry {
    pub version: String,
    pub params: Vec<(String, AnnotationValue)>,
    pub metrics: Vec<(String, f64)>,
    pub agent: Option<String>,
    pub target: Option<String>,
    pub date: Option<String>,
}

/// Struct definition
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: Spanned<String>,
    pub annotations: Vec<Spanned<Annotation>>,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: Spanned<String>,
    pub ty: TypeExpr,
    pub annotations: Vec<Spanned<Annotation>>,
}

/// Type alias
#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub name: Spanned<String>,
    pub ty: TypeExpr,
}

/// Import declaration
#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub path: Vec<String>,
    pub alias: Option<String>,
}
RUST

# ── Stub crates ──────────────────────────────────────────────────────
for CRATE_NAME in axiom-hir axiom-mir axiom-codegen axiom-optimize; do
    CRATE="$PROJECT_ROOT/crates/$CRATE_NAME"
    mkdir -p "$CRATE/src"
    cat > "$CRATE/Cargo.toml" << EOF
[package]
name = "$CRATE_NAME"
version.workspace = true
edition.workspace = true

[dependencies]
thiserror.workspace = true
EOF
    cat > "$CRATE/src/lib.rs" << EOF
//! $CRATE_NAME — TODO: implement in Phase 1/2
EOF
done

# ── Driver crate ─────────────────────────────────────────────────────
CRATE="$PROJECT_ROOT/crates/axiom-driver"
mkdir -p "$CRATE/src"

cat > "$CRATE/Cargo.toml" << 'EOF'
[package]
name = "axiom-driver"
version.workspace = true
edition.workspace = true

[[bin]]
name = "axiom"
path = "src/main.rs"

[dependencies]
axiom-lexer = { path = "../axiom-lexer" }
axiom-parser = { path = "../axiom-parser" }
clap.workspace = true
miette = { workspace = true, features = ["fancy"] }
tracing.workspace = true
tracing-subscriber.workspace = true
EOF

cat > "$CRATE/src/main.rs" << 'RUST'
//! AXIOM compiler CLI driver.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "axiom", about = "AXIOM — AI eXchange Intermediate Optimization Medium")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile an AXIOM source file
    Compile {
        /// Input .axm file
        input: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,

        /// Emit intermediate representation instead of binary
        #[arg(long, value_parser = ["tokens", "ast", "hir", "mir", "mlir", "llvm-ir"])]
        emit: Option<String>,
    },

    /// Tokenize an AXIOM source file (debug tool)
    Lex {
        /// Input .axm file
        input: String,
    },
}

fn main() -> miette::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Lex { input } => {
            let source = std::fs::read_to_string(&input)
                .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;
            let (tokens, errors) = axiom_lexer::Lexer::new(&source).tokenize();

            for tok in &tokens {
                let text = &source[tok.span.start as usize..tok.span.end as usize];
                println!("{:4}..{:<4} {:?}  {:?}", tok.span.start, tok.span.end, tok.kind, text);
            }

            if !errors.is_empty() {
                eprintln!("\n--- Errors ---");
                for err in &errors {
                    eprintln!("  {:?}", err);
                }
            }

            Ok(())
        }

        Commands::Compile { input, output: _, emit } => {
            let source = std::fs::read_to_string(&input)
                .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;

            match emit.as_deref() {
                Some("tokens") => {
                    let (tokens, _) = axiom_lexer::Lexer::new(&source).tokenize();
                    for tok in &tokens {
                        println!("{:?}", tok);
                    }
                }
                Some(level) => {
                    eprintln!("TODO: --emit={level} not yet implemented");
                }
                None => {
                    eprintln!("TODO: full compilation not yet implemented");
                }
            }

            Ok(())
        }
    }
}
RUST

# ── Sample AXIOM programs ───────────────────────────────────────────
cat > "$PROJECT_ROOT/tests/samples/hello.axm" << 'AXM'
@module hello;
@intent("Print greeting to stdout");

fn main() -> i32 {
    print("Hello from AXIOM!");
    return 0;
}
AXM

cat > "$PROJECT_ROOT/tests/samples/fibonacci.axm" << 'AXM'
@module fibonacci;
@intent("Compute Nth Fibonacci number iteratively");

@pure
@complexity O(n)
fn fib(n: i32) -> i64 {
    if n <= 1 {
        return widen(n);
    }
    let a: i64 = 0;
    let b: i64 = 1;
    for i: i32 in range(2, n + 1) {
        let temp: i64 = b;
        b = a + b;
        a = temp;
    }
    return b;
}

fn main() -> i32 {
    let result: i64 = fib(40);
    print_i64(result);
    return 0;
}
AXM

cat > "$PROJECT_ROOT/tests/samples/matmul_naive.axm" << 'AXM'
@module matmul;
@intent("Dense matrix multiplication — naive baseline for optimization");
@constraint { correctness: "IEEE 754 compliant" };
@target { cpu.simd };

@pure
@complexity O(n^3)
@vectorizable(i, j, k)
fn matmul(
    a: tensor[f32, M, K] @layout(row_major) @align(64),
    b: tensor[f32, K, N] @layout(col_major) @align(64),
) -> tensor[f32, M, N] @layout(row_major) {

    @strategy {
        tiling:   { M: ?tile_m, N: ?tile_n, K: ?tile_k }
        order:    ?loop_order
        parallel: ?parallel_dims
        unroll:   ?unroll_factor
        prefetch: ?prefetch_distance
    }

    let result: tensor[f32, M, N] = tensor.zeros[f32, M, N];

    for i: i32 in range(M) {
        for j: i32 in range(N) {
            let acc: f32 = 0.0;
            for k: i32 in range(K) {
                acc = acc + a[i, k] * b[k, j];
            }
            result[i, j] = acc;
        }
    }

    return result;
}
AXM

# ── DESIGN.md ────────────────────────────────────────────────────────
cat > "$PROJECT_ROOT/DESIGN.md" << 'EOF'
# AXIOM Language Specification v0.1 (Draft)

This is the living specification for the AXIOM language. It evolves as the
implementation progresses. See CLAUDE.md for project structure and conventions.

## Status

- [x] Token types defined
- [x] Lexer implemented with error recovery
- [x] AST node types defined
- [ ] Parser implementation
- [ ] HIR definition and lowering
- [ ] Type checking
- [ ] LLVM codegen
- [ ] Optimization protocol

## Grammar (Informal)

See `spec/grammar.ebnf` (TODO) for the formal grammar.
See `CLAUDE.md` for the syntax rules and type system summary.
EOF

# ── .gitignore ───────────────────────────────────────────────────────
cat > "$PROJECT_ROOT/.gitignore" << 'EOF'
/target
*.swp
*.swo
*~
.DS_Store
EOF

# ── Git init ─────────────────────────────────────────────────────────
cd "$PROJECT_ROOT"
git init -q
git add -A
git commit -q -m "feat: initial AXIOM project scaffold — lexer with tests, AST types, CLI driver"

echo ""
echo "✅ AXIOM project bootstrapped successfully!"
echo ""
echo "   cd $(basename "$PROJECT_ROOT")"
echo "   cargo test          # Run all tests (lexer has 14 tests)"
echo "   cargo build         # Build the axiom CLI"
echo "   cargo run -- lex tests/samples/fibonacci.axm   # Tokenize a sample"
echo ""
echo "   Next: implement the parser (Milestone 1.2)"
echo ""
