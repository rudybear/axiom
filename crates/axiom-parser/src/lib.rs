//! AXIOM Parser — transforms token stream into a typed AST.
//!
//! The parser uses recursive descent with Pratt parsing for expressions.
//! It recovers from errors by skipping to synchronization points and
//! collecting all errors rather than stopping at the first one.
//!
//! # Usage
//!
//! ```
//! let result = axiom_parser::parse("fn main() -> i32 { return 0; }");
//! assert!(!result.has_errors());
//! println!("{:#?}", result.module);
//! ```

pub mod ast;
pub mod error;
pub mod parser;

pub use ast::*;
pub use error::{ParseError, ParseResult};
pub use parser::parse;
