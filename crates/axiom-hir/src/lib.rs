//! axiom-hir -- High-level IR preserving semantic annotations.
//!
//! The HIR is produced by lowering the AST. It validates annotation placement,
//! checks type references against known primitives and user-defined types,
//! and assigns unique node IDs for later compiler phases to reference.
//!
//! # Usage
//!
//! ```ignore
//! let ast_module = axiom_parser::parse(source).module;
//! let hir_module = axiom_hir::lower(&ast_module)?;
//! println!("{}", hir_module);
//! ```

pub mod display;
pub mod error;
pub mod hir;
pub mod lower;

pub use display::display_hir;
pub use error::{AnnotationTarget, LowerError};
pub use hir::*;
pub use lower::lower;
