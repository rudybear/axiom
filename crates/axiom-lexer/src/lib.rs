//! AXIOM Lexer — tokenizes `.axm` source into a stream of typed tokens.
//!
//! Design principles:
//! - Every token carries a Span (byte offset range)
//! - Error recovery: invalid chars produce Error tokens, lexing continues
//! - Annotations (`@name`) and optimization holes (`?name`) are first-class tokens
//! - Line/column positions available on demand via [`LineIndex`]

pub mod token;
pub mod lexer;

pub use token::{Token, TokenKind, Span, IntBase, IntSuffix, FloatSuffix, LineIndex};
pub use lexer::Lexer;
