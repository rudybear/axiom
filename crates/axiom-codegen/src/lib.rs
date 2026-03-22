//! axiom-codegen -- LLVM IR code generation from HIR.
//!
//! Generates LLVM IR as text from an [`axiom_hir::HirModule`]. Uses an
//! alloca-based strategy for local variables. The output is valid `.ll` format
//! that can be compiled with `llc` and linked with `clang`.
//!
//! # Usage
//!
//! ```ignore
//! let hir_module = axiom_hir::lower(&ast_module)?;
//! let llvm_ir = axiom_codegen::codegen(&hir_module)?;
//! println!("{llvm_ir}");
//! ```

pub mod error;
pub mod llvm;

pub use error::CodegenError;
pub use llvm::codegen;
