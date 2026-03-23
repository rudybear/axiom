//! Transfer protocol for inter-agent handoff.
//!
//! This module provides types and functions for the `@transfer` block in AXIOM
//! programs. A transfer block carries metadata about which AI agent produced
//! the current program state, which agent should pick it up next, any open
//! questions, and confidence scores.
//!
//! # Example
//!
//! ```
//! use axiom_optimize::transfer::{TransferInfo, Confidence, extract_transfer, generate_transfer};
//!
//! let info = TransferInfo {
//!     source_agent: Some("optimizer-v1".to_string()),
//!     target_agent: Some("verifier-v2".to_string()),
//!     context: Some("Tiling applied to matmul inner loop".to_string()),
//!     open_questions: vec!["Is prefetch distance optimal?".to_string()],
//!     confidence: Some(Confidence {
//!         correctness: 0.95,
//!         optimality: 0.7,
//!     }),
//! };
//!
//! let text = generate_transfer(&info);
//! assert!(text.contains("optimizer-v1"));
//! assert!(text.contains("verifier-v2"));
//!
//! // Round-trip through AXIOM source
//! let source = format!("fn main() -> i32 {{\n    {text}\n    return 0;\n}}");
//! let extracted = extract_transfer(&source);
//! assert!(extracted.is_some());
//! let extracted = extracted.unwrap();
//! assert_eq!(extracted.source_agent.as_deref(), Some("optimizer-v1"));
//! assert_eq!(extracted.target_agent.as_deref(), Some("verifier-v2"));
//! ```

use axiom_hir::hir::{HirAnnotationKind, HirModule};
use axiom_parser::ast::TransferBlock;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Transfer metadata for inter-agent handoff.
///
/// Captures structured information about which agent produced the current
/// program state and which agent should consume it next.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransferInfo {
    /// The agent that produced the current program state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_agent: Option<String>,

    /// The agent that should pick up the work next.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_agent: Option<String>,

    /// Free-form context describing what was done or what to do next.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,

    /// Open questions or issues for the next agent to address.
    #[serde(default)]
    pub open_questions: Vec<String>,

    /// Confidence scores for the current program state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

/// Confidence scores for an optimization result.
///
/// Both values are expected to be in the range `[0.0, 1.0]`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Confidence {
    /// Confidence that the result is functionally correct.
    pub correctness: f64,
    /// Confidence that the result is near-optimal.
    pub optimality: f64,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when working with the transfer protocol.
#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    /// The source failed to parse.
    #[error("parse error: {0}")]
    Parse(String),
    /// The source failed to lower to HIR.
    #[error("lowering error: {0}")]
    Lower(String),
}

// ---------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------

/// Extract transfer info from an AXIOM source string.
///
/// Parses the source into HIR and searches all annotations (module-level,
/// function-level, and block-level) for a `@transfer` block. Returns the
/// first one found, or `None` if none is present.
///
/// Returns `None` if the source cannot be parsed or contains no `@transfer`.
pub fn extract_transfer(source: &str) -> Option<TransferInfo> {
    let parse_result = axiom_parser::parse(source);
    if parse_result.has_errors() {
        return None;
    }

    let hir_module = axiom_hir::lower(&parse_result.module).ok()?;

    extract_transfer_from_hir(&hir_module)
}

/// Extract transfer info from a pre-lowered HIR module.
///
/// Searches module-level annotations, then function-level annotations, then
/// function body block annotations for a `@transfer` block. Returns the
/// first one found.
pub fn extract_transfer_from_hir(module: &HirModule) -> Option<TransferInfo> {
    // Search module-level annotations
    for ann in &module.annotations {
        if let HirAnnotationKind::Transfer(ref tb) = ann.kind {
            return Some(transfer_block_to_info(tb));
        }
    }

    // Search function-level and block-level annotations
    for func in &module.functions {
        for ann in &func.annotations {
            if let HirAnnotationKind::Transfer(ref tb) = ann.kind {
                return Some(transfer_block_to_info(tb));
            }
        }
        for ann in &func.body.annotations {
            if let HirAnnotationKind::Transfer(ref tb) = ann.kind {
                return Some(transfer_block_to_info(tb));
            }
        }
    }

    None
}

/// Generate a `@transfer { ... }` block as AXIOM source text.
///
/// The output is suitable for embedding directly in an AXIOM source file
/// as a block-level or module-level annotation.
pub fn generate_transfer(info: &TransferInfo) -> String {
    let mut lines = Vec::new();
    lines.push("@transfer {".to_string());

    if let Some(ref agent) = info.source_agent {
        lines.push(format!("    source_agent: \"{agent}\""));
    }

    if let Some(ref agent) = info.target_agent {
        lines.push(format!("    target_agent: \"{agent}\""));
    }

    if let Some(ref ctx) = info.context {
        lines.push(format!("    context: \"{ctx}\""));
    }

    if !info.open_questions.is_empty() {
        let questions: Vec<String> = info
            .open_questions
            .iter()
            .map(|q| format!("\"{q}\""))
            .collect();
        lines.push(format!("    open_questions: [{}]", questions.join(", ")));
    }

    if let Some(ref conf) = info.confidence {
        lines.push(format!(
            "    confidence: {{ correctness: {}, optimality: {} }}",
            conf.correctness, conf.optimality
        ));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

/// Convert an AST/HIR [`TransferBlock`] into our richer [`TransferInfo`].
fn transfer_block_to_info(tb: &TransferBlock) -> TransferInfo {
    TransferInfo {
        source_agent: tb.source_agent.clone(),
        target_agent: tb.target_agent.clone(),
        context: tb.context.clone(),
        open_questions: tb.open_questions.clone(),
        confidence: tb.confidence.map(|(c, o)| Confidence {
            correctness: c,
            optimality: o,
        }),
    }
}

/// Convert a [`TransferInfo`] back into an AST [`TransferBlock`].
pub fn info_to_transfer_block(info: &TransferInfo) -> TransferBlock {
    TransferBlock {
        source_agent: info.source_agent.clone(),
        target_agent: info.target_agent.clone(),
        context: info.context.clone(),
        open_questions: info.open_questions.clone(),
        confidence: info
            .confidence
            .as_ref()
            .map(|c| (c.correctness, c.optimality)),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_transfer_info() -> TransferInfo {
        TransferInfo {
            source_agent: Some("optimizer-v1".to_string()),
            target_agent: Some("verifier-v2".to_string()),
            context: Some("Tiling applied to matmul inner loop".to_string()),
            open_questions: vec![
                "Is prefetch distance optimal?".to_string(),
                "Should we try col_major layout?".to_string(),
            ],
            confidence: Some(Confidence {
                correctness: 0.95,
                optimality: 0.7,
            }),
        }
    }

    #[test]
    fn test_transfer_roundtrip() {
        let info = sample_transfer_info();

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&info).expect("serialize should succeed");

        // Deserialize back
        let info2: TransferInfo =
            serde_json::from_str(&json).expect("deserialize should succeed");

        assert_eq!(info, info2);
        assert_eq!(info2.source_agent.as_deref(), Some("optimizer-v1"));
        assert_eq!(info2.target_agent.as_deref(), Some("verifier-v2"));
        assert_eq!(info2.open_questions.len(), 2);
        let conf = info2.confidence.as_ref().expect("should have confidence");
        assert!((conf.correctness - 0.95).abs() < f64::EPSILON);
        assert!((conf.optimality - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_transfer_generation() {
        let info = sample_transfer_info();
        let text = generate_transfer(&info);

        assert!(text.starts_with("@transfer {"));
        assert!(text.ends_with('}'));
        assert!(text.contains("source_agent: \"optimizer-v1\""));
        assert!(text.contains("target_agent: \"verifier-v2\""));
        assert!(text.contains("context: \"Tiling applied to matmul inner loop\""));
        assert!(text.contains("open_questions:"));
        assert!(text.contains("\"Is prefetch distance optimal?\""));
        assert!(text.contains("confidence:"));
        assert!(text.contains("correctness: 0.95"));
        assert!(text.contains("optimality: 0.7"));
    }

    #[test]
    fn test_transfer_generation_minimal() {
        let info = TransferInfo {
            source_agent: None,
            target_agent: None,
            context: None,
            open_questions: vec![],
            confidence: None,
        };
        let text = generate_transfer(&info);
        assert_eq!(text, "@transfer {\n}");
    }

    #[test]
    fn test_transfer_generation_partial() {
        let info = TransferInfo {
            source_agent: Some("agent-a".to_string()),
            target_agent: None,
            context: Some("partial transfer".to_string()),
            open_questions: vec![],
            confidence: None,
        };
        let text = generate_transfer(&info);
        assert!(text.contains("source_agent: \"agent-a\""));
        assert!(!text.contains("target_agent"));
        assert!(text.contains("context: \"partial transfer\""));
        assert!(!text.contains("open_questions"));
        assert!(!text.contains("confidence"));
    }

    #[test]
    fn test_transfer_block_to_info() {
        let tb = TransferBlock {
            source_agent: Some("src".to_string()),
            target_agent: Some("dst".to_string()),
            context: Some("ctx".to_string()),
            open_questions: vec!["q1".to_string()],
            confidence: Some((0.9, 0.8)),
        };

        let info = transfer_block_to_info(&tb);
        assert_eq!(info.source_agent.as_deref(), Some("src"));
        assert_eq!(info.target_agent.as_deref(), Some("dst"));
        assert_eq!(info.context.as_deref(), Some("ctx"));
        assert_eq!(info.open_questions, vec!["q1"]);
        let conf = info.confidence.as_ref().expect("confidence");
        assert!((conf.correctness - 0.9).abs() < f64::EPSILON);
        assert!((conf.optimality - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_info_to_transfer_block() {
        let info = sample_transfer_info();
        let tb = info_to_transfer_block(&info);

        assert_eq!(tb.source_agent, info.source_agent);
        assert_eq!(tb.target_agent, info.target_agent);
        assert_eq!(tb.context, info.context);
        assert_eq!(tb.open_questions, info.open_questions);
        assert_eq!(tb.confidence, Some((0.95, 0.7)));
    }

    #[test]
    fn test_extract_transfer_from_source_with_transfer() {
        let source = r#"
fn main() -> i32 {
    @transfer {
        source_agent: "test-agent"
        target_agent: "next-agent"
        context: "testing transfer extraction"
    }
    return 0;
}
"#;
        let info = extract_transfer(source);
        assert!(
            info.is_some(),
            "should extract transfer info from source with @transfer block"
        );
        let info = info.expect("already checked");
        assert_eq!(info.source_agent.as_deref(), Some("test-agent"));
        assert_eq!(info.target_agent.as_deref(), Some("next-agent"));
        assert_eq!(
            info.context.as_deref(),
            Some("testing transfer extraction")
        );
    }

    #[test]
    fn test_extract_transfer_from_source_without_transfer() {
        let source = r#"
fn main() -> i32 {
    return 0;
}
"#;
        let info = extract_transfer(source);
        assert!(info.is_none());
    }

    #[test]
    fn test_extract_transfer_from_invalid_source() {
        let source = "this is not valid AXIOM at all @@@";
        let info = extract_transfer(source);
        assert!(info.is_none());
    }

    #[test]
    fn test_transfer_json_roundtrip_minimal() {
        let info = TransferInfo {
            source_agent: None,
            target_agent: None,
            context: None,
            open_questions: vec![],
            confidence: None,
        };

        let json = serde_json::to_string(&info).expect("serialize");
        let info2: TransferInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(info, info2);
    }

    #[test]
    fn test_confidence_values() {
        let conf = Confidence {
            correctness: 1.0,
            optimality: 0.0,
        };
        let json = serde_json::to_string(&conf).expect("serialize");
        let conf2: Confidence = serde_json::from_str(&json).expect("deserialize");
        assert!((conf2.correctness - 1.0).abs() < f64::EPSILON);
        assert!((conf2.optimality - 0.0).abs() < f64::EPSILON);
    }
}
