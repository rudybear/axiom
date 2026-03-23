//! Recursive descent parser with Pratt expression parsing for AXIOM.
//!
//! The parser consumes a `Vec<Token>` from [`axiom_lexer::Lexer`] and produces
//! a typed [`Module`] AST. It recovers from errors by skipping to synchronization
//! points and collecting all errors rather than stopping at the first one.
//!
//! # Usage
//!
//! ```
//! use axiom_parser::parse;
//!
//! let result = parse("@module hello;\nfn main() -> i32 { return 0; }");
//! assert!(!result.has_errors());
//! ```

use axiom_lexer::{Lexer, Span, Token, TokenKind};

use crate::ast::{
    Annotation, AnnotationValue, BinOp, Block, DimExpr, Expr, ExternFunction, Function,
    ImportDecl, InlineHint, Item, LayoutKind, Module, Param, Spanned, StrategyBlock,
    StrategyValue, StructDef, StructField, Stmt, TransferBlock, TypeAlias, TypeExpr, UnaryOp,
};
use crate::error::{span_to_source_span, ParseError, ParseResult};

/// Parse an AXIOM source string into a [`Module`] AST.
///
/// This is the main entry point for the parser. It lexes the source text and
/// then runs the recursive descent parser, collecting all errors along the way.
///
/// # Examples
///
/// ```
/// let result = axiom_parser::parse("fn main() -> i32 { return 0; }");
/// assert!(!result.has_errors());
/// assert_eq!(result.module.items.len(), 1);
/// ```
pub fn parse(source: &str) -> ParseResult {
    let (tokens, lex_errors) = Lexer::new(source).tokenize();
    let mut parser = Parser::new(source, tokens);

    // Forward lexer errors
    for err_tok in &lex_errors {
        if let TokenKind::Error(msg) = &err_tok.kind {
            parser.errors.push(ParseError::LexerError {
                message: msg.clone(),
                span: span_to_source_span(err_tok.span),
            });
        }
    }

    let module = parser.parse_module();
    ParseResult {
        module,
        errors: parser.errors,
    }
}

/// Recursive descent parser for AXIOM source.
pub(crate) struct Parser<'src> {
    source: &'src str,
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ParseError>,
    depth: u32,
}

/// Maximum nesting depth to prevent stack overflow on adversarial input.
const MAX_DEPTH: u32 = 256;

impl<'src> Parser<'src> {
    /// Create a new parser from source text and a pre-lexed token stream.
    pub(crate) fn new(source: &'src str, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            pos: 0,
            errors: Vec::new(),
            depth: 0,
        }
    }

    // ── Token navigation ───────────────────────────────────────────────

    /// Peek at the current token's kind without consuming it.
    fn peek(&self) -> &TokenKind {
        self.tokens
            .get(self.pos)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    /// Peek at the token `n` positions ahead.
    fn peek_nth(&self, n: usize) -> &TokenKind {
        self.tokens
            .get(self.pos + n)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    /// Get the span of the current token.
    fn current_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span)
            .unwrap_or_else(|| {
                // EOF span: point at end of last token or start of source
                self.tokens
                    .last()
                    .map(|t| t.span)
                    .unwrap_or(Span::new(0, 0))
            })
    }

    /// Get the span of the previous token (for error reporting after advance).
    fn prev_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span
        } else {
            Span::new(0, 0)
        }
    }

    /// Advance to the next token, returning the consumed token.
    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos];
        if tok.kind != TokenKind::Eof {
            self.pos += 1;
        }
        tok
    }

    /// Check if we have reached the end of the token stream.
    fn at_end(&self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    /// Check if the current token matches the given kind (discriminant only).
    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(kind)
    }

    /// Check if the current token is an `Ident` with a specific name.
    fn check_ident(&self, name: &str) -> bool {
        matches!(self.peek(), TokenKind::Ident(s) if s == name)
    }

    /// Consume the current token if it matches the given kind (discriminant).
    /// Returns `true` on match.
    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Consume the current token if it matches, otherwise record an error.
    /// Returns the token's span on success, `None` on failure.
    fn expect(&mut self, kind: &TokenKind, expected_desc: &str) -> Option<Span> {
        if self.check(kind) {
            let span = self.current_span();
            self.advance();
            Some(span)
        } else if self.at_end() {
            self.errors.push(ParseError::UnexpectedEof {
                expected: expected_desc.to_string(),
                span: span_to_source_span(self.current_span()),
            });
            None
        } else {
            self.errors.push(ParseError::UnexpectedToken {
                expected: expected_desc.to_string(),
                found: format!("{:?}", self.peek()),
                span: span_to_source_span(self.current_span()),
            });
            None
        }
    }

    /// Consume and return the identifier string if the current token is `Ident`.
    fn eat_ident(&mut self) -> Option<(String, Span)> {
        if let TokenKind::Ident(_) = self.peek() {
            let span = self.current_span();
            let tok = self.advance().clone();
            if let TokenKind::Ident(name) = tok.kind {
                return Some((name, span));
            }
        }
        None
    }

    /// Expect an identifier, recording an error if not found.
    fn expect_ident(&mut self, context: &str) -> Option<(String, Span)> {
        if let Some(result) = self.eat_ident() {
            Some(result)
        } else if self.at_end() {
            self.errors.push(ParseError::UnexpectedEof {
                expected: format!("identifier ({context})"),
                span: span_to_source_span(self.current_span()),
            });
            None
        } else {
            // Check for type keywords used as identifiers (common in types)
            if let Some(name) = self.try_eat_type_keyword_as_ident() {
                return Some(name);
            }
            self.errors.push(ParseError::UnexpectedToken {
                expected: format!("identifier ({context})"),
                found: format!("{:?}", self.peek()),
                span: span_to_source_span(self.current_span()),
            });
            None
        }
    }

    /// Try to eat a type keyword token and return it as an identifier string.
    /// This allows using type keywords like `i32`, `f64` etc. as type names.
    fn try_eat_type_keyword_as_ident(&mut self) -> Option<(String, Span)> {
        let name = match self.peek() {
            TokenKind::I8 => "i8",
            TokenKind::I16 => "i16",
            TokenKind::I32 => "i32",
            TokenKind::I64 => "i64",
            TokenKind::I128 => "i128",
            TokenKind::U8 => "u8",
            TokenKind::U16 => "u16",
            TokenKind::U32 => "u32",
            TokenKind::U64 => "u64",
            TokenKind::U128 => "u128",
            TokenKind::F16 => "f16",
            TokenKind::Bf16 => "bf16",
            TokenKind::F32 => "f32",
            TokenKind::F64 => "f64",
            TokenKind::Bool => "bool",
            _ => return None,
        };
        let span = self.current_span();
        self.advance();
        Some((name.to_string(), span))
    }

    /// Consume an optional semicolon. Returns true if consumed.
    fn eat_semicolon(&mut self) -> bool {
        self.eat(&TokenKind::Semicolon)
    }

    /// Expect a semicolon with smart recovery: if the next token starts a
    /// new statement, record an error but do NOT consume that token.
    fn expect_semicolon(&mut self) {
        if !self.eat(&TokenKind::Semicolon) {
            // Check if the next token could start a new statement
            let can_recover = matches!(
                self.peek(),
                TokenKind::Let
                    | TokenKind::Return
                    | TokenKind::If
                    | TokenKind::For
                    | TokenKind::While
                    | TokenKind::RBrace
                    | TokenKind::Fn
                    | TokenKind::Eof
            ) || matches!(self.peek(), TokenKind::Annotation(_));

            if can_recover {
                self.errors.push(ParseError::MissingSemicolon {
                    span: span_to_source_span(self.prev_span()),
                });
            } else {
                self.errors.push(ParseError::MissingSemicolon {
                    span: span_to_source_span(self.prev_span()),
                });
                // Skip the unexpected token
                self.advance();
            }
        }
    }

    /// Skip tokens that are lexer errors, recording them.
    fn skip_error_tokens(&mut self) {
        while let TokenKind::Error(msg) = self.peek() {
            let msg = msg.clone();
            let span = self.current_span();
            self.errors.push(ParseError::LexerError {
                message: msg,
                span: span_to_source_span(span),
            });
            self.advance();
        }
    }

    // ── Error recovery / synchronization ──────────────────────────────

    /// Skip tokens until we reach a statement-level synchronization point.
    fn synchronize_stmt(&mut self) {
        loop {
            match self.peek() {
                TokenKind::Semicolon => {
                    self.advance();
                    return;
                }
                TokenKind::RBrace
                | TokenKind::Let
                | TokenKind::Return
                | TokenKind::If
                | TokenKind::For
                | TokenKind::While
                | TokenKind::Fn
                | TokenKind::Eof => return,
                _ => {
                    if matches!(self.peek(), TokenKind::Annotation(_)) {
                        return;
                    }
                    self.advance();
                }
            }
        }
    }

    /// Skip tokens until we reach an item-level synchronization point.
    fn synchronize_item(&mut self) {
        loop {
            match self.peek() {
                TokenKind::Fn
                | TokenKind::Extern
                | TokenKind::Struct
                | TokenKind::Type
                | TokenKind::Import
                | TokenKind::Eof => return,
                _ => {
                    if matches!(self.peek(), TokenKind::Annotation(_)) {
                        return;
                    }
                    self.advance();
                }
            }
        }
    }

    // ── Top-level parsing ─────────────────────────────────────────────

    /// Parse a complete AXIOM module.
    ///
    /// Module-level annotations (like `@module name;`, `@intent("...");`,
    /// `@constraint { ... };`, `@target { ... };`) are terminated by `;`.
    /// Annotations that appear without a trailing `;` before an item keyword
    /// belong to that item.
    pub(crate) fn parse_module(&mut self) -> Module {
        let mut module_name: Option<Spanned<String>> = None;
        let mut module_annotations: Vec<Spanned<Annotation>> = Vec::new();
        let mut items: Vec<Spanned<Item>> = Vec::new();

        // Parse module-level annotations.
        // Strategy: annotations followed by `;` are module-level.
        // When we see annotations NOT followed by `;`, they belong to
        // the next item, so we stop collecting module annotations.
        while matches!(self.peek(), TokenKind::Annotation(_)) && !self.at_end() {
            self.skip_error_tokens();

            // Check if this annotation is followed by `;` (module-level)
            // or by an item keyword (item-level). We use save/restore of
            // position to peek ahead.
            let saved_pos = self.pos;
            let saved_errors_len = self.errors.len();

            if let Some(ann) = self.parse_annotation() {
                // Check if a semicolon follows — if so, it's module-level
                if self.check(&TokenKind::Semicolon) {
                    self.eat_semicolon();
                    if let Annotation::Module(ref name) = ann.node {
                        module_name = Some(Spanned::new(name.clone(), ann.span));
                    }
                    module_annotations.push(ann);
                } else {
                    // No semicolon — this annotation (and any following)
                    // belongs to the next item. Restore position.
                    self.pos = saved_pos;
                    self.errors.truncate(saved_errors_len);
                    break;
                }
            } else {
                break;
            }
        }

        // Parse items
        while !self.at_end() {
            self.skip_error_tokens();
            if self.at_end() {
                break;
            }

            if let Some(item) = self.parse_item() {
                items.push(item);
            } else {
                // Error recovery: skip to next item
                if !self.at_end() {
                    self.synchronize_item();
                }
            }
        }

        Module {
            name: module_name,
            annotations: module_annotations,
            items,
        }
    }

    /// Parse a single top-level item (function, struct, type alias, import).
    fn parse_item(&mut self) -> Option<Spanned<Item>> {
        // Collect annotations preceding the item
        let annotations = self.parse_annotations();

        match self.peek() {
            TokenKind::Fn => Some(self.parse_function(annotations)),
            TokenKind::Extern => Some(self.parse_extern_function(annotations)),
            TokenKind::Struct => Some(self.parse_struct(annotations)),
            TokenKind::Type => Some(self.parse_type_alias(annotations)),
            TokenKind::Import => Some(self.parse_import(annotations)),
            _ => {
                if !self.at_end() {
                    self.errors.push(ParseError::UnexpectedToken {
                        expected: "item (fn, extern, struct, type, or import)".to_string(),
                        found: format!("{:?}", self.peek()),
                        span: span_to_source_span(self.current_span()),
                    });
                }
                None
            }
        }
    }

    // ── Annotations ───────────────────────────────────────────────────

    /// Collect all leading annotations.
    fn parse_annotations(&mut self) -> Vec<Spanned<Annotation>> {
        let mut annotations = Vec::new();
        while matches!(self.peek(), TokenKind::Annotation(_)) {
            if let Some(ann) = self.parse_annotation() {
                annotations.push(ann);
            }
            // Eat optional semicolons after annotations
            self.eat_semicolon();
        }
        annotations
    }

    /// Parse a single annotation.
    fn parse_annotation(&mut self) -> Option<Spanned<Annotation>> {
        let start_span = self.current_span();
        let name = if let TokenKind::Annotation(name) = self.peek() {
            let n = name.clone();
            self.advance();
            n
        } else {
            return None;
        };

        let annotation = match name.as_str() {
            "pure" => Annotation::Pure,
            "const" => Annotation::Const,
            "module" => {
                // @module name — the name is the next identifier
                if let Some((mod_name, _)) = self.eat_ident() {
                    Annotation::Module(mod_name)
                } else {
                    self.errors.push(ParseError::InvalidAnnotation {
                        name: "module".to_string(),
                        reason: "expected module name".to_string(),
                        span: span_to_source_span(start_span),
                    });
                    return None;
                }
            }
            "inline" => {
                // @inline(always|never|hint)
                if self.eat(&TokenKind::LParen) {
                    let hint = if self.check_ident("always") {
                        self.advance();
                        InlineHint::Always
                    } else if self.check_ident("never") {
                        self.advance();
                        InlineHint::Never
                    } else if self.check_ident("hint") {
                        self.advance();
                        InlineHint::Hint
                    } else {
                        self.errors.push(ParseError::InvalidAnnotation {
                            name: "inline".to_string(),
                            reason: "expected always, never, or hint".to_string(),
                            span: span_to_source_span(self.current_span()),
                        });
                        InlineHint::Hint
                    };
                    self.expect(&TokenKind::RParen, "')'");
                    Annotation::Inline(hint)
                } else {
                    Annotation::Inline(InlineHint::Hint)
                }
            }
            "intent" => {
                // @intent("description")
                if self.eat(&TokenKind::LParen) {
                    if let TokenKind::StringLiteral(_) = self.peek() {
                        let tok = self.advance().clone();
                        if let TokenKind::StringLiteral(s) = tok.kind {
                            self.expect(&TokenKind::RParen, "')'");
                            Annotation::Intent(s)
                        } else {
                            Annotation::Intent(String::new())
                        }
                    } else {
                        self.errors.push(ParseError::InvalidAnnotation {
                            name: "intent".to_string(),
                            reason: "expected string literal".to_string(),
                            span: span_to_source_span(self.current_span()),
                        });
                        self.expect(&TokenKind::RParen, "')'");
                        Annotation::Intent(String::new())
                    }
                } else {
                    Annotation::Intent(String::new())
                }
            }
            "export" => Annotation::Export,
            "complexity" => {
                // @complexity O(n^3) — collect tokens until next annotation/fn/semicolon/brace
                let complexity_start = self.current_span().start;
                let mut complexity_end = complexity_start;
                while !matches!(
                    self.peek(),
                    TokenKind::Fn
                        | TokenKind::Semicolon
                        | TokenKind::LBrace
                        | TokenKind::Eof
                ) && !matches!(self.peek(), TokenKind::Annotation(_))
                {
                    complexity_end = self.current_span().end;
                    self.advance();
                }
                let text = if complexity_end > complexity_start {
                    self.source[complexity_start as usize..complexity_end as usize].trim().to_string()
                } else {
                    String::new()
                };
                Annotation::Complexity(text)
            }
            "constraint" => {
                // @constraint { key: value, ... }
                if self.eat(&TokenKind::LBrace) {
                    let kvs = self.parse_annotation_kv_list();
                    self.expect(&TokenKind::RBrace, "'}'");
                    Annotation::Constraint(kvs)
                } else {
                    Annotation::Constraint(Vec::new())
                }
            }
            "target" => {
                // @target { cpu.simd, gpu.compute }
                if self.eat(&TokenKind::LBrace) {
                    let targets = self.parse_target_list();
                    self.expect(&TokenKind::RBrace, "'}'");
                    Annotation::Target(targets)
                } else {
                    Annotation::Target(Vec::new())
                }
            }
            "strategy" => {
                // @strategy { key: value, ... }
                if self.eat(&TokenKind::LBrace) {
                    let block = self.parse_strategy_block();
                    self.expect(&TokenKind::RBrace, "'}'");
                    Annotation::Strategy(block)
                } else {
                    Annotation::Strategy(StrategyBlock {
                        entries: Vec::new(),
                    })
                }
            }
            "transfer" => {
                // @transfer { source_agent: "...", target_agent: "...", ... }
                if self.eat(&TokenKind::LBrace) {
                    let kvs = self.parse_annotation_kv_list();
                    self.expect(&TokenKind::RBrace, "'}'");
                    let mut source_agent = None;
                    let mut target_agent = None;
                    let mut context = None;
                    let mut open_questions = Vec::new();
                    let mut confidence = None;
                    for (key, val) in &kvs {
                        match key.as_str() {
                            "source_agent" => {
                                if let AnnotationValue::String(s) = val {
                                    source_agent = Some(s.clone());
                                }
                            }
                            "target_agent" => {
                                if let AnnotationValue::String(s) = val {
                                    target_agent = Some(s.clone());
                                }
                            }
                            "context" => {
                                if let AnnotationValue::String(s) = val {
                                    context = Some(s.clone());
                                }
                            }
                            "open_questions" => {
                                if let AnnotationValue::List(items) = val {
                                    for item in items {
                                        if let AnnotationValue::String(s) = item {
                                            open_questions.push(s.clone());
                                        }
                                    }
                                }
                            }
                            "confidence" => {
                                if let AnnotationValue::Map(entries) = val {
                                    let mut correctness = 0.0;
                                    let mut optimality = 0.0;
                                    for (ck, cv) in entries {
                                        match ck.as_str() {
                                            "correctness" => {
                                                if let AnnotationValue::Float(f) = cv {
                                                    correctness = *f;
                                                } else if let AnnotationValue::Int(i) = cv {
                                                    correctness = *i as f64;
                                                }
                                            }
                                            "optimality" => {
                                                if let AnnotationValue::Float(f) = cv {
                                                    optimality = *f;
                                                } else if let AnnotationValue::Int(i) = cv {
                                                    optimality = *i as f64;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    confidence = Some((correctness, optimality));
                                }
                            }
                            _ => {}
                        }
                    }
                    Annotation::Transfer(TransferBlock {
                        source_agent,
                        target_agent,
                        context,
                        open_questions,
                        confidence,
                    })
                } else {
                    Annotation::Transfer(TransferBlock {
                        source_agent: None,
                        target_agent: None,
                        context: None,
                        open_questions: Vec::new(),
                        confidence: None,
                    })
                }
            }
            "vectorizable" => {
                // @vectorizable(i, j, k)
                let dims = self.parse_ident_list_in_parens();
                Annotation::Vectorizable(dims)
            }
            "parallel" => {
                // @parallel(i, j)
                let dims = self.parse_ident_list_in_parens();
                Annotation::Parallel(dims)
            }
            "layout" => {
                // @layout(row_major|col_major|custom)
                if self.eat(&TokenKind::LParen) {
                    let kind = if self.check_ident("row_major") {
                        self.advance();
                        LayoutKind::RowMajor
                    } else if self.check_ident("col_major") {
                        self.advance();
                        LayoutKind::ColMajor
                    } else if let Some((name, _)) = self.eat_ident() {
                        LayoutKind::Custom(name)
                    } else {
                        LayoutKind::RowMajor
                    };
                    self.expect(&TokenKind::RParen, "')'");
                    Annotation::Layout(kind)
                } else {
                    Annotation::Layout(LayoutKind::RowMajor)
                }
            }
            "align" => {
                // @align(64)
                if self.eat(&TokenKind::LParen) {
                    let value = if let TokenKind::IntLiteral { value, .. } = self.peek() {
                        let v = *value as u64;
                        self.advance();
                        v
                    } else {
                        0
                    };
                    self.expect(&TokenKind::RParen, "')'");
                    Annotation::Align(value)
                } else {
                    Annotation::Align(0)
                }
            }
            _ => {
                // Custom annotation: @name or @name(args) or @name { kv }
                if self.eat(&TokenKind::LParen) {
                    let args = self.parse_annotation_arg_list();
                    self.expect(&TokenKind::RParen, "')'");
                    Annotation::Custom(name, args)
                } else if self.eat(&TokenKind::LBrace) {
                    let kvs = self.parse_annotation_kv_list();
                    self.expect(&TokenKind::RBrace, "'}'");
                    let map_value = AnnotationValue::Map(kvs);
                    Annotation::Custom(name, vec![map_value])
                } else {
                    Annotation::Custom(name, Vec::new())
                }
            }
        };

        let end_span = self.prev_span();
        Some(Spanned::new(annotation, start_span.merge(end_span)))
    }

    /// Parse a parenthesized list of identifiers: `(a, b, c)`.
    fn parse_ident_list_in_parens(&mut self) -> Vec<String> {
        let mut names = Vec::new();
        if self.eat(&TokenKind::LParen) {
            loop {
                if self.check(&TokenKind::RParen) || self.at_end() {
                    break;
                }
                if let Some((name, _)) = self.eat_ident() {
                    names.push(name);
                } else {
                    break;
                }
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::RParen, "')'");
        }
        names
    }

    /// Parse annotation argument list (comma-separated values).
    fn parse_annotation_arg_list(&mut self) -> Vec<AnnotationValue> {
        let mut args = Vec::new();
        loop {
            if self.check(&TokenKind::RParen) || self.at_end() {
                break;
            }
            if let Some(val) = self.parse_annotation_value() {
                args.push(val);
            } else {
                break;
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        args
    }

    /// Parse annotation key-value list: `key: value, key: value`.
    fn parse_annotation_kv_list(&mut self) -> Vec<(String, AnnotationValue)> {
        let mut kvs = Vec::new();
        loop {
            if self.check(&TokenKind::RBrace) || self.at_end() {
                break;
            }
            if let Some((key, _)) = self.eat_ident() {
                if self.eat(&TokenKind::Colon) {
                    if let Some(val) = self.parse_annotation_value() {
                        kvs.push((key, val));
                    }
                }
            } else {
                break;
            }
            // commas are optional between kv pairs
            self.eat(&TokenKind::Comma);
        }
        kvs
    }

    /// Parse a single annotation value.
    fn parse_annotation_value(&mut self) -> Option<AnnotationValue> {
        match self.peek() {
            TokenKind::StringLiteral(_) => {
                let tok = self.advance().clone();
                if let TokenKind::StringLiteral(s) = tok.kind {
                    Some(AnnotationValue::String(s))
                } else {
                    None
                }
            }
            TokenKind::IntLiteral { .. } => {
                let tok = self.advance().clone();
                if let TokenKind::IntLiteral { value, .. } = tok.kind {
                    Some(AnnotationValue::Int(value as i64))
                } else {
                    None
                }
            }
            TokenKind::FloatLiteral { .. } => {
                let tok = self.advance().clone();
                if let TokenKind::FloatLiteral { value, .. } = tok.kind {
                    Some(AnnotationValue::Float(value))
                } else {
                    None
                }
            }
            TokenKind::BoolLiteral(_) => {
                let tok = self.advance().clone();
                if let TokenKind::BoolLiteral(b) = tok.kind {
                    Some(AnnotationValue::Bool(b))
                } else {
                    None
                }
            }
            TokenKind::Ident(_) => {
                let tok = self.advance().clone();
                if let TokenKind::Ident(s) = tok.kind {
                    Some(AnnotationValue::Ident(s))
                } else {
                    None
                }
            }
            TokenKind::LBrace => {
                self.advance();
                let kvs = self.parse_annotation_kv_list();
                self.expect(&TokenKind::RBrace, "'}'");
                Some(AnnotationValue::Map(kvs))
            }
            TokenKind::LBracket => {
                self.advance();
                let mut items = Vec::new();
                loop {
                    if self.check(&TokenKind::RBracket) || self.at_end() {
                        break;
                    }
                    if let Some(val) = self.parse_annotation_value() {
                        items.push(val);
                    } else {
                        break;
                    }
                    self.eat(&TokenKind::Comma);
                }
                self.expect(&TokenKind::RBracket, "']'");
                Some(AnnotationValue::List(items))
            }
            _ => None,
        }
    }

    /// Parse target list inside braces: `cpu.simd, gpu.compute`.
    fn parse_target_list(&mut self) -> Vec<String> {
        let mut targets = Vec::new();
        loop {
            if self.check(&TokenKind::RBrace) || self.at_end() {
                break;
            }
            if let Some((first, _)) = self.eat_ident() {
                let mut name = first;
                while self.eat(&TokenKind::Dot) {
                    if let Some((part, _)) = self.eat_ident() {
                        name.push('.');
                        name.push_str(&part);
                    }
                }
                targets.push(name);
            } else {
                break;
            }
            self.eat(&TokenKind::Comma);
        }
        targets
    }

    // ── Strategy block ────────────────────────────────────────────────

    /// Parse the contents of a strategy block (inside braces).
    fn parse_strategy_block(&mut self) -> StrategyBlock {
        let mut entries = Vec::new();
        loop {
            if self.check(&TokenKind::RBrace) || self.at_end() {
                break;
            }
            // Each entry is IDENT ':' StrategyValue
            // Detect entries by checking for IDENT followed by ':'
            if matches!(self.peek(), TokenKind::Ident(_))
                && matches!(self.peek_nth(1), TokenKind::Colon)
            {
                if let Some((key, _)) = self.eat_ident() {
                    self.eat(&TokenKind::Colon);
                    let value = self.parse_strategy_value();
                    entries.push((key, value));
                }
            } else {
                // Not a valid entry — skip token to avoid infinite loop
                self.advance();
            }
        }
        StrategyBlock { entries }
    }

    /// Parse a strategy value: `?hole`, `{ k: v }`, or a concrete value.
    fn parse_strategy_value(&mut self) -> StrategyValue {
        match self.peek() {
            TokenKind::OptHole(_) => {
                let tok = self.advance().clone();
                if let TokenKind::OptHole(name) = tok.kind {
                    StrategyValue::Hole(name)
                } else {
                    StrategyValue::Concrete(AnnotationValue::String(String::new()))
                }
            }
            TokenKind::LBrace => {
                self.advance();
                let mut map = Vec::new();
                loop {
                    if self.check(&TokenKind::RBrace) || self.at_end() {
                        break;
                    }
                    if let Some((key, _)) = self.eat_ident() {
                        if self.eat(&TokenKind::Colon) {
                            let val = self.parse_strategy_value();
                            map.push((key, val));
                        }
                    } else {
                        break;
                    }
                    self.eat(&TokenKind::Comma);
                }
                self.expect(&TokenKind::RBrace, "'}'");
                StrategyValue::Map(map)
            }
            _ => {
                if let Some(val) = self.parse_annotation_value() {
                    StrategyValue::Concrete(val)
                } else {
                    // Fallback: skip token
                    self.advance();
                    StrategyValue::Concrete(AnnotationValue::String(String::new()))
                }
            }
        }
    }

    // ── Function ──────────────────────────────────────────────────────

    /// Parse a function definition.
    fn parse_function(&mut self, annotations: Vec<Spanned<Annotation>>) -> Spanned<Item> {
        let start_span = self.current_span();
        self.advance(); // consume `fn`

        let name = if let Some((name, span)) = self.expect_ident("function name") {
            Spanned::new(name, span)
        } else {
            Spanned::new("_error_".to_string(), self.current_span())
        };

        // Parse parameter list
        let params = if self.eat(&TokenKind::LParen) {
            let params = self.parse_param_list();
            self.expect(&TokenKind::RParen, "')'");
            params
        } else {
            self.errors.push(ParseError::UnexpectedToken {
                expected: "'('".to_string(),
                found: format!("{:?}", self.peek()),
                span: span_to_source_span(self.current_span()),
            });
            Vec::new()
        };

        // Parse return type
        let return_type = if self.eat(&TokenKind::Arrow) {
            self.parse_type_expr()
        } else {
            // If no arrow, check for direct `{` which means missing return type
            TypeExpr::Named("void".to_string())
        };

        // Parse optional annotations on return type (before the body)
        // e.g., -> tensor[f32, M, N] @layout(row_major)
        let mut all_annotations = annotations;
        while matches!(self.peek(), TokenKind::Annotation(_)) {
            if let Some(ann) = self.parse_annotation() {
                all_annotations.push(ann);
            }
        }

        // Parse body
        let body = if self.check(&TokenKind::LBrace) {
            self.parse_block()
        } else {
            self.errors.push(ParseError::UnexpectedToken {
                expected: "'{'".to_string(),
                found: format!("{:?}", self.peek()),
                span: span_to_source_span(self.current_span()),
            });
            Block {
                annotations: Vec::new(),
                stmts: Vec::new(),
            }
        };

        let end_span = self.prev_span();
        Spanned::new(
            Item::Function(Function {
                name,
                annotations: all_annotations,
                params,
                return_type,
                body,
            }),
            start_span.merge(end_span),
        )
    }

    /// Parse an extern function declaration: `extern fn name(params) -> RetType;`
    fn parse_extern_function(&mut self, annotations: Vec<Spanned<Annotation>>) -> Spanned<Item> {
        let start_span = self.current_span();
        self.advance(); // consume `extern`

        // Expect `fn` keyword after `extern`
        if !self.eat(&TokenKind::Fn) {
            self.errors.push(ParseError::UnexpectedToken {
                expected: "'fn' after 'extern'".to_string(),
                found: format!("{:?}", self.peek()),
                span: span_to_source_span(self.current_span()),
            });
        }

        let name = if let Some((name, span)) = self.expect_ident("extern function name") {
            Spanned::new(name, span)
        } else {
            Spanned::new("_error_".to_string(), self.current_span())
        };

        // Parse parameter list
        let params = if self.eat(&TokenKind::LParen) {
            let params = self.parse_param_list();
            self.expect(&TokenKind::RParen, "')'");
            params
        } else {
            self.errors.push(ParseError::UnexpectedToken {
                expected: "'('".to_string(),
                found: format!("{:?}", self.peek()),
                span: span_to_source_span(self.current_span()),
            });
            Vec::new()
        };

        // Parse optional return type
        let return_type = if self.eat(&TokenKind::Arrow) {
            self.parse_type_expr()
        } else {
            TypeExpr::Named("void".to_string())
        };

        // Expect semicolon (no body)
        self.expect_semicolon();

        let end_span = self.prev_span();
        Spanned::new(
            Item::ExternFunction(ExternFunction {
                name,
                annotations,
                params,
                return_type,
            }),
            start_span.merge(end_span),
        )
    }

    /// Parse a comma-separated parameter list (inside parentheses).
    fn parse_param_list(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        loop {
            if self.check(&TokenKind::RParen) || self.at_end() {
                break;
            }
            if let Some(param) = self.parse_param() {
                params.push(param);
            } else {
                break;
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        params
    }

    /// Parse a single function parameter: `name: Type @annotations`.
    fn parse_param(&mut self) -> Option<Param> {
        let (name, name_span) = self.expect_ident("parameter name")?;
        self.expect(&TokenKind::Colon, "':'");
        let ty = self.parse_type_expr();

        // Parse inline annotations on the parameter
        let mut annotations = Vec::new();
        while matches!(self.peek(), TokenKind::Annotation(_)) {
            if let Some(ann) = self.parse_annotation() {
                annotations.push(ann);
            }
        }

        Some(Param {
            name: Spanned::new(name, name_span),
            ty,
            annotations,
        })
    }

    // ── Struct ────────────────────────────────────────────────────────

    /// Parse a struct definition.
    fn parse_struct(&mut self, annotations: Vec<Spanned<Annotation>>) -> Spanned<Item> {
        let start_span = self.current_span();
        self.advance(); // consume `struct`

        let name = if let Some((name, span)) = self.expect_ident("struct name") {
            Spanned::new(name, span)
        } else {
            Spanned::new("_error_".to_string(), self.current_span())
        };

        let mut fields = Vec::new();
        if self.eat(&TokenKind::LBrace) {
            loop {
                if self.check(&TokenKind::RBrace) || self.at_end() {
                    break;
                }
                if let Some(field) = self.parse_struct_field() {
                    fields.push(field);
                } else {
                    break;
                }
                self.eat(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RBrace, "'}'");
        }

        let end_span = self.prev_span();
        Spanned::new(
            Item::Struct(StructDef {
                name,
                annotations,
                fields,
            }),
            start_span.merge(end_span),
        )
    }

    /// Parse a struct field: `name: Type @annotations`.
    fn parse_struct_field(&mut self) -> Option<StructField> {
        let (name, name_span) = self.eat_ident()?;
        self.expect(&TokenKind::Colon, "':'");
        let ty = self.parse_type_expr();

        let mut annotations = Vec::new();
        while matches!(self.peek(), TokenKind::Annotation(_)) {
            if let Some(ann) = self.parse_annotation() {
                annotations.push(ann);
            }
        }

        Some(StructField {
            name: Spanned::new(name, name_span),
            ty,
            annotations,
        })
    }

    // ── Type alias ────────────────────────────────────────────────────

    /// Parse a type alias: `type Name = TypeExpr;`.
    fn parse_type_alias(&mut self, annotations: Vec<Spanned<Annotation>>) -> Spanned<Item> {
        let start_span = self.current_span();
        self.advance(); // consume `type`

        let name = if let Some((name, span)) = self.expect_ident("type name") {
            Spanned::new(name, span)
        } else {
            Spanned::new("_error_".to_string(), self.current_span())
        };

        self.expect(&TokenKind::Assign, "'='");
        let ty = self.parse_type_expr();
        self.expect_semicolon();

        let end_span = self.prev_span();
        let _ = annotations; // type aliases don't have annotations in the AST yet
        Spanned::new(
            Item::TypeAlias(TypeAlias { name, ty }),
            start_span.merge(end_span),
        )
    }

    // ── Import ────────────────────────────────────────────────────────

    /// Parse an import declaration: `import path::to::module;`.
    fn parse_import(&mut self, annotations: Vec<Spanned<Annotation>>) -> Spanned<Item> {
        let start_span = self.current_span();
        self.advance(); // consume `import`

        let mut path = Vec::new();
        if let Some((first, _)) = self.expect_ident("import path") {
            path.push(first);
            while self.eat(&TokenKind::ColonColon) {
                if let Some((part, _)) = self.expect_ident("import path segment") {
                    path.push(part);
                }
            }
        }

        // Check for `as alias`
        let alias = if self.check_ident("as") {
            self.advance();
            self.eat_ident().map(|(name, _)| name)
        } else {
            None
        };

        self.expect_semicolon();

        let end_span = self.prev_span();
        let _ = annotations;
        Spanned::new(
            Item::Import(ImportDecl { path, alias }),
            start_span.merge(end_span),
        )
    }

    // ── Type expressions ──────────────────────────────────────────────

    /// Parse a type expression.
    fn parse_type_expr(&mut self) -> TypeExpr {
        match self.peek() {
            // Type keyword tokens -> Named types
            TokenKind::I8 => { self.advance(); TypeExpr::Named("i8".to_string()) }
            TokenKind::I16 => { self.advance(); TypeExpr::Named("i16".to_string()) }
            TokenKind::I32 => { self.advance(); TypeExpr::Named("i32".to_string()) }
            TokenKind::I64 => { self.advance(); TypeExpr::Named("i64".to_string()) }
            TokenKind::I128 => { self.advance(); TypeExpr::Named("i128".to_string()) }
            TokenKind::U8 => { self.advance(); TypeExpr::Named("u8".to_string()) }
            TokenKind::U16 => { self.advance(); TypeExpr::Named("u16".to_string()) }
            TokenKind::U32 => { self.advance(); TypeExpr::Named("u32".to_string()) }
            TokenKind::U64 => { self.advance(); TypeExpr::Named("u64".to_string()) }
            TokenKind::U128 => { self.advance(); TypeExpr::Named("u128".to_string()) }
            TokenKind::F16 => { self.advance(); TypeExpr::Named("f16".to_string()) }
            TokenKind::Bf16 => { self.advance(); TypeExpr::Named("bf16".to_string()) }
            TokenKind::F32 => { self.advance(); TypeExpr::Named("f32".to_string()) }
            TokenKind::F64 => { self.advance(); TypeExpr::Named("f64".to_string()) }
            TokenKind::Bool => { self.advance(); TypeExpr::Named("bool".to_string()) }

            // Tensor type: tensor[T, dims...]
            TokenKind::Tensor => {
                self.advance();
                if self.eat(&TokenKind::LBracket) {
                    let elem = self.parse_type_expr();
                    let mut dims = Vec::new();
                    while self.eat(&TokenKind::Comma) {
                        if self.check(&TokenKind::RBracket) {
                            break;
                        }
                        dims.push(self.parse_dim_expr());
                    }
                    self.expect(&TokenKind::RBracket, "']'");
                    TypeExpr::Tensor(Box::new(elem), dims)
                } else {
                    TypeExpr::Named("tensor".to_string())
                }
            }

            // Array type: array[T, N]
            TokenKind::Array => {
                self.advance();
                if self.eat(&TokenKind::LBracket) {
                    let elem = self.parse_type_expr();
                    self.expect(&TokenKind::Comma, "','");
                    let size = self.parse_expr();
                    self.expect(&TokenKind::RBracket, "']'");
                    TypeExpr::Array(Box::new(elem), Box::new(size))
                } else {
                    TypeExpr::Named("array".to_string())
                }
            }

            // Slice type: slice[T]
            TokenKind::Slice => {
                self.advance();
                if self.eat(&TokenKind::LBracket) {
                    let elem = self.parse_type_expr();
                    self.expect(&TokenKind::RBracket, "']'");
                    TypeExpr::Slice(Box::new(elem))
                } else {
                    TypeExpr::Named("slice".to_string())
                }
            }

            // Ptr type: ptr[T]
            TokenKind::Ptr => {
                self.advance();
                if self.eat(&TokenKind::LBracket) {
                    let elem = self.parse_type_expr();
                    self.expect(&TokenKind::RBracket, "']'");
                    TypeExpr::Ptr(Box::new(elem))
                } else {
                    TypeExpr::Named("ptr".to_string())
                }
            }

            // Function type: fn(T1, T2) -> R
            TokenKind::Fn => {
                self.advance();
                if self.eat(&TokenKind::LParen) {
                    let mut param_types = Vec::new();
                    loop {
                        if self.check(&TokenKind::RParen) || self.at_end() {
                            break;
                        }
                        param_types.push(self.parse_type_expr());
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen, "')'");
                    self.expect(&TokenKind::Arrow, "'->'");
                    let ret = self.parse_type_expr();
                    TypeExpr::Fn(param_types, Box::new(ret))
                } else {
                    TypeExpr::Named("fn".to_string())
                }
            }

            // Tuple type: (T1, T2, ...)
            TokenKind::LParen => {
                self.advance();
                let first = self.parse_type_expr();
                if self.eat(&TokenKind::Comma) {
                    let mut types = vec![first];
                    loop {
                        if self.check(&TokenKind::RParen) || self.at_end() {
                            break;
                        }
                        types.push(self.parse_type_expr());
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen, "')'");
                    TypeExpr::Tuple(types)
                } else {
                    self.expect(&TokenKind::RParen, "')'");
                    // Single element in parens — just grouping
                    first
                }
            }

            // Named type (identifier)
            TokenKind::Ident(_) => {
                let tok = self.advance().clone();
                if let TokenKind::Ident(name) = tok.kind {
                    TypeExpr::Named(name)
                } else {
                    TypeExpr::Named("_error_".to_string())
                }
            }

            _ => {
                self.errors.push(ParseError::InvalidTypeExpression {
                    detail: format!("unexpected token {:?}", self.peek()),
                    span: span_to_source_span(self.current_span()),
                });
                TypeExpr::Named("_error_".to_string())
            }
        }
    }

    /// Parse a dimension expression in tensor types.
    fn parse_dim_expr(&mut self) -> DimExpr {
        match self.peek() {
            TokenKind::IntLiteral { .. } => {
                let tok = self.advance().clone();
                if let TokenKind::IntLiteral { value, .. } = tok.kind {
                    DimExpr::Const(value as i64)
                } else {
                    DimExpr::Dynamic
                }
            }
            TokenKind::Ident(_) => {
                let tok = self.advance().clone();
                if let TokenKind::Ident(name) = tok.kind {
                    DimExpr::Named(name)
                } else {
                    DimExpr::Dynamic
                }
            }
            TokenKind::OptHole(_) => {
                self.advance();
                DimExpr::Dynamic
            }
            _ => {
                // `?` for dynamic is also the OptHole token
                // but a bare `?` isn't produced by the lexer
                self.errors.push(ParseError::InvalidTypeExpression {
                    detail: "expected dimension (integer, identifier, or ?)".to_string(),
                    span: span_to_source_span(self.current_span()),
                });
                self.advance();
                DimExpr::Dynamic
            }
        }
    }

    // ── Statements ────────────────────────────────────────────────────

    /// Parse a block: `{ stmt* }`.
    fn parse_block(&mut self) -> Block {
        let open_span = self.current_span();
        self.expect(&TokenKind::LBrace, "'{'");

        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.errors.push(ParseError::InvalidExpression {
                detail: "maximum nesting depth exceeded".to_string(),
                span: span_to_source_span(self.current_span()),
            });
            self.depth -= 1;
            return Block {
                annotations: Vec::new(),
                stmts: Vec::new(),
            };
        }

        let mut stmts = Vec::new();
        let mut block_annotations = Vec::new();

        while !self.check(&TokenKind::RBrace) && !self.at_end() {
            self.skip_error_tokens();
            if self.check(&TokenKind::RBrace) || self.at_end() {
                break;
            }

            // Check for standalone annotations inside a block (e.g., @strategy)
            if matches!(self.peek(), TokenKind::Annotation(_)) {
                // Peek ahead to see if this annotation precedes a statement or
                // is standalone (like @strategy { ... })
                if let Some(ann) = self.parse_annotation() {
                    // If we just parsed an annotation and the next token can start
                    // a statement, this is a statement annotation (collect for the
                    // next statement). Otherwise it's a block annotation.
                    if matches!(ann.node, Annotation::Strategy(_)) {
                        block_annotations.push(ann);
                    } else {
                        // For now, just store as block annotation
                        block_annotations.push(ann);
                    }
                    self.eat_semicolon();
                    continue;
                }
            }

            if let Some(stmt) = self.parse_stmt() {
                stmts.push(stmt);
            } else {
                self.synchronize_stmt();
            }
        }

        if !self.eat(&TokenKind::RBrace) {
            self.errors.push(ParseError::MissingClosingDelimiter {
                delimiter: '}',
                open_span: span_to_source_span(open_span),
                span: span_to_source_span(self.current_span()),
            });
        }

        self.depth -= 1;

        Block {
            annotations: block_annotations,
            stmts,
        }
    }

    /// Parse a single statement.
    fn parse_stmt(&mut self) -> Option<Spanned<Stmt>> {
        match self.peek() {
            TokenKind::Let => Some(self.parse_let_stmt()),
            TokenKind::Return => Some(self.parse_return_stmt()),
            TokenKind::If => Some(self.parse_if_stmt()),
            TokenKind::For => Some(self.parse_for_stmt()),
            TokenKind::While => Some(self.parse_while_stmt()),
            _ => Some(self.parse_assign_or_expr_stmt()),
        }
    }

    /// Parse a let binding: `let [mut] name: Type = expr;`.
    fn parse_let_stmt(&mut self) -> Spanned<Stmt> {
        let start_span = self.current_span();
        self.advance(); // consume `let`

        let mutable = self.eat(&TokenKind::Mut);

        let name = if let Some((name, span)) = self.expect_ident("variable name") {
            Spanned::new(name, span)
        } else {
            let span = self.current_span();
            Spanned::new("_error_".to_string(), span)
        };

        self.expect(&TokenKind::Colon, "':'");
        let ty = self.parse_type_expr();
        self.expect(&TokenKind::Assign, "'='");
        let value = self.parse_expr();
        self.expect_semicolon();

        let end_span = self.prev_span();
        Spanned::new(
            Stmt::Let {
                name,
                ty,
                value,
                mutable,
            },
            start_span.merge(end_span),
        )
    }

    /// Parse a return statement: `return expr;`.
    fn parse_return_stmt(&mut self) -> Spanned<Stmt> {
        let start_span = self.current_span();
        self.advance(); // consume `return`

        let value = self.parse_expr();
        self.expect_semicolon();

        let end_span = self.prev_span();
        Spanned::new(Stmt::Return(value), start_span.merge(end_span))
    }

    /// Parse an if statement: `if cond { } else { }`.
    fn parse_if_stmt(&mut self) -> Spanned<Stmt> {
        let start_span = self.current_span();
        self.advance(); // consume `if`

        let condition = self.parse_expr();
        let then_block = self.parse_block();

        let else_block = if self.eat(&TokenKind::Else) {
            Some(self.parse_block())
        } else {
            None
        };

        let end_span = self.prev_span();
        Spanned::new(
            Stmt::If {
                condition,
                then_block,
                else_block,
            },
            start_span.merge(end_span),
        )
    }

    /// Parse a for loop: `for name: Type in expr { }`.
    fn parse_for_stmt(&mut self) -> Spanned<Stmt> {
        let start_span = self.current_span();
        self.advance(); // consume `for`

        let var = if let Some((name, span)) = self.expect_ident("loop variable") {
            Spanned::new(name, span)
        } else {
            Spanned::new("_error_".to_string(), self.current_span())
        };

        self.expect(&TokenKind::Colon, "':'");
        let var_type = self.parse_type_expr();
        self.expect(&TokenKind::In, "'in'");
        let iterable = self.parse_expr();
        let body = self.parse_block();

        let end_span = self.prev_span();
        Spanned::new(
            Stmt::For {
                var,
                var_type,
                iterable,
                body,
            },
            start_span.merge(end_span),
        )
    }

    /// Parse a while loop: `while cond { }`.
    fn parse_while_stmt(&mut self) -> Spanned<Stmt> {
        let start_span = self.current_span();
        self.advance(); // consume `while`

        let condition = self.parse_expr();
        let body = self.parse_block();

        let end_span = self.prev_span();
        Spanned::new(
            Stmt::While { condition, body },
            start_span.merge(end_span),
        )
    }

    /// Parse an assignment or expression statement: `expr = expr;` or `expr;`.
    fn parse_assign_or_expr_stmt(&mut self) -> Spanned<Stmt> {
        let start_span = self.current_span();
        let expr = self.parse_expr();

        if self.eat(&TokenKind::Assign) {
            let value = self.parse_expr();
            self.expect_semicolon();
            let end_span = self.prev_span();
            Spanned::new(
                Stmt::Assign {
                    target: expr,
                    value,
                },
                start_span.merge(end_span),
            )
        } else {
            self.expect_semicolon();
            let end_span = self.prev_span();
            Spanned::new(Stmt::Expr(expr), start_span.merge(end_span))
        }
    }

    // ── Expressions (Pratt parser) ────────────────────────────────────

    /// Parse an expression (entry point).
    fn parse_expr(&mut self) -> Expr {
        self.parse_expr_bp(0)
    }

    /// Core Pratt expression parser with minimum binding power.
    fn parse_expr_bp(&mut self, min_bp: u8) -> Expr {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.errors.push(ParseError::InvalidExpression {
                detail: "maximum expression nesting depth exceeded".to_string(),
                span: span_to_source_span(self.current_span()),
            });
            self.depth -= 1;
            return Expr::IntLiteral(0);
        }

        let mut lhs = self.parse_prefix();

        loop {
            // Check for postfix operators first (highest precedence)
            if let Some(left_bp) = self.postfix_bp() {
                if left_bp < min_bp {
                    break;
                }
                lhs = self.parse_postfix(lhs);
                continue;
            }

            // Check for infix operators
            if let Some((op, left_bp, right_bp)) = self.peek_infix_op() {
                if left_bp < min_bp {
                    break;
                }
                self.advance(); // consume operator
                let rhs = self.parse_expr_bp(right_bp);
                lhs = Expr::BinaryOp {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
                continue;
            }

            break;
        }

        self.depth -= 1;
        lhs
    }

    /// Parse a prefix expression (literals, identifiers, unary ops, grouping).
    fn parse_prefix(&mut self) -> Expr {
        match self.peek().clone() {
            TokenKind::IntLiteral { value, .. } => {
                let v = value;
                self.advance();
                Expr::IntLiteral(v)
            }
            TokenKind::FloatLiteral { value, .. } => {
                let v = value;
                self.advance();
                Expr::FloatLiteral(v)
            }
            TokenKind::StringLiteral(ref s) => {
                let s = s.clone();
                self.advance();
                Expr::StringLiteral(s)
            }
            TokenKind::BoolLiteral(b) => {
                self.advance();
                Expr::BoolLiteral(b)
            }
            TokenKind::Ident(ref name) if name == "array_zeros" => {
                self.advance();
                // Parse array_zeros[T, N]
                if self.eat(&TokenKind::LBracket) {
                    let elem_type = self.parse_type_expr();
                    self.expect(&TokenKind::Comma, "','");
                    let size = self.parse_expr();
                    self.expect(&TokenKind::RBracket, "']'");
                    Expr::ArrayZeros {
                        element_type: elem_type,
                        size: Box::new(size),
                    }
                } else {
                    Expr::Ident("array_zeros".to_string())
                }
            }
            TokenKind::Ident(ref name) => {
                let name = name.clone();
                self.advance();
                Expr::Ident(name)
            }
            TokenKind::OptHole(ref name) => {
                let name = name.clone();
                self.advance();
                Expr::OptHole(name)
            }
            // Unary minus
            TokenKind::Minus => {
                self.advance();
                let operand = self.parse_expr_bp(11); // prefix binding power
                Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                }
            }
            // Unary not
            TokenKind::Not => {
                self.advance();
                let operand = self.parse_expr_bp(11);
                Expr::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                }
            }
            // Parenthesized expression
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr_bp(0);
                self.expect(&TokenKind::RParen, "')'");
                expr
            }
            // Conversion keywords: widen(x), narrow(x), truncate(x)
            TokenKind::Widen => {
                self.advance();
                self.parse_call_args(Expr::Ident("widen".to_string()))
            }
            TokenKind::Narrow => {
                self.advance();
                self.parse_call_args(Expr::Ident("narrow".to_string()))
            }
            TokenKind::Truncate => {
                self.advance();
                self.parse_call_args(Expr::Ident("truncate".to_string()))
            }
            // Type keywords used as identifiers in expressions (e.g., tensor.zeros)
            ref tok if is_type_keyword(tok) => {
                let name = type_keyword_name(self.peek());
                self.advance();
                Expr::Ident(name)
            }
            _ => {
                self.errors.push(ParseError::InvalidExpression {
                    detail: format!("unexpected token {:?}", self.peek()),
                    span: span_to_source_span(self.current_span()),
                });
                // Don't advance — let caller handle recovery
                Expr::IntLiteral(0)
            }
        }
    }

    /// Get the binding power of a postfix operator at the current position.
    fn postfix_bp(&self) -> Option<u8> {
        match self.peek() {
            TokenKind::LParen => Some(13),
            TokenKind::LBracket => Some(13),
            TokenKind::Dot => Some(13),
            _ => None,
        }
    }

    /// Get the infix operator, left bp, and right bp for the current token.
    fn peek_infix_op(&self) -> Option<(BinOp, u8, u8)> {
        match self.peek() {
            TokenKind::Or => Some((BinOp::Or, 1, 2)),
            TokenKind::And => Some((BinOp::And, 3, 4)),
            TokenKind::Eq => Some((BinOp::Eq, 5, 6)),
            TokenKind::NotEq => Some((BinOp::NotEq, 5, 6)),
            TokenKind::Lt => Some((BinOp::Lt, 5, 6)),
            TokenKind::Gt => Some((BinOp::Gt, 5, 6)),
            TokenKind::LtEq => Some((BinOp::LtEq, 5, 6)),
            TokenKind::GtEq => Some((BinOp::GtEq, 5, 6)),
            TokenKind::Plus => Some((BinOp::Add, 7, 8)),
            TokenKind::Minus => Some((BinOp::Sub, 7, 8)),
            TokenKind::PlusWrap => Some((BinOp::AddWrap, 7, 8)),
            TokenKind::PlusSat => Some((BinOp::AddSat, 7, 8)),
            TokenKind::MinusWrap => Some((BinOp::SubWrap, 7, 8)),
            TokenKind::MinusSat => Some((BinOp::SubSat, 7, 8)),
            TokenKind::Star => Some((BinOp::Mul, 9, 10)),
            TokenKind::Slash => Some((BinOp::Div, 9, 10)),
            TokenKind::Percent => Some((BinOp::Mod, 9, 10)),
            TokenKind::StarWrap => Some((BinOp::MulWrap, 9, 10)),
            _ => None,
        }
    }

    /// Parse a postfix expression (call, index, field access, method call).
    fn parse_postfix(&mut self, lhs: Expr) -> Expr {
        match self.peek() {
            TokenKind::LParen => self.parse_call_args(lhs),
            TokenKind::LBracket => {
                self.advance(); // consume `[`
                let mut indices = Vec::new();
                loop {
                    if self.check(&TokenKind::RBracket) || self.at_end() {
                        break;
                    }
                    indices.push(self.parse_expr());
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RBracket, "']'");
                Expr::Index {
                    expr: Box::new(lhs),
                    indices,
                }
            }
            TokenKind::Dot => {
                self.advance(); // consume `.`
                if let Some((name, _)) = self.eat_ident() {
                    // Check if this is a method call (next token is `(`)
                    if matches!(self.peek(), TokenKind::LParen) {
                        self.parse_call_args(Expr::MethodCall {
                            expr: Box::new(lhs),
                            method: name,
                            args: Vec::new(), // placeholder, will be replaced
                        })
                        // Actually, we need a different approach: build MethodCall directly
                    } else {
                        Expr::FieldAccess {
                            expr: Box::new(lhs),
                            field: name,
                        }
                    }
                } else if let Some((name, _)) = self.try_eat_type_keyword_as_ident() {
                    // Handle things like `tensor.zeros` where zeros might be a type keyword
                    Expr::FieldAccess {
                        expr: Box::new(lhs),
                        field: name,
                    }
                } else {
                    self.errors.push(ParseError::UnexpectedToken {
                        expected: "field name".to_string(),
                        found: format!("{:?}", self.peek()),
                        span: span_to_source_span(self.current_span()),
                    });
                    lhs
                }
            }
            _ => lhs,
        }
    }

    /// Parse call arguments: `(expr, expr, ...)`.
    /// If `func` is a MethodCall placeholder, extracts method info.
    fn parse_call_args(&mut self, func: Expr) -> Expr {
        self.advance(); // consume `(`
        let mut args = Vec::new();
        loop {
            if self.check(&TokenKind::RParen) || self.at_end() {
                break;
            }
            args.push(self.parse_expr());
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "')'");

        // If the func is a MethodCall placeholder (from dot access), build MethodCall
        if let Expr::MethodCall {
            expr,
            method,
            args: _,
        } = func
        {
            Expr::MethodCall {
                expr,
                method,
                args,
            }
        } else {
            Expr::Call {
                func: Box::new(func),
                args,
            }
        }
    }
}

/// Check if a token kind is a type keyword.
fn is_type_keyword(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::I8
            | TokenKind::I16
            | TokenKind::I32
            | TokenKind::I64
            | TokenKind::I128
            | TokenKind::U8
            | TokenKind::U16
            | TokenKind::U32
            | TokenKind::U64
            | TokenKind::U128
            | TokenKind::F16
            | TokenKind::Bf16
            | TokenKind::F32
            | TokenKind::F64
            | TokenKind::Bool
            | TokenKind::Tensor
            | TokenKind::Array
            | TokenKind::Slice
            | TokenKind::Ptr
    )
}

/// Get the string name for a type keyword token.
fn type_keyword_name(kind: &TokenKind) -> String {
    match kind {
        TokenKind::I8 => "i8",
        TokenKind::I16 => "i16",
        TokenKind::I32 => "i32",
        TokenKind::I64 => "i64",
        TokenKind::I128 => "i128",
        TokenKind::U8 => "u8",
        TokenKind::U16 => "u16",
        TokenKind::U32 => "u32",
        TokenKind::U64 => "u64",
        TokenKind::U128 => "u128",
        TokenKind::F16 => "f16",
        TokenKind::Bf16 => "bf16",
        TokenKind::F32 => "f32",
        TokenKind::F64 => "f64",
        TokenKind::Bool => "bool",
        TokenKind::Tensor => "tensor",
        TokenKind::Array => "array",
        TokenKind::Slice => "slice",
        TokenKind::Ptr => "ptr",
        _ => "_unknown_",
    }
    .to_string()
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: parse source and return the module, panicking on errors.
    fn parse_ok(source: &str) -> Module {
        let result = parse(source);
        if result.has_errors() {
            panic!(
                "Expected no parse errors, got {} errors:\n{:#?}",
                result.errors.len(),
                result.errors
            );
        }
        result.module
    }

    /// Helper: parse a single expression from a function body.
    fn parse_expr_from(source: &str) -> Expr {
        // Wrap in a function body so we can extract the expression
        let full = format!("fn _test() -> i32 {{ return {source}; }}");
        let result = parse(&full);
        if result.has_errors() {
            panic!(
                "Expected no parse errors, got {} errors:\n{:#?}",
                result.errors.len(),
                result.errors
            );
        }
        let func = match &result.module.items[0].node {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        let stmt = &func.body.stmts[0].node;
        match stmt {
            Stmt::Return(expr) => expr.clone(),
            _ => panic!("expected return statement"),
        }
    }

    /// Helper: parse a single statement from a function body.
    fn parse_stmt_from(stmt_source: &str) -> Stmt {
        let full = format!("fn _test() -> i32 {{ {stmt_source} }}");
        let result = parse(&full);
        if result.has_errors() {
            panic!(
                "Expected no parse errors, got {} errors:\n{:#?}",
                result.errors.len(),
                result.errors
            );
        }
        let func = match &result.module.items[0].node {
            Item::Function(f) => f,
            _ => panic!("expected function"),
        };
        func.body.stmts[0].node.clone()
    }

    // ── Sample file tests ─────────────────────────────────────────────

    #[test]
    fn test_parse_hello() {
        let source = std::fs::read_to_string("../../tests/samples/hello.axm")
            .expect("failed to read hello.axm");
        let module = parse_ok(&source);

        assert_eq!(
            module.name.as_ref().map(|n| n.node.as_str()),
            Some("hello")
        );
        assert_eq!(module.items.len(), 1);
        match &module.items[0].node {
            Item::Function(f) => {
                assert_eq!(f.name.node, "main");
                assert_eq!(f.params.len(), 0);
                assert!(matches!(f.return_type, TypeExpr::Named(ref n) if n == "i32"));
                assert_eq!(f.body.stmts.len(), 2); // print call + return
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_parse_fibonacci() {
        let source = std::fs::read_to_string("../../tests/samples/fibonacci.axm")
            .expect("failed to read fibonacci.axm");
        let module = parse_ok(&source);

        assert_eq!(
            module.name.as_ref().map(|n| n.node.as_str()),
            Some("fibonacci")
        );
        assert_eq!(module.items.len(), 2); // fib + main

        // Check fib function
        match &module.items[0].node {
            Item::Function(f) => {
                assert_eq!(f.name.node, "fib");
                assert_eq!(f.params.len(), 1);
                assert_eq!(f.params[0].name.node, "n");
                assert!(matches!(f.return_type, TypeExpr::Named(ref n) if n == "i64"));

                // Check annotations: @pure and @complexity
                let has_pure = f
                    .annotations
                    .iter()
                    .any(|a| matches!(a.node, Annotation::Pure));
                assert!(has_pure, "expected @pure annotation on fib");
                let has_complexity = f
                    .annotations
                    .iter()
                    .any(|a| matches!(a.node, Annotation::Complexity(_)));
                assert!(has_complexity, "expected @complexity annotation on fib");

                // Check body has if, let, for, return statements
                assert!(f.body.stmts.len() >= 4);
            }
            _ => panic!("expected function"),
        }

        // Check main function
        match &module.items[1].node {
            Item::Function(f) => {
                assert_eq!(f.name.node, "main");
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_parse_matmul() {
        let source = std::fs::read_to_string("../../tests/samples/matmul_naive.axm")
            .expect("failed to read matmul_naive.axm");
        let result = parse(&source);

        // matmul_naive.axm should parse without errors
        if result.has_errors() {
            for err in &result.errors {
                eprintln!("  {:?}", err);
            }
            panic!(
                "Expected no parse errors for matmul_naive.axm, got {}",
                result.errors.len()
            );
        }

        let module = result.module;
        assert_eq!(
            module.name.as_ref().map(|n| n.node.as_str()),
            Some("matmul")
        );
        assert_eq!(module.items.len(), 1); // matmul function

        match &module.items[0].node {
            Item::Function(f) => {
                assert_eq!(f.name.node, "matmul");
                assert_eq!(f.params.len(), 2);

                // Check tensor types
                assert!(matches!(f.params[0].ty, TypeExpr::Tensor(_, _)));
                assert!(matches!(f.params[1].ty, TypeExpr::Tensor(_, _)));

                // Check return type is tensor
                assert!(matches!(f.return_type, TypeExpr::Tensor(_, _)));

                // Check @pure and @complexity annotations
                let has_pure = f
                    .annotations
                    .iter()
                    .any(|a| matches!(a.node, Annotation::Pure));
                assert!(has_pure);

                // Check @strategy block in body
                let has_strategy = f
                    .body
                    .annotations
                    .iter()
                    .any(|a| matches!(a.node, Annotation::Strategy(_)));
                assert!(has_strategy, "expected @strategy in function body");
            }
            _ => panic!("expected function"),
        }
    }

    // ── Annotation tests ──────────────────────────────────────────────

    #[test]
    fn test_annotations() {
        let source = r#"
@pure
@intent("Compute something")
@complexity O(n)
fn foo(n: i32) -> i32 {
    return n;
}
"#;
        let module = parse_ok(source);
        match &module.items[0].node {
            Item::Function(f) => {
                let has_pure = f
                    .annotations
                    .iter()
                    .any(|a| matches!(a.node, Annotation::Pure));
                assert!(has_pure);

                let intent = f.annotations.iter().find_map(|a| {
                    if let Annotation::Intent(ref s) = a.node {
                        Some(s.clone())
                    } else {
                        None
                    }
                });
                assert_eq!(intent.as_deref(), Some("Compute something"));

                let complexity = f.annotations.iter().find_map(|a| {
                    if let Annotation::Complexity(ref s) = a.node {
                        Some(s.clone())
                    } else {
                        None
                    }
                });
                assert!(complexity.is_some());
                assert!(complexity.as_ref().map_or(false, |s| s.contains("O")));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_strategy_block() {
        let source = r#"
fn foo() -> i32 {
    @strategy {
        tiling: { M: ?tile_m, N: ?tile_n }
        order: ?loop_order
        unroll: ?unroll_factor
    }
    return 0;
}
"#;
        let module = parse_ok(source);
        match &module.items[0].node {
            Item::Function(f) => {
                assert!(!f.body.annotations.is_empty());
                let strategy = f.body.annotations.iter().find_map(|a| {
                    if let Annotation::Strategy(ref s) = a.node {
                        Some(s)
                    } else {
                        None
                    }
                });
                assert!(strategy.is_some());
                let s = strategy.expect("strategy block");
                assert_eq!(s.entries.len(), 3);

                // Check tiling entry is a Map
                assert_eq!(s.entries[0].0, "tiling");
                assert!(matches!(s.entries[0].1, StrategyValue::Map(_)));

                // Check order is a Hole
                assert_eq!(s.entries[1].0, "order");
                assert!(matches!(s.entries[1].1, StrategyValue::Hole(ref n) if n == "loop_order"));

                // Check unroll is a Hole
                assert_eq!(s.entries[2].0, "unroll");
                assert!(
                    matches!(s.entries[2].1, StrategyValue::Hole(ref n) if n == "unroll_factor")
                );
            }
            _ => panic!("expected function"),
        }
    }

    // ── Precedence tests ──────────────────────────────────────────────

    #[test]
    fn test_precedence() {
        // 1 + 2 * 3 -> Add(1, Mul(2, 3))
        let expr = parse_expr_from("1 + 2 * 3");
        match expr {
            Expr::BinaryOp {
                op: BinOp::Add,
                lhs,
                rhs,
            } => {
                assert!(matches!(*lhs, Expr::IntLiteral(1)));
                match *rhs {
                    Expr::BinaryOp {
                        op: BinOp::Mul,
                        lhs: inner_l,
                        rhs: inner_r,
                    } => {
                        assert!(matches!(*inner_l, Expr::IntLiteral(2)));
                        assert!(matches!(*inner_r, Expr::IntLiteral(3)));
                    }
                    _ => panic!("expected Mul"),
                }
            }
            _ => panic!("expected Add"),
        }
    }

    #[test]
    fn test_precedence_comparison_over_logic() {
        let expr = parse_expr_from("a == b and c != d");
        match expr {
            Expr::BinaryOp {
                op: BinOp::And,
                lhs,
                rhs,
            } => {
                assert!(matches!(
                    *lhs,
                    Expr::BinaryOp {
                        op: BinOp::Eq,
                        ..
                    }
                ));
                assert!(matches!(
                    *rhs,
                    Expr::BinaryOp {
                        op: BinOp::NotEq,
                        ..
                    }
                ));
            }
            _ => panic!("expected And"),
        }
    }

    #[test]
    fn test_precedence_unary_neg() {
        let expr = parse_expr_from("-a + b");
        match expr {
            Expr::BinaryOp {
                op: BinOp::Add,
                lhs,
                rhs,
            } => {
                assert!(matches!(
                    *lhs,
                    Expr::UnaryOp {
                        op: UnaryOp::Neg,
                        ..
                    }
                ));
                assert!(matches!(*rhs, Expr::Ident(ref n) if n == "b"));
            }
            _ => panic!("expected Add with Neg prefix"),
        }
    }

    #[test]
    fn test_left_associativity() {
        // a - b - c -> Sub(Sub(a, b), c)
        let expr = parse_expr_from("a - b - c");
        match expr {
            Expr::BinaryOp {
                op: BinOp::Sub,
                lhs,
                rhs,
            } => {
                assert!(matches!(
                    *lhs,
                    Expr::BinaryOp {
                        op: BinOp::Sub,
                        ..
                    }
                ));
                assert!(matches!(*rhs, Expr::Ident(ref n) if n == "c"));
            }
            _ => panic!("expected left-associative Sub"),
        }
    }

    #[test]
    fn test_grouped_expression() {
        // (a + b) * c -> Mul(Add(a, b), c)
        let expr = parse_expr_from("(a + b) * c");
        match expr {
            Expr::BinaryOp {
                op: BinOp::Mul,
                lhs,
                rhs,
            } => {
                assert!(matches!(
                    *lhs,
                    Expr::BinaryOp {
                        op: BinOp::Add,
                        ..
                    }
                ));
                assert!(matches!(*rhs, Expr::Ident(ref n) if n == "c"));
            }
            _ => panic!("expected Mul with grouped Add"),
        }
    }

    // ── Type expression tests ─────────────────────────────────────────

    #[test]
    fn test_type_expr() {
        // Test named type
        let source = "fn foo(x: i32) -> i32 { return x; }";
        let module = parse_ok(source);
        match &module.items[0].node {
            Item::Function(f) => {
                assert!(matches!(f.params[0].ty, TypeExpr::Named(ref n) if n == "i32"));
                assert!(matches!(f.return_type, TypeExpr::Named(ref n) if n == "i32"));
            }
            _ => panic!("expected function"),
        }

        // Test tensor type
        let source = "fn foo(x: tensor[f32, M, N]) -> i32 { return 0; }";
        let module = parse_ok(source);
        match &module.items[0].node {
            Item::Function(f) => match &f.params[0].ty {
                TypeExpr::Tensor(elem, dims) => {
                    assert!(matches!(elem.as_ref(), TypeExpr::Named(ref n) if n == "f32"));
                    assert_eq!(dims.len(), 2);
                    assert!(matches!(dims[0], DimExpr::Named(ref n) if n == "M"));
                    assert!(matches!(dims[1], DimExpr::Named(ref n) if n == "N"));
                }
                _ => panic!("expected tensor type"),
            },
            _ => panic!("expected function"),
        }

        // Test array type
        let source = "fn foo(x: array[f32, 1024]) -> i32 { return 0; }";
        let module = parse_ok(source);
        match &module.items[0].node {
            Item::Function(f) => {
                assert!(matches!(f.params[0].ty, TypeExpr::Array(_, _)));
            }
            _ => panic!("expected function"),
        }

        // Test tuple type
        let source = "fn foo(x: (i32, f64, bool)) -> i32 { return 0; }";
        let module = parse_ok(source);
        match &module.items[0].node {
            Item::Function(f) => match &f.params[0].ty {
                TypeExpr::Tuple(types) => {
                    assert_eq!(types.len(), 3);
                }
                _ => panic!("expected tuple type"),
            },
            _ => panic!("expected function"),
        }

        // Test fn type
        let source = "fn foo(x: fn(i32, f64) -> bool) -> i32 { return 0; }";
        let module = parse_ok(source);
        match &module.items[0].node {
            Item::Function(f) => match &f.params[0].ty {
                TypeExpr::Fn(params, ret) => {
                    assert_eq!(params.len(), 2);
                    assert!(matches!(ret.as_ref(), TypeExpr::Named(ref n) if n == "bool"));
                }
                _ => panic!("expected fn type"),
            },
            _ => panic!("expected function"),
        }
    }

    // ── Error recovery tests ──────────────────────────────────────────

    #[test]
    fn test_error_recovery() {
        // Multiple errors: missing semicolon + second statement
        let source = r#"
fn foo() -> i32 {
    let x: i32 = 1
    let y: i32 = 2;
    return y;
}
"#;
        let result = parse(source);
        assert!(result.has_errors());
        // Should still parse both let statements and the return
        match &result.module.items[0].node {
            Item::Function(f) => {
                // We should get statements despite the missing semicolon
                assert!(!f.body.stmts.is_empty());
            }
            _ => panic!("expected function"),
        }
    }

    // ── OptHole test ──────────────────────────────────────────────────

    #[test]
    fn test_opt_hole() {
        let expr = parse_expr_from("?tile_size");
        assert!(matches!(expr, Expr::OptHole(ref n) if n == "tile_size"));
    }

    // ── If/else test ──────────────────────────────────────────────────

    #[test]
    fn test_if_else() {
        let stmt = parse_stmt_from("if x > 0 { return x; } else { return 0; }");
        match stmt {
            Stmt::If {
                condition,
                then_block,
                else_block,
            } => {
                assert!(matches!(
                    condition,
                    Expr::BinaryOp {
                        op: BinOp::Gt,
                        ..
                    }
                ));
                assert_eq!(then_block.stmts.len(), 1);
                assert!(else_block.is_some());
                assert_eq!(else_block.as_ref().map(|b| b.stmts.len()), Some(1));
            }
            _ => panic!("expected if statement"),
        }
    }

    // ── For loop test ─────────────────────────────────────────────────

    #[test]
    fn test_for_loop() {
        let stmt = parse_stmt_from("for i: i32 in range(10) { x = x + 1; }");
        match stmt {
            Stmt::For {
                var,
                var_type,
                iterable,
                body,
            } => {
                assert_eq!(var.node, "i");
                assert!(matches!(var_type, TypeExpr::Named(ref n) if n == "i32"));
                assert!(matches!(iterable, Expr::Call { .. }));
                assert_eq!(body.stmts.len(), 1);
            }
            _ => panic!("expected for statement"),
        }
    }

    // ── Let binding test ──────────────────────────────────────────────

    #[test]
    fn test_let_binding() {
        let stmt = parse_stmt_from("let x: i32 = 42;");
        match stmt {
            Stmt::Let {
                name,
                ty,
                value,
                mutable,
            } => {
                assert_eq!(name.node, "x");
                assert!(matches!(ty, TypeExpr::Named(ref n) if n == "i32"));
                assert!(matches!(value, Expr::IntLiteral(42)));
                assert!(!mutable);
            }
            _ => panic!("expected let statement"),
        }
    }

    #[test]
    fn test_let_mut_binding() {
        let stmt = parse_stmt_from("let mut x: i32 = 0;");
        match stmt {
            Stmt::Let {
                name,
                mutable,
                ..
            } => {
                assert_eq!(name.node, "x");
                assert!(mutable);
            }
            _ => panic!("expected let statement"),
        }
    }

    // ── Call and index tests ──────────────────────────────────────────

    #[test]
    fn test_call_and_index() {
        // Function call
        let expr = parse_expr_from("f(x, y)");
        match expr {
            Expr::Call { func, args } => {
                assert!(matches!(*func, Expr::Ident(ref n) if n == "f"));
                assert_eq!(args.len(), 2);
            }
            _ => panic!("expected call"),
        }

        // Multi-index
        let expr = parse_expr_from("a[i, j]");
        match expr {
            Expr::Index { expr, indices } => {
                assert!(matches!(*expr, Expr::Ident(ref n) if n == "a"));
                assert_eq!(indices.len(), 2);
            }
            _ => panic!("expected index"),
        }
    }

    #[test]
    fn test_method_call() {
        let expr = parse_expr_from("obj.method(x)");
        match expr {
            Expr::MethodCall {
                expr,
                method,
                args,
            } => {
                assert!(matches!(*expr, Expr::Ident(ref n) if n == "obj"));
                assert_eq!(method, "method");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("expected method call"),
        }
    }

    #[test]
    fn test_field_access() {
        let expr = parse_expr_from("a.b");
        match expr {
            Expr::FieldAccess { expr, field } => {
                assert!(matches!(*expr, Expr::Ident(ref n) if n == "a"));
                assert_eq!(field, "b");
            }
            _ => panic!("expected field access"),
        }
    }

    #[test]
    fn test_chained_postfix() {
        // a.b.c(x)
        let expr = parse_expr_from("a.b.c(x)");
        match expr {
            Expr::MethodCall {
                expr,
                method,
                args,
            } => {
                assert_eq!(method, "c");
                assert_eq!(args.len(), 1);
                match *expr {
                    Expr::FieldAccess { expr: inner, field } => {
                        assert_eq!(field, "b");
                        assert!(matches!(*inner, Expr::Ident(ref n) if n == "a"));
                    }
                    _ => panic!("expected field access"),
                }
            }
            _ => panic!("expected method call"),
        }
    }

    #[test]
    fn test_wrapping_operators() {
        let expr = parse_expr_from("a +% b");
        assert!(matches!(
            expr,
            Expr::BinaryOp {
                op: BinOp::AddWrap,
                ..
            }
        ));

        let expr = parse_expr_from("a -| b");
        assert!(matches!(
            expr,
            Expr::BinaryOp {
                op: BinOp::SubSat,
                ..
            }
        ));

        let expr = parse_expr_from("a *% b");
        assert!(matches!(
            expr,
            Expr::BinaryOp {
                op: BinOp::MulWrap,
                ..
            }
        ));
    }

    #[test]
    fn test_conversion_keywords() {
        let expr = parse_expr_from("widen(n)");
        match expr {
            Expr::Call { func, args } => {
                assert!(matches!(*func, Expr::Ident(ref n) if n == "widen"));
                assert_eq!(args.len(), 1);
            }
            _ => panic!("expected call to widen"),
        }
    }

    #[test]
    fn test_empty_module() {
        let module = parse_ok("");
        assert!(module.name.is_none());
        assert!(module.items.is_empty());
    }

    #[test]
    fn test_module_with_only_annotations() {
        let source = r#"@module test;
@intent("A module with no functions");"#;
        let module = parse_ok(source);
        assert_eq!(
            module.name.as_ref().map(|n| n.node.as_str()),
            Some("test")
        );
        assert!(module.items.is_empty());
    }

    #[test]
    fn test_empty_function_body() {
        let module = parse_ok("fn foo() -> i32 { }");
        match &module.items[0].node {
            Item::Function(f) => {
                assert!(f.body.stmts.is_empty());
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_trailing_comma_in_params() {
        let module = parse_ok("fn foo(a: i32, b: i64,) -> i32 { return 0; }");
        match &module.items[0].node {
            Item::Function(f) => {
                assert_eq!(f.params.len(), 2);
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_constraint_annotation() {
        let source = r#"
@constraint { correctness: "IEEE 754 compliant" }
fn foo() -> i32 { return 0; }
"#;
        let module = parse_ok(source);
        match &module.items[0].node {
            Item::Function(f) => {
                let constraint = f.annotations.iter().find_map(|a| {
                    if let Annotation::Constraint(ref kvs) = a.node {
                        Some(kvs.clone())
                    } else {
                        None
                    }
                });
                assert!(constraint.is_some());
                let kvs = constraint.expect("constraint");
                assert_eq!(kvs.len(), 1);
                assert_eq!(kvs[0].0, "correctness");
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_while_loop() {
        let stmt = parse_stmt_from("while x > 0 { x = x - 1; }");
        match stmt {
            Stmt::While {
                condition, body, ..
            } => {
                assert!(matches!(
                    condition,
                    Expr::BinaryOp {
                        op: BinOp::Gt,
                        ..
                    }
                ));
                assert_eq!(body.stmts.len(), 1);
            }
            _ => panic!("expected while statement"),
        }
    }

    #[test]
    fn test_assign_stmt() {
        let stmt = parse_stmt_from("x = 42;");
        match stmt {
            Stmt::Assign { target, value } => {
                assert!(matches!(target, Expr::Ident(ref n) if n == "x"));
                assert!(matches!(value, Expr::IntLiteral(42)));
            }
            _ => panic!("expected assign statement"),
        }
    }

    #[test]
    fn test_nested_exprs() {
        let expr = parse_expr_from("((((a + b) * c) - d) / e)");
        match expr {
            Expr::BinaryOp {
                op: BinOp::Div, ..
            } => {} // Success — deeply nested is fine
            _ => panic!("expected Div at top level"),
        }
    }

    #[test]
    fn test_string_literal() {
        let expr = parse_expr_from("\"hello world\"");
        match expr {
            Expr::StringLiteral(s) => assert_eq!(s, "hello world"),
            _ => panic!("expected string literal"),
        }
    }

    #[test]
    fn test_bool_literal() {
        let expr = parse_expr_from("true");
        assert!(matches!(expr, Expr::BoolLiteral(true)));

        let expr = parse_expr_from("false");
        assert!(matches!(expr, Expr::BoolLiteral(false)));
    }

    #[test]
    fn test_float_literal() {
        let expr = parse_expr_from("3.14");
        match expr {
            Expr::FloatLiteral(v) => {
                assert!((v - 3.14).abs() < f64::EPSILON);
            }
            _ => panic!("expected float literal"),
        }
    }

    #[test]
    fn test_target_annotation() {
        let source = r#"
@target { cpu.simd }
fn foo() -> i32 { return 0; }
"#;
        let module = parse_ok(source);
        match &module.items[0].node {
            Item::Function(f) => {
                let target = f.annotations.iter().find_map(|a| {
                    if let Annotation::Target(ref targets) = a.node {
                        Some(targets.clone())
                    } else {
                        None
                    }
                });
                assert!(target.is_some());
                let targets = target.expect("target");
                assert_eq!(targets.len(), 1);
                assert_eq!(targets[0], "cpu.simd");
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_inline_annotation() {
        let source = r#"
@inline(always)
fn foo() -> i32 { return 0; }
"#;
        let module = parse_ok(source);
        match &module.items[0].node {
            Item::Function(f) => {
                let has_inline = f.annotations.iter().any(|a| {
                    matches!(a.node, Annotation::Inline(InlineHint::Always))
                });
                assert!(has_inline);
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_vectorizable_annotation() {
        let source = r#"
@vectorizable(i, j, k)
fn foo() -> i32 { return 0; }
"#;
        let module = parse_ok(source);
        match &module.items[0].node {
            Item::Function(f) => {
                let dims = f.annotations.iter().find_map(|a| {
                    if let Annotation::Vectorizable(ref d) = a.node {
                        Some(d.clone())
                    } else {
                        None
                    }
                });
                assert_eq!(dims, Some(vec!["i".to_string(), "j".to_string(), "k".to_string()]));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_expr_stmt() {
        let stmt = parse_stmt_from("print(42);");
        match stmt {
            Stmt::Expr(Expr::Call { func, args }) => {
                assert!(matches!(*func, Expr::Ident(ref n) if n == "print"));
                assert_eq!(args.len(), 1);
            }
            _ => panic!("expected expression statement with call"),
        }
    }

    #[test]
    fn test_extern_function() {
        let result = parse("extern fn sin(x: f64) -> f64;");
        assert!(
            !result.has_errors(),
            "Parse errors: {:?}",
            result.errors
        );
        assert_eq!(result.module.items.len(), 1);
        match &result.module.items[0].node {
            Item::ExternFunction(ef) => {
                assert_eq!(ef.name.node, "sin");
                assert_eq!(ef.params.len(), 1);
                assert_eq!(ef.params[0].name.node, "x");
                assert!(matches!(ef.return_type, TypeExpr::Named(ref n) if n == "f64"));
            }
            _ => panic!("expected ExternFunction"),
        }
    }

    #[test]
    fn test_extern_function_no_params() {
        let result = parse("extern fn clock() -> i64;");
        assert!(
            !result.has_errors(),
            "Parse errors: {:?}",
            result.errors
        );
        assert_eq!(result.module.items.len(), 1);
        match &result.module.items[0].node {
            Item::ExternFunction(ef) => {
                assert_eq!(ef.name.node, "clock");
                assert!(ef.params.is_empty());
                assert!(matches!(ef.return_type, TypeExpr::Named(ref n) if n == "i64"));
            }
            _ => panic!("expected ExternFunction"),
        }
    }

    #[test]
    fn test_extern_function_void_return() {
        let result = parse("extern fn free(p: ptr[u8]);");
        assert!(
            !result.has_errors(),
            "Parse errors: {:?}",
            result.errors
        );
        assert_eq!(result.module.items.len(), 1);
        match &result.module.items[0].node {
            Item::ExternFunction(ef) => {
                assert_eq!(ef.name.node, "free");
                assert_eq!(ef.params.len(), 1);
                // No arrow, so void return type
                assert!(matches!(ef.return_type, TypeExpr::Named(ref n) if n == "void"));
            }
            _ => panic!("expected ExternFunction"),
        }
    }

    #[test]
    fn test_export_annotation() {
        let result = parse(
            r#"
@export
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
"#,
        );
        assert!(
            !result.has_errors(),
            "Parse errors: {:?}",
            result.errors
        );
        assert_eq!(result.module.items.len(), 1);
        match &result.module.items[0].node {
            Item::Function(f) => {
                assert_eq!(f.name.node, "add");
                assert!(
                    f.annotations
                        .iter()
                        .any(|a| matches!(a.node, Annotation::Export)),
                    "should have @export annotation"
                );
            }
            _ => panic!("expected Function"),
        }
    }

    #[test]
    fn test_ffi_test_sample() {
        let source = r#"
@module ffi_test;
extern fn clock() -> i64;

@export
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

fn main() -> i32 {
    let t: i64 = clock();
    print_i64(t);
    return 0;
}
"#;
        let result = parse(source);
        assert!(
            !result.has_errors(),
            "Parse errors: {:?}",
            result.errors
        );
        // Should have 3 items: extern fn clock, fn add, fn main
        assert_eq!(result.module.items.len(), 3);
        assert!(matches!(
            result.module.items[0].node,
            Item::ExternFunction(_)
        ));
        assert!(matches!(result.module.items[1].node, Item::Function(_)));
        assert!(matches!(result.module.items[2].node, Item::Function(_)));
    }

    // ── Array support tests ─────────────────────────────────────────────

    #[test]
    fn test_array_type_expr() {
        let source = "fn test(data: array[i32, 100]) -> i32 { return 0; }";
        let module = parse_ok(source);
        match &module.items[0].node {
            Item::Function(f) => {
                assert!(
                    matches!(&f.params[0].ty, TypeExpr::Array(elem, size)
                        if matches!(**elem, TypeExpr::Named(ref n) if n == "i32")
                        && matches!(**size, Expr::IntLiteral(100))
                    ),
                    "expected array[i32, 100] type, got {:?}",
                    f.params[0].ty
                );
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_array_zeros_expr() {
        let stmt = parse_stmt_from("let arr: array[i32, 10] = array_zeros[i32, 10];");
        match stmt {
            Stmt::Let { ty, value, .. } => {
                assert!(
                    matches!(&ty, TypeExpr::Array(_, _)),
                    "expected array type, got {:?}",
                    ty
                );
                assert!(
                    matches!(&value, Expr::ArrayZeros { element_type, size }
                        if matches!(element_type, TypeExpr::Named(ref n) if n == "i32")
                        && matches!(**size, Expr::IntLiteral(10))
                    ),
                    "expected array_zeros[i32, 10], got {:?}",
                    value
                );
            }
            _ => panic!("expected let statement"),
        }
    }

    #[test]
    fn test_array_index_expr() {
        let expr = parse_expr_from("arr[5]");
        assert!(
            matches!(&expr, Expr::Index { expr: base, indices }
                if matches!(**base, Expr::Ident(ref n) if n == "arr")
                && indices.len() == 1
                && matches!(indices[0], Expr::IntLiteral(5))
            ),
            "expected arr[5] index expr, got {:?}",
            expr
        );
    }

    #[test]
    fn test_array_full_program_parse() {
        let source = r#"
@module array_test;
fn main() -> i32 {
    let arr: array[i32, 10] = array_zeros[i32, 10];
    for i: i32 in range(0, 10) {
        arr[i] = i * i;
    }
    let sum: i32 = 0;
    for i: i32 in range(0, 10) {
        sum = sum + arr[i];
    }
    print_i32(sum);
    return 0;
}
"#;
        let result = parse(source);
        assert!(
            !result.has_errors(),
            "array program should parse without errors: {:?}",
            result.errors
        );
        assert_eq!(
            result.module.name.as_ref().map(|n| n.node.as_str()),
            Some("array_test")
        );
    }
}
