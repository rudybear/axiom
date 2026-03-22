//! Codegen error types with diagnostic integration.

/// Errors that can occur during LLVM IR code generation.
#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    /// A type was encountered that the codegen does not yet support.
    #[error("unsupported type `{ty}` in {context}")]
    UnsupportedType {
        /// The type that was unsupported.
        ty: String,
        /// Where the type was encountered.
        context: String,
    },

    /// An expression kind was encountered that codegen does not yet handle.
    #[error("unsupported expression `{expr}` in {context}")]
    UnsupportedExpression {
        /// The expression kind.
        expr: String,
        /// Where the expression was encountered.
        context: String,
    },

    /// A statement kind was encountered that codegen does not yet handle.
    #[error("unsupported statement `{stmt}` in {context}")]
    UnsupportedStatement {
        /// The statement kind.
        stmt: String,
        /// Where the statement was encountered.
        context: String,
    },

    /// A variable was referenced that has no alloca in the current scope.
    #[error("undefined variable `{name}`")]
    UndefinedVariable {
        /// The variable name.
        name: String,
    },

    /// A function was called that has not been defined or declared.
    #[error("undefined function `{name}`")]
    UndefinedFunction {
        /// The function name.
        name: String,
    },

    /// Types did not match where they were expected to.
    #[error("type mismatch: expected `{expected}`, found `{found}` in {context}")]
    TypeMismatch {
        /// The expected type.
        expected: String,
        /// The found type.
        found: String,
        /// Where the mismatch was encountered.
        context: String,
    },

    /// The main function has an invalid signature.
    #[error("invalid main function: {reason}")]
    InvalidMain {
        /// The reason the main function is invalid.
        reason: String,
    },

    /// An internal invariant was violated (compiler bug).
    #[error("internal codegen error: {message}")]
    InternalError {
        /// Description of the internal error.
        message: String,
    },
}
