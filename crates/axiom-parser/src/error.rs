//! Parse error types with source spans for diagnostic rendering.
//!
//! Every error variant carries a [`miette::SourceSpan`] so that error
//! messages can point to the exact location in source text.
//!
//! The `unused_assignments` lint is suppressed at the module level because
//! `thiserror` and `miette` derive macros consume the named fields in
//! format strings, but rustc does not see those reads.
#![allow(unused_assignments)]

use crate::ast::Module;
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

/// All possible errors produced during parsing.
#[derive(Debug, Clone, Error, Diagnostic)]
pub enum ParseError {
    /// A token was found that does not match what the grammar expects.
    #[error("expected {expected}, found {found}")]
    #[diagnostic(code(axiom::parse::unexpected_token))]
    UnexpectedToken {
        expected: String,
        found: String,
        #[label("unexpected token here")]
        span: SourceSpan,
    },

    /// The token stream ended before the parser finished a production.
    #[error("unexpected end of file, expected {expected}")]
    #[diagnostic(code(axiom::parse::unexpected_eof))]
    UnexpectedEof {
        expected: String,
        #[label("file ends here")]
        span: SourceSpan,
    },

    /// An annotation has invalid syntax or arguments.
    #[error("invalid annotation @{name}: {reason}")]
    #[diagnostic(code(axiom::parse::invalid_annotation))]
    InvalidAnnotation {
        name: String,
        reason: String,
        #[label("this annotation")]
        span: SourceSpan,
    },

    /// A type expression could not be parsed.
    #[error("invalid type expression: {detail}")]
    #[diagnostic(code(axiom::parse::invalid_type))]
    InvalidTypeExpression {
        detail: String,
        #[label("here")]
        span: SourceSpan,
    },

    /// A statement-ending semicolon is missing.
    #[error("missing semicolon")]
    #[diagnostic(
        code(axiom::parse::missing_semicolon),
        help("add `;` at the end of this statement")
    )]
    MissingSemicolon {
        #[label("expected `;` after this")]
        span: SourceSpan,
    },

    /// A closing delimiter (`}`, `)`, `]`) is missing.
    #[error("missing closing `{delimiter}`")]
    #[diagnostic(code(axiom::parse::missing_delimiter))]
    MissingClosingDelimiter {
        delimiter: char,
        #[label("opened here")]
        open_span: SourceSpan,
        #[label("expected closing `{delimiter}` before here")]
        span: SourceSpan,
    },

    /// An expression could not be parsed.
    #[error("invalid expression: {detail}")]
    #[diagnostic(code(axiom::parse::invalid_expression))]
    InvalidExpression {
        detail: String,
        #[label("here")]
        span: SourceSpan,
    },

    /// A lexer error token was encountered during parsing.
    #[error("lexer error: {message}")]
    #[diagnostic(code(axiom::lex::error))]
    LexerError {
        message: String,
        #[label("here")]
        span: SourceSpan,
    },
}

/// Result of parsing an AXIOM source string.
///
/// Always contains a [`Module`] (possibly partial due to error recovery)
/// and all collected [`ParseError`]s. A parse is considered successful
/// when `errors` is empty.
#[derive(Debug)]
pub struct ParseResult {
    /// The parsed module AST (may be partial if errors occurred).
    pub module: Module,
    /// All errors collected during parsing.
    pub errors: Vec<ParseError>,
}

impl ParseResult {
    /// Returns `true` if any errors were collected during parsing.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Convert an `axiom_lexer::Span` to a `miette::SourceSpan`.
pub(crate) fn span_to_source_span(span: axiom_lexer::Span) -> SourceSpan {
    SourceSpan::from((span.start as usize, (span.end - span.start) as usize))
}
