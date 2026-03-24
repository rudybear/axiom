//! Lexer implementation for AXIOM.
//!
//! Hand-written lexer with full error recovery.
//! Invalid characters produce Error tokens; lexing always continues.

use crate::token::{Token, TokenKind, Span, IntBase, IntSuffix, FloatSuffix};

/// Tokenizes AXIOM source text into a stream of [`Token`]s.
///
/// The lexer operates on byte slices for performance while requiring
/// valid UTF-8 input. It recovers from errors by producing [`TokenKind::Error`]
/// tokens and continuing to lex subsequent characters.
pub struct Lexer<'src> {
    source: &'src [u8],
    pos: u32,
    errors: Vec<Token>,
    /// Set by `skip_whitespace_and_comments` when an unterminated block
    /// comment is detected. The next call to `next_token` will emit it.
    pending_error: Option<Token>,
}

impl<'src> Lexer<'src> {
    /// Create a new lexer for the given source string.
    pub fn new(source: &'src str) -> Self {
        Self {
            source: source.as_bytes(),
            pos: 0,
            errors: Vec::new(),
            pending_error: None,
        }
    }

    /// Tokenize the entire source, returning all tokens including Eof.
    ///
    /// Returns a tuple of `(tokens, errors)` where `errors` is a subset
    /// of `tokens` containing only the error tokens for convenient access.
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
        // Emit any pending error from unterminated block comment
        if let Some(err) = self.pending_error.take() {
            self.errors.push(err.clone());
            return err;
        }

        self.skip_whitespace_and_comments();

        // Check again after skipping comments (an unterminated comment may set pending_error)
        if let Some(err) = self.pending_error.take() {
            self.errors.push(err.clone());
            return err;
        }

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
            b'^' => Token::new(TokenKind::Caret, Span::new(start, self.pos)),

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
                        b'0' => value.push('\0'),
                        b'x' => {
                            // Hex byte escape: \xNN (exactly 2 hex digits)
                            match self.lex_hex_escape(2) {
                                Ok(byte_val) => {
                                    value.push(byte_val as char);
                                }
                                Err(msg) => {
                                    // Consume rest of string to avoid cascading errors
                                    while !self.is_at_end() && self.peek() != b'"' {
                                        self.advance();
                                    }
                                    if !self.is_at_end() {
                                        self.advance(); // closing quote
                                    }
                                    let err = Token::new(
                                        TokenKind::Error(msg),
                                        Span::new(start, self.pos),
                                    );
                                    self.errors.push(err.clone());
                                    return err;
                                }
                            }
                        }
                        b'u' => {
                            // Unicode escape: \u{NNNN} (1-6 hex digits)
                            match self.lex_unicode_escape() {
                                Ok(ch) => {
                                    value.push(ch);
                                }
                                Err(msg) => {
                                    while !self.is_at_end() && self.peek() != b'"' {
                                        self.advance();
                                    }
                                    if !self.is_at_end() {
                                        self.advance(); // closing quote
                                    }
                                    let err = Token::new(
                                        TokenKind::Error(msg),
                                        Span::new(start, self.pos),
                                    );
                                    self.errors.push(err.clone());
                                    return err;
                                }
                            }
                        }
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

    /// Parse exactly `count` hex digits and return the resulting byte value.
    fn lex_hex_escape(&mut self, count: usize) -> Result<u8, String> {
        let mut val: u8 = 0;
        for _ in 0..count {
            if self.is_at_end() {
                return Err("incomplete hex escape sequence".into());
            }
            let b = self.peek();
            let digit = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(format!("invalid hex digit '{}' in escape sequence", b as char)),
            };
            self.advance();
            val = val * 16 + digit;
        }
        Ok(val)
    }

    /// Parse a unicode escape `\u{NNNN}` (the `\u` has already been consumed).
    fn lex_unicode_escape(&mut self) -> Result<char, String> {
        if self.is_at_end() || self.peek() != b'{' {
            return Err("expected '{' after \\u".into());
        }
        self.advance(); // consume '{'

        let mut val: u32 = 0;
        let mut digit_count = 0;
        while !self.is_at_end() && self.peek() != b'}' {
            let b = self.peek();
            let digit = match b {
                b'0'..=b'9' => (b - b'0') as u32,
                b'a'..=b'f' => (b - b'a') as u32 + 10,
                b'A'..=b'F' => (b - b'A') as u32 + 10,
                _ => return Err(format!("invalid hex digit '{}' in unicode escape", b as char)),
            };
            self.advance();
            digit_count += 1;
            if digit_count > 6 {
                return Err("unicode escape must have at most 6 hex digits".into());
            }
            val = val * 16 + digit;
        }

        if self.is_at_end() {
            return Err("unterminated unicode escape, expected '}'".into());
        }
        self.advance(); // consume '}'

        if digit_count == 0 {
            return Err("unicode escape must have at least 1 hex digit".into());
        }

        char::from_u32(val).ok_or_else(|| format!("invalid unicode scalar value: U+{val:04X}"))
    }

    fn lex_number(&mut self) -> Token {
        let start = self.pos;
        let first = self.advance();
        let mut is_float = false;
        let mut base = IntBase::Decimal;

        // Check for base prefix after leading '0'
        if first == b'0' && !self.is_at_end() {
            match self.peek() {
                b'x' | b'X' => {
                    base = IntBase::Hex;
                    self.advance(); // consume 'x'
                    return self.lex_int_with_base(start, base, is_hex_digit);
                }
                b'b' | b'B' => {
                    base = IntBase::Binary;
                    self.advance(); // consume 'b'
                    return self.lex_int_with_base(start, base, is_binary_digit);
                }
                b'o' | b'O' => {
                    base = IntBase::Octal;
                    self.advance(); // consume 'o'
                    return self.lex_int_with_base(start, base, is_octal_digit);
                }
                _ => {}
            }
        }

        // Decimal integer/float: consume remaining decimal digits + underscores
        let mut had_double_underscore = false;
        let mut had_trailing_underscore = false;
        let mut last_was_underscore = false;
        while !self.is_at_end() && (self.peek().is_ascii_digit() || self.peek() == b'_') {
            if self.peek() == b'_' {
                if last_was_underscore {
                    had_double_underscore = true;
                }
                last_was_underscore = true;
                self.advance();
            } else {
                last_was_underscore = false;
                self.advance();
            }
        }
        if last_was_underscore {
            had_trailing_underscore = true;
        }

        // Check for decimal point (but not `..` range operator)
        if !self.is_at_end() && self.peek() == b'.'
            && self.pos + 1 < self.source.len() as u32
            && self.source[(self.pos + 1) as usize] != b'.'
            // Also make sure what follows the dot is a digit, not an identifier
            && self.source[(self.pos + 1) as usize].is_ascii_digit()
        {
            is_float = true;
            had_trailing_underscore = false;
            self.advance(); // consume '.'
            last_was_underscore = false;
            while !self.is_at_end() && (self.peek().is_ascii_digit() || self.peek() == b'_') {
                if self.peek() == b'_' {
                    if last_was_underscore {
                        had_double_underscore = true;
                    }
                    last_was_underscore = true;
                    self.advance();
                } else {
                    last_was_underscore = false;
                    self.advance();
                }
            }
            if last_was_underscore {
                had_trailing_underscore = true;
            }
        }

        // Check for scientific notation (e/E)
        if !self.is_at_end() && (self.peek() == b'e' || self.peek() == b'E') {
            // Look ahead to ensure this is actually exponent, not a suffix/ident
            let saved = self.pos;
            self.advance(); // consume e/E
            let mut has_exp_digits = false;

            // Optional sign
            if !self.is_at_end() && (self.peek() == b'+' || self.peek() == b'-') {
                self.advance();
            }

            // Must have at least one digit
            if !self.is_at_end() && self.peek().is_ascii_digit() {
                has_exp_digits = true;
                while !self.is_at_end() && (self.peek().is_ascii_digit() || self.peek() == b'_') {
                    self.advance();
                }
            }

            if has_exp_digits {
                is_float = true;
            } else {
                // Not a valid exponent — back up
                self.pos = saved;
            }
        }

        // Validate underscore rules
        if had_double_underscore {
            let err = Token::new(
                TokenKind::Error("consecutive underscores in numeric literal".into()),
                Span::new(start, self.pos),
            );
            self.errors.push(err.clone());
            return err;
        }

        // Build the numeric text (strip underscores)
        let raw = &self.source[start as usize..self.pos as usize];
        let text: String = raw.iter().filter(|&&b| b != b'_').map(|&b| b as char).collect();

        // Check for trailing underscore: the last char before suffix area
        if had_trailing_underscore {
            let err = Token::new(
                TokenKind::Error("trailing underscore in numeric literal".into()),
                Span::new(start, self.pos),
            );
            self.errors.push(err.clone());
            return err;
        }

        // Check for width suffix
        if is_float {
            let float_suffix = self.try_consume_float_suffix();
            // Float with int suffix is an error
            if float_suffix.is_none() {
                if let Some(_int_suf) = self.peek_int_suffix() {
                    let suf_len = self.int_suffix_len();
                    for _ in 0..suf_len { self.advance(); }
                    let err = Token::new(
                        TokenKind::Error("integer suffix on float literal".into()),
                        Span::new(start, self.pos),
                    );
                    self.errors.push(err.clone());
                    return err;
                }
            }
            match text.parse::<f64>() {
                Ok(v) => Token::new(
                    TokenKind::FloatLiteral { value: v, suffix: float_suffix },
                    Span::new(start, self.pos),
                ),
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
            // Check for float suffix on integer (e.g. 42f32 -> FloatLiteral)
            let float_suffix = self.try_consume_float_suffix();
            if let Some(fs) = float_suffix {
                match text.parse::<f64>() {
                    Ok(v) => Token::new(
                        TokenKind::FloatLiteral { value: v, suffix: Some(fs) },
                        Span::new(start, self.pos),
                    ),
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
                let int_suffix = self.try_consume_int_suffix();
                match text.parse::<i128>() {
                    Ok(v) => Token::new(
                        TokenKind::IntLiteral { value: v, suffix: int_suffix, base },
                        Span::new(start, self.pos),
                    ),
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
    }

    /// Lex an integer literal with a specific base (hex, binary, octal).
    /// The prefix (e.g. `0x`) has already been consumed.
    fn lex_int_with_base(
        &mut self,
        start: u32,
        base: IntBase,
        is_valid_digit: fn(u8) -> bool,
    ) -> Token {
        let digit_start = self.pos;
        let mut last_was_underscore = false;
        let mut had_double_underscore = false;

        while !self.is_at_end() && (is_valid_digit(self.peek()) || self.peek() == b'_') {
            if self.peek() == b'_' {
                if last_was_underscore {
                    had_double_underscore = true;
                }
                last_was_underscore = true;
            } else {
                last_was_underscore = false;
            }
            self.advance();
        }

        // No digits after prefix
        if self.pos == digit_start {
            let base_name = match base {
                IntBase::Hex => "hexadecimal",
                IntBase::Binary => "binary",
                IntBase::Octal => "octal",
                IntBase::Decimal => "decimal",
            };
            let err = Token::new(
                TokenKind::Error(format!("expected {base_name} digits after prefix")),
                Span::new(start, self.pos),
            );
            self.errors.push(err.clone());
            return err;
        }

        if had_double_underscore {
            let err = Token::new(
                TokenKind::Error("consecutive underscores in numeric literal".into()),
                Span::new(start, self.pos),
            );
            self.errors.push(err.clone());
            return err;
        }

        if last_was_underscore {
            let err = Token::new(
                TokenKind::Error("trailing underscore in numeric literal".into()),
                Span::new(start, self.pos),
            );
            self.errors.push(err.clone());
            return err;
        }

        // Build digit string (strip underscores, skip prefix)
        let digits: String = self.source[digit_start as usize..self.pos as usize]
            .iter()
            .filter(|&&b| b != b'_')
            .map(|&b| b as char)
            .collect();

        let radix = match base {
            IntBase::Hex => 16,
            IntBase::Binary => 2,
            IntBase::Octal => 8,
            IntBase::Decimal => 10,
        };

        let int_suffix = self.try_consume_int_suffix();

        match i128::from_str_radix(&digits, radix) {
            Ok(v) => Token::new(
                TokenKind::IntLiteral { value: v, suffix: int_suffix, base },
                Span::new(start, self.pos),
            ),
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

    /// Try to consume a float suffix (f16, bf16, f32, f64) at the current position.
    /// Only consumes if the suffix is immediately followed by a non-alphanumeric character
    /// (or EOF), so `f16x` would NOT be consumed as a suffix.
    fn try_consume_float_suffix(&mut self) -> Option<FloatSuffix> {
        let remaining = &self.source[self.pos as usize..];

        // Check bf16 first (longest match)
        if remaining.len() >= 4
            && &remaining[..4] == b"bf16"
            && !remaining.get(4).is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'_')
        {
            self.pos += 4;
            return Some(FloatSuffix::Bf16);
        }

        let suffixes: &[(&[u8], FloatSuffix)] = &[
            (b"f16", FloatSuffix::F16),
            (b"f32", FloatSuffix::F32),
            (b"f64", FloatSuffix::F64),
        ];

        for &(pat, suf) in suffixes {
            if remaining.len() >= pat.len()
                && &remaining[..pat.len()] == pat
                && !remaining.get(pat.len()).is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'_')
            {
                self.pos += pat.len() as u32;
                return Some(suf);
            }
        }

        None
    }

    /// Try to consume an integer suffix (i8, i16, ..., u128) at the current position.
    fn try_consume_int_suffix(&mut self) -> Option<IntSuffix> {
        let remaining = &self.source[self.pos as usize..];

        let suffixes: &[(&[u8], IntSuffix)] = &[
            // Longer suffixes first to avoid prefix matching issues
            (b"i128", IntSuffix::I128),
            (b"i16", IntSuffix::I16),
            (b"i32", IntSuffix::I32),
            (b"i64", IntSuffix::I64),
            (b"i8", IntSuffix::I8),
            (b"u128", IntSuffix::U128),
            (b"u16", IntSuffix::U16),
            (b"u32", IntSuffix::U32),
            (b"u64", IntSuffix::U64),
            (b"u8", IntSuffix::U8),
        ];

        for &(pat, suf) in suffixes {
            if remaining.len() >= pat.len()
                && &remaining[..pat.len()] == pat
                && !remaining.get(pat.len()).is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'_')
            {
                self.pos += pat.len() as u32;
                return Some(suf);
            }
        }

        None
    }

    /// Peek at what integer suffix is next without consuming.
    fn peek_int_suffix(&self) -> Option<IntSuffix> {
        let remaining = &self.source[self.pos as usize..];

        let suffixes: &[(&[u8], IntSuffix)] = &[
            (b"i128", IntSuffix::I128),
            (b"i16", IntSuffix::I16),
            (b"i32", IntSuffix::I32),
            (b"i64", IntSuffix::I64),
            (b"i8", IntSuffix::I8),
            (b"u128", IntSuffix::U128),
            (b"u16", IntSuffix::U16),
            (b"u32", IntSuffix::U32),
            (b"u64", IntSuffix::U64),
            (b"u8", IntSuffix::U8),
        ];

        for &(pat, suf) in suffixes {
            if remaining.len() >= pat.len()
                && &remaining[..pat.len()] == pat
                && !remaining.get(pat.len()).is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Some(suf);
            }
        }

        None
    }

    /// Return the byte length of the int suffix at current position.
    fn int_suffix_len(&self) -> u32 {
        let remaining = &self.source[self.pos as usize..];
        let suffixes: &[&[u8]] = &[
            b"i128", b"u128", b"i16", b"i32", b"i64", b"u16", b"u32", b"u64", b"i8", b"u8",
        ];
        for pat in suffixes {
            if remaining.len() >= pat.len()
                && &remaining[..pat.len()] == *pat
                && !remaining.get(pat.len()).is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return pat.len() as u32;
            }
        }
        0
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

            // Skip block comments (with unterminated detection)
            if self.remaining() >= 2 && self.peek() == b'/' && self.source[(self.pos + 1) as usize] == b'*' {
                let comment_start = self.pos;
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

                // Unterminated block comment
                if depth > 0 {
                    let err = Token::new(
                        TokenKind::Error("unterminated block comment".into()),
                        Span::new(comment_start, self.pos),
                    );
                    self.pending_error = Some(err);
                    return;
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

fn is_hex_digit(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn is_binary_digit(b: u8) -> bool {
    b == b'0' || b == b'1'
}

fn is_octal_digit(b: u8) -> bool {
    (b'0'..=b'7').contains(&b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::{IntBase, IntSuffix, FloatSuffix, LineIndex};

    fn lex(src: &str) -> Vec<TokenKind> {
        let (tokens, _) = Lexer::new(src).tokenize();
        tokens.into_iter().map(|t| t.kind).collect()
    }

    fn lex_with_errors(src: &str) -> (Vec<TokenKind>, Vec<Token>) {
        let (tokens, errors) = Lexer::new(src).tokenize();
        let kinds = tokens.into_iter().map(|t| t.kind).collect();
        (kinds, errors)
    }

    // ── Original tests (updated for new IntLiteral/FloatLiteral shape) ──

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
            TokenKind::int(42),
            TokenKind::float(3.14),
            TokenKind::int(0),
            TokenKind::int(100),
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

    // ── New feature tests ──────────────────────────────────────

    #[test]
    fn test_caret_operator() {
        let kinds = lex("n^3");
        assert_eq!(kinds, vec![
            TokenKind::Ident("n".into()),
            TokenKind::Caret,
            TokenKind::int(3),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_hex_literal() {
        let kinds = lex("0xFF 0x1A2B");
        assert_eq!(kinds, vec![
            TokenKind::IntLiteral { value: 255, suffix: None, base: IntBase::Hex },
            TokenKind::IntLiteral { value: 0x1A2B, suffix: None, base: IntBase::Hex },
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_hex_literal_lowercase() {
        let kinds = lex("0xff 0xab");
        assert_eq!(kinds, vec![
            TokenKind::IntLiteral { value: 255, suffix: None, base: IntBase::Hex },
            TokenKind::IntLiteral { value: 0xab, suffix: None, base: IntBase::Hex },
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_binary_literal() {
        let kinds = lex("0b1010 0b11111111");
        assert_eq!(kinds, vec![
            TokenKind::IntLiteral { value: 10, suffix: None, base: IntBase::Binary },
            TokenKind::IntLiteral { value: 255, suffix: None, base: IntBase::Binary },
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_octal_literal() {
        let kinds = lex("0o777 0o12");
        assert_eq!(kinds, vec![
            TokenKind::IntLiteral { value: 511, suffix: None, base: IntBase::Octal },
            TokenKind::IntLiteral { value: 10, suffix: None, base: IntBase::Octal },
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_underscore_in_numbers() {
        let kinds = lex("1_000_000 0xFF_FF 0b1010_0101");
        assert_eq!(kinds, vec![
            TokenKind::int(1_000_000),
            TokenKind::IntLiteral { value: 0xFFFF, suffix: None, base: IntBase::Hex },
            TokenKind::IntLiteral { value: 0b1010_0101, suffix: None, base: IntBase::Binary },
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_scientific_notation() {
        let kinds = lex("1e10 3.14e-2 2.5E+3");
        assert_eq!(kinds, vec![
            TokenKind::float(1e10),
            TokenKind::float(3.14e-2),
            TokenKind::float(2.5E+3),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_int_suffix() {
        let kinds = lex("42i32 100u64 0i8");
        assert_eq!(kinds, vec![
            TokenKind::IntLiteral { value: 42, suffix: Some(IntSuffix::I32), base: IntBase::Decimal },
            TokenKind::IntLiteral { value: 100, suffix: Some(IntSuffix::U64), base: IntBase::Decimal },
            TokenKind::IntLiteral { value: 0, suffix: Some(IntSuffix::I8), base: IntBase::Decimal },
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_float_suffix() {
        let kinds = lex("3.14f32 1.0f64");
        assert_eq!(kinds, vec![
            TokenKind::FloatLiteral { value: 3.14, suffix: Some(FloatSuffix::F32) },
            TokenKind::FloatLiteral { value: 1.0, suffix: Some(FloatSuffix::F64) },
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_float_suffix_f16_bf16() {
        let kinds = lex("1.0f16 2.0bf16");
        assert_eq!(kinds, vec![
            TokenKind::FloatLiteral { value: 1.0, suffix: Some(FloatSuffix::F16) },
            TokenKind::FloatLiteral { value: 2.0, suffix: Some(FloatSuffix::Bf16) },
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_int_with_float_suffix() {
        // `42f32` should produce FloatLiteral(42.0, Some(F32))
        let kinds = lex("42f32");
        assert_eq!(kinds, vec![
            TokenKind::FloatLiteral { value: 42.0, suffix: Some(FloatSuffix::F32) },
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_hex_with_suffix() {
        let kinds = lex("0xFFu8");
        assert_eq!(kinds, vec![
            TokenKind::IntLiteral { value: 255, suffix: Some(IntSuffix::U8), base: IntBase::Hex },
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_string_escape_null() {
        let kinds = lex(r#""\0""#);
        assert_eq!(kinds, vec![
            TokenKind::StringLiteral("\0".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_string_escape_hex() {
        let kinds = lex(r#""\x41""#);
        assert_eq!(kinds, vec![
            TokenKind::StringLiteral("A".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_string_escape_unicode() {
        let kinds = lex(r#""\u{41}""#);
        assert_eq!(kinds, vec![
            TokenKind::StringLiteral("A".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_string_escape_unicode_emoji() {
        let kinds = lex(r#""\u{1F600}""#);
        assert_eq!(kinds[0], TokenKind::StringLiteral("\u{1F600}".into()));
    }

    #[test]
    fn test_unterminated_block_comment() {
        let (kinds, errors) = lex_with_errors("fn /* unterminated");
        assert_eq!(errors.len(), 1);
        assert!(matches!(&kinds[0], TokenKind::Fn));
        assert!(matches!(&kinds[1], TokenKind::Error(msg) if msg == "unterminated block comment"));
        assert!(matches!(&kinds[2], TokenKind::Eof));
    }

    #[test]
    fn test_invalid_hex_digit() {
        let (kinds, errors) = lex_with_errors("0xGG");
        assert_eq!(errors.len(), 1);
        assert!(matches!(&kinds[0], TokenKind::Error(_)));
    }

    #[test]
    fn test_invalid_binary_digit() {
        let (kinds, errors) = lex_with_errors("0b23");
        assert_eq!(errors.len(), 1);
        // 0b has no valid digits (2 and 3 are not binary), so we get an error
        // Actually: 0b is consumed as prefix, then no valid binary digits → error
        assert!(matches!(&kinds[0], TokenKind::Error(_)));
    }

    #[test]
    fn test_consecutive_underscores() {
        let (kinds, errors) = lex_with_errors("1__0");
        assert_eq!(errors.len(), 1);
        assert!(matches!(&kinds[0], TokenKind::Error(msg) if msg.contains("consecutive underscores")));
    }

    #[test]
    fn test_trailing_underscore() {
        let (kinds, errors) = lex_with_errors("42_");
        assert_eq!(errors.len(), 1);
        assert!(matches!(&kinds[0], TokenKind::Error(msg) if msg.contains("trailing underscore")));
    }

    #[test]
    fn test_zero_alone() {
        let kinds = lex("0");
        assert_eq!(kinds, vec![TokenKind::int(0), TokenKind::Eof]);
    }

    #[test]
    fn test_zero_dot_five() {
        let kinds = lex("0.5");
        assert_eq!(kinds, vec![TokenKind::float(0.5), TokenKind::Eof]);
    }

    #[test]
    fn test_zero_dotdot() {
        let kinds = lex("0..5");
        assert_eq!(kinds, vec![
            TokenKind::int(0),
            TokenKind::DotDot,
            TokenKind::int(5),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_empty_base_prefix() {
        // 0x with no digits
        let (kinds, errors) = lex_with_errors("0x");
        assert_eq!(errors.len(), 1);
        assert!(matches!(&kinds[0], TokenKind::Error(msg) if msg.contains("hexadecimal digits")));

        // 0b with no digits
        let (kinds, errors) = lex_with_errors("0b");
        assert_eq!(errors.len(), 1);
        assert!(matches!(&kinds[0], TokenKind::Error(msg) if msg.contains("binary digits")));

        // 0o with no digits
        let (kinds, errors) = lex_with_errors("0o");
        assert_eq!(errors.len(), 1);
        assert!(matches!(&kinds[0], TokenKind::Error(msg) if msg.contains("octal digits")));
    }

    #[test]
    fn test_scientific_notation_edge_cases() {
        // `1e` with no exponent digits — 'e' is not consumed, becomes Ident
        let kinds = lex("1e");
        assert_eq!(kinds, vec![
            TokenKind::int(1),
            TokenKind::Ident("e".into()),
            TokenKind::Eof,
        ]);

        // `1e+` with sign but no digits
        let kinds = lex("1e+");
        assert_eq!(kinds, vec![
            TokenKind::int(1),
            TokenKind::Ident("e".into()),
            TokenKind::Plus,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_partial_suffix_not_consumed() {
        // `42i3` — "i3" is not a valid suffix, should be Ident
        let kinds = lex("42i3");
        assert_eq!(kinds, vec![
            TokenKind::int(42),
            TokenKind::Ident("i3".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_number_followed_by_ident() {
        // `42foo` — no valid suffix, "foo" is an ident
        let kinds = lex("42foo");
        assert_eq!(kinds, vec![
            TokenKind::int(42),
            TokenKind::Ident("foo".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_float_with_int_suffix_error() {
        // `3.14i32` — float with integer suffix is an error
        let (kinds, errors) = lex_with_errors("3.14i32");
        assert_eq!(errors.len(), 1);
        assert!(matches!(&kinds[0], TokenKind::Error(msg) if msg.contains("integer suffix on float")));
    }

    #[test]
    fn test_span_accuracy() {
        let (tokens, _) = Lexer::new("fn fib").tokenize();
        // "fn" at positions 0..2
        assert_eq!(tokens[0].span, Span::new(0, 2));
        assert_eq!(tokens[0].kind, TokenKind::Fn);
        // "fib" at positions 3..6
        assert_eq!(tokens[1].span, Span::new(3, 6));
        assert_eq!(tokens[1].kind, TokenKind::Ident("fib".into()));
    }

    #[test]
    fn test_all_type_keywords() {
        let kinds = lex("i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f16 bf16 f32 f64 bool tensor array slice ptr readonly_ptr writeonly_ptr");
        assert_eq!(kinds, vec![
            TokenKind::I8, TokenKind::I16, TokenKind::I32, TokenKind::I64, TokenKind::I128,
            TokenKind::U8, TokenKind::U16, TokenKind::U32, TokenKind::U64, TokenKind::U128,
            TokenKind::F16, TokenKind::Bf16, TokenKind::F32, TokenKind::F64,
            TokenKind::Bool, TokenKind::Tensor, TokenKind::Array, TokenKind::Slice, TokenKind::Ptr,
            TokenKind::ReadonlyPtr, TokenKind::WriteonlyPtr,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_all_keywords() {
        let kinds = lex("fn let mut return if else for while in struct type module import pub unsafe and or not widen narrow truncate");
        assert_eq!(kinds, vec![
            TokenKind::Fn, TokenKind::Let, TokenKind::Mut, TokenKind::Return,
            TokenKind::If, TokenKind::Else, TokenKind::For, TokenKind::While, TokenKind::In,
            TokenKind::Struct, TokenKind::Type, TokenKind::Module, TokenKind::Import,
            TokenKind::Pub, TokenKind::Unsafe,
            TokenKind::And, TokenKind::Or, TokenKind::Not,
            TokenKind::Widen, TokenKind::Narrow, TokenKind::Truncate,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_span_is_empty() {
        let span = Span::new(5, 5);
        assert!(span.is_empty());
        assert_eq!(span.len(), 0);

        let span2 = Span::new(0, 3);
        assert!(!span2.is_empty());
        assert_eq!(span2.len(), 3);
    }

    #[test]
    fn test_line_index() {
        let idx = LineIndex::new("hello\nworld\nfoo");
        assert_eq!(idx.line_col(0), (0, 0));    // 'h'
        assert_eq!(idx.line_col(5), (0, 5));    // '\n'
        assert_eq!(idx.line_col(6), (1, 0));    // 'w'
        assert_eq!(idx.line_col(11), (1, 5));   // '\n'
        assert_eq!(idx.line_col(12), (2, 0));   // 'f'
        assert_eq!(idx.line_count(), 3);
    }

    #[test]
    fn test_line_index_empty() {
        let idx = LineIndex::new("");
        assert_eq!(idx.line_col(0), (0, 0));
        assert_eq!(idx.line_count(), 1);
    }

    #[test]
    fn test_sample_hello_no_errors() {
        let source = std::fs::read_to_string("../../tests/samples/hello.axm")
            .expect("hello.axm should exist");
        let (_, errors) = Lexer::new(&source).tokenize();
        assert!(errors.is_empty(), "hello.axm produced {} error(s): {:?}", errors.len(), errors);
    }

    #[test]
    fn test_sample_fibonacci_no_errors() {
        let source = std::fs::read_to_string("../../tests/samples/fibonacci.axm")
            .expect("fibonacci.axm should exist");
        let (_, errors) = Lexer::new(&source).tokenize();
        assert!(errors.is_empty(), "fibonacci.axm produced {} error(s): {:?}", errors.len(), errors);
    }

    #[test]
    fn test_sample_matmul_no_errors() {
        let source = std::fs::read_to_string("../../tests/samples/matmul_naive.axm")
            .expect("matmul_naive.axm should exist");
        let (_, errors) = Lexer::new(&source).tokenize();
        assert!(errors.is_empty(), "matmul_naive.axm produced {} error(s): {:?}", errors.len(), errors);
    }

    #[test]
    fn test_scientific_notation_with_suffix() {
        let kinds = lex("1.5e2f32");
        assert_eq!(kinds, vec![
            TokenKind::FloatLiteral { value: 1.5e2, suffix: Some(FloatSuffix::F32) },
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_hex_underscore_trailing() {
        let (_, errors) = lex_with_errors("0xFF_");
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_hex_underscore_double() {
        let (_, errors) = lex_with_errors("0xF__F");
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_i128_suffix() {
        let kinds = lex("42i128 7u128");
        assert_eq!(kinds, vec![
            TokenKind::IntLiteral { value: 42, suffix: Some(IntSuffix::I128), base: IntBase::Decimal },
            TokenKind::IntLiteral { value: 7, suffix: Some(IntSuffix::U128), base: IntBase::Decimal },
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_integer_overflow() {
        // A number that overflows i128
        let huge = "999999999999999999999999999999999999999999";
        let (kinds, errors) = lex_with_errors(huge);
        assert_eq!(errors.len(), 1);
        assert!(matches!(&kinds[0], TokenKind::Error(msg) if msg.contains("invalid integer")));
    }

    #[test]
    fn test_zero_point_identifier() {
        // `0.method` should be IntLiteral(0) + Dot + Ident("method")
        let kinds = lex("0.method");
        assert_eq!(kinds, vec![
            TokenKind::int(0),
            TokenKind::Dot,
            TokenKind::Ident("method".into()),
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn test_only_whitespace() {
        let kinds = lex("   \n\t  \r\n  ");
        assert_eq!(kinds, vec![TokenKind::Eof]);
    }

    #[test]
    fn test_suffix_space_separation() {
        // `42 i32` is IntLiteral(42) then keyword I32 (space separates)
        let kinds = lex("42 i32");
        assert_eq!(kinds, vec![
            TokenKind::int(42),
            TokenKind::I32,
            TokenKind::Eof,
        ]);
    }
}
