//! Error types for HIR lowering.
//!
//! All errors carry source spans for diagnostic rendering via [`miette`].
//! The lowering pass collects all errors rather than stopping at the first one,
//! enabling comprehensive feedback to the user or AI agent.
//!
//! The `unused_assignments` lint is suppressed at the module level because
//! `thiserror` and `miette` derive macros consume the named fields in
//! format strings, but rustc does not see those reads.
#![allow(unused_assignments)]

use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

/// Identifies what kind of item an annotation is attached to, for validation purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationTarget {
    /// Top-level module.
    Module,
    /// Function definition.
    Function,
    /// Function parameter.
    Param,
    /// Struct definition.
    StructDef,
    /// Struct field.
    StructField,
    /// Block of statements.
    Block,
}

impl std::fmt::Display for AnnotationTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Module => write!(f, "module"),
            Self::Function => write!(f, "function"),
            Self::Param => write!(f, "parameter"),
            Self::StructDef => write!(f, "struct"),
            Self::StructField => write!(f, "struct field"),
            Self::Block => write!(f, "block"),
        }
    }
}

/// All possible errors produced during AST-to-HIR lowering.
#[derive(Debug, Clone, Error, Diagnostic)]
pub enum LowerError {
    /// An annotation was placed on an invalid target.
    #[error("@{annotation} is not valid on {target}; valid on: {valid_targets}")]
    #[diagnostic(code(axiom::hir::invalid_annotation_target))]
    InvalidAnnotationTarget {
        /// Name of the annotation (e.g., "pure").
        annotation: String,
        /// What it was placed on (e.g., "parameter").
        target: String,
        /// Comma-separated list of valid targets.
        valid_targets: String,
        /// Source location of the annotation.
        #[label("this annotation")]
        span: SourceSpan,
    },

    /// A type name could not be resolved to any known primitive or user-defined type.
    #[error("unknown type `{name}`")]
    #[diagnostic(
        code(axiom::hir::unknown_type),
        help("known primitive types: i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, f16, bf16, f32, f64, bool")
    )]
    UnknownType {
        /// The unresolved type name.
        name: String,
        /// Source location of the type reference.
        #[label("unknown type")]
        span: SourceSpan,
    },

    /// A function, struct, or type alias was defined more than once with the same name.
    #[error("duplicate {kind} definition `{name}`")]
    #[diagnostic(code(axiom::hir::duplicate_definition))]
    DuplicateDefinition {
        /// The duplicated name.
        name: String,
        /// Kind of definition ("function", "struct", or "type alias").
        kind: String,
        /// Location of the first definition.
        #[label("first defined here")]
        first_span: SourceSpan,
        /// Location of the duplicate definition.
        #[label("duplicate definition")]
        second_span: SourceSpan,
    },

    /// Multiple `@module` annotations were found.
    #[error("multiple @module annotations found; only one is allowed")]
    #[diagnostic(code(axiom::hir::duplicate_module))]
    DuplicateModuleAnnotation {
        /// Location of the duplicate `@module` annotation.
        #[label("duplicate @module")]
        span: SourceSpan,
    },
}

/// Convert an `axiom_lexer::Span` to a `miette::SourceSpan`.
pub(crate) fn span_to_source_span(span: axiom_lexer::Span) -> SourceSpan {
    SourceSpan::from((span.start as usize, (span.end - span.start) as usize))
}
