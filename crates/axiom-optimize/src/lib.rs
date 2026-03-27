//! axiom-optimize — AI optimization protocol for AXIOM programs.
//!
//! This crate provides the core optimization loop infrastructure:
//!
//! - **Surface extraction**: Discover all tunable `?holes` and `@strategy`
//!   blocks in an AXIOM source program and return structured [`surface::OptSurface`]
//!   descriptors.
//! - **Proposal validation**: Check that a proposed set of concrete values
//!   matches the declared types and ranges before applying them.
//! - **Transfer protocol**: Extract and generate `@transfer` blocks for
//!   inter-agent handoff metadata.
//! - **Agent API**: High-level [`agent_api::AgentSession`] for AI agents to
//!   load programs, apply proposals, track history, and export with transfer info.
//!
//! # Example
//!
//! ```
//! use axiom_optimize::surface::extract_surfaces;
//! use axiom_optimize::proposal::{Proposal, validate_proposal};
//! use axiom_optimize::surface::Value;
//!
//! let source = r#"
//! fn add(a: i32, b: i32) -> i32 {
//!     @strategy {
//!         unroll: ?unroll_factor
//!     }
//!     return a + b;
//! }
//! "#;
//!
//! // Extract optimisation surfaces
//! let surfaces = extract_surfaces(source).expect("valid source");
//! assert_eq!(surfaces.len(), 1);
//! assert_eq!(surfaces[0].holes[0].name, "unroll_factor");
//!
//! // Build and validate a proposal
//! let mut proposal = Proposal::new();
//! proposal.set("unroll_factor", Value::Int(4));
//! validate_proposal(&proposal, &surfaces).expect("valid proposal");
//! ```

pub mod agent_api;
pub mod benchmark;
pub mod fuzz;
pub mod history;
pub mod llm_optimizer;
pub mod proposal;
pub mod surface;
pub mod transfer;

// Re-export key types at the crate root for convenience.
pub use agent_api::{AgentError, AgentSession, SessionSummary};
pub use history::{HistoryError, OptHistory, OptRecord};
pub use proposal::{Proposal, ValidationError, validate_proposal};
pub use surface::{
    HoleType, OptHole, OptSurface, StrategyEntry, StrategyEntryValue, StrategyInfo, Value,
    extract_surfaces, extract_surfaces_from_hir,
};
pub use transfer::{Confidence, TransferError, TransferInfo, extract_transfer, generate_transfer};
pub use fuzz::{FuzzRange, extract_fuzz_ranges, generate_fuzz_inputs};
