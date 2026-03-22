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
