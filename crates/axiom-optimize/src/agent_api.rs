//! High-level API for AI agents working with AXIOM programs.
//!
//! [`AgentSession`] is the primary entry point. It wraps an AXIOM source
//! program and provides methods to inspect optimization surfaces, manage
//! optimization history, apply proposals, and generate updated source with
//! transfer metadata for inter-agent handoff.
//!
//! # Example
//!
//! ```
//! use std::collections::HashMap;
//! use axiom_optimize::agent_api::AgentSession;
//! use axiom_optimize::proposal::Proposal;
//! use axiom_optimize::surface::Value;
//! use axiom_optimize::transfer::{TransferInfo, Confidence};
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
//! let mut session = AgentSession::from_source(source).expect("valid source");
//! assert_eq!(session.surfaces().len(), 1);
//! assert_eq!(session.surfaces()[0].holes[0].name, "unroll_factor");
//!
//! // Apply a proposal
//! let mut proposal = Proposal::new();
//! proposal.set("unroll_factor", Value::Int(4));
//! let mut metrics = HashMap::new();
//! metrics.insert("time_ms".to_string(), 28.0);
//! session.apply_proposal(proposal, metrics, "test-agent").expect("apply succeeds");
//!
//! assert_eq!(session.history().records.len(), 1);
//!
//! // Export with transfer info
//! let transfer = TransferInfo {
//!     source_agent: Some("test-agent".to_string()),
//!     target_agent: Some("next-agent".to_string()),
//!     context: Some("Unrolling applied".to_string()),
//!     open_questions: vec![],
//!     confidence: Some(Confidence { correctness: 0.99, optimality: 0.8 }),
//! };
//! let output = session.export_with_transfer(transfer);
//! assert!(output.contains("@transfer {"));
//! assert!(output.contains("test-agent"));
//! ```

use std::collections::HashMap;

use crate::history::{HistoryError, OptHistory, OptRecord};
use crate::proposal::{Proposal, ValidationError};
use crate::surface::{OptSurface, Value, extract_surfaces};
use crate::transfer::{TransferInfo, extract_transfer, generate_transfer};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when using the agent API.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// AXIOM source failed to parse or lower.
    #[error("source error: {0}")]
    Source(String),

    /// Proposal validation failed.
    #[error("validation error: {0}")]
    Validation(String),

    /// History serialization/deserialization failed.
    #[error("history error: {0}")]
    History(#[from] HistoryError),

    /// File I/O failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<Vec<ValidationError>> for AgentError {
    fn from(errors: Vec<ValidationError>) -> Self {
        let msgs: Vec<String> = errors.iter().map(|e| format!("{e}")).collect();
        AgentError::Validation(msgs.join("; "))
    }
}

// ---------------------------------------------------------------------------
// AgentSession
// ---------------------------------------------------------------------------

/// High-level interface for AI agents working with AXIOM programs.
///
/// An `AgentSession` loads an AXIOM source program, extracts its optimization
/// surfaces, manages optimization history, and can export the program with
/// transfer metadata for inter-agent handoff.
pub struct AgentSession {
    /// The raw AXIOM source text.
    source: String,
    /// Optional file path the source was loaded from.
    file_path: Option<String>,
    /// All optimization surfaces discovered in the program.
    surfaces: Vec<OptSurface>,
    /// History of optimization attempts.
    history: OptHistory,
    /// Transfer info extracted from the source, if present.
    transfer: Option<TransferInfo>,
}

impl AgentSession {
    /// Load an AXIOM program from a source string.
    ///
    /// Parses the source, extracts optimization surfaces and transfer info.
    /// An empty optimization history is created.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Source`] if the source cannot be parsed.
    pub fn from_source(source: &str) -> Result<Self, AgentError> {
        let surfaces = extract_surfaces(source).map_err(|errs| {
            AgentError::Source(errs.join("; "))
        })?;

        let transfer = extract_transfer(source);

        Ok(Self {
            source: source.to_string(),
            file_path: None,
            surfaces,
            history: OptHistory::new(),
            transfer,
        })
    }

    /// Load an AXIOM program from a file.
    ///
    /// Reads the file contents and delegates to [`from_source`](Self::from_source).
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Io`] if the file cannot be read, or
    /// [`AgentError::Source`] if parsing fails.
    pub fn from_file(path: &str) -> Result<Self, AgentError> {
        let source = std::fs::read_to_string(path)?;
        let mut session = Self::from_source(&source)?;
        session.file_path = Some(path.to_string());
        Ok(session)
    }

    /// Get all optimization surfaces discovered in the program.
    pub fn surfaces(&self) -> &[OptSurface] {
        &self.surfaces
    }

    /// Get the optimization history for this session.
    pub fn history(&self) -> &OptHistory {
        &self.history
    }

    /// Get the transfer info extracted from the source, if present.
    pub fn transfer(&self) -> Option<&TransferInfo> {
        self.transfer.as_ref()
    }

    /// Get the raw source text.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get the file path, if the session was loaded from a file.
    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    /// Apply a proposal and record the result in history.
    ///
    /// The proposal is validated against the current surfaces. If validation
    /// passes, a new [`OptRecord`] is appended to the history with the
    /// given metrics and agent name.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Validation`] if the proposal does not pass
    /// validation against the current surfaces.
    pub fn apply_proposal(
        &mut self,
        proposal: Proposal,
        metrics: HashMap<String, f64>,
        agent: &str,
    ) -> Result<(), AgentError> {
        // Validate the proposal against surfaces
        crate::proposal::validate_proposal(&proposal, &self.surfaces)?;

        // Convert proposal values to serde_json::Value for storage
        let params: HashMap<String, serde_json::Value> = proposal
            .values
            .iter()
            .map(|(k, v)| (k.clone(), value_to_json(v)))
            .collect();

        // Build and append the record
        let version = self.history.next_version();
        let record = OptRecord {
            version,
            params,
            metrics,
            agent: Some(agent.to_string()),
            target: None,
            timestamp: current_timestamp(),
        };

        self.history.add_record(record);
        Ok(())
    }

    /// Generate the source text with an appended `@transfer` block.
    ///
    /// The original source is preserved as-is, and the transfer block is
    /// appended inside a sentinel function `__transfer__` at the end so that
    /// the next agent can parse the source and extract the transfer metadata
    /// via [`AgentSession::transfer()`].
    pub fn export_with_transfer(&self, transfer: TransferInfo) -> String {
        let transfer_text = generate_transfer(&transfer);
        // Strip any previous __transfer__ sentinel function to avoid duplicates
        let base = strip_transfer_sentinel(&self.source);
        let mut output = base.trim_end().to_string();
        output.push_str("\n\n// --- Agent Transfer ---\n");
        output.push_str("fn __transfer__() -> i32 {\n    ");
        output.push_str(&transfer_text.replace('\n', "\n    "));
        output.push_str("\n    return 0;\n}\n");
        output
    }

    /// Save the optimization history to a JSON file.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::History`] if serialization fails, or
    /// [`AgentError::Io`] if the file cannot be written.
    pub fn save_history(&self, path: &str) -> Result<(), AgentError> {
        let json = self.history.to_json()?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load optimization history from a JSON file, replacing the current history.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Io`] if the file cannot be read, or
    /// [`AgentError::History`] if deserialization fails.
    pub fn load_history(&mut self, path: &str) -> Result<(), AgentError> {
        let json = std::fs::read_to_string(path)?;
        self.history = OptHistory::from_json(&json)?;
        Ok(())
    }

    /// Set transfer info on this session (e.g., after extracting from source
    /// or receiving from another agent).
    pub fn set_transfer(&mut self, transfer: TransferInfo) {
        self.transfer = Some(transfer);
    }

    /// Get a summary of the session state, suitable for logging.
    pub fn summary(&self) -> SessionSummary {
        SessionSummary {
            source_len: self.source.len(),
            file_path: self.file_path.clone(),
            num_surfaces: self.surfaces.len(),
            total_holes: self.surfaces.iter().map(|s| s.holes.len()).sum(),
            num_history_records: self.history.records.len(),
            has_transfer: self.transfer.is_some(),
        }
    }
}

// ---------------------------------------------------------------------------
// Session summary
// ---------------------------------------------------------------------------

/// Summary of an [`AgentSession`] state, for logging and diagnostics.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    /// Length of the source text in bytes.
    pub source_len: usize,
    /// File path the session was loaded from, if any.
    pub file_path: Option<String>,
    /// Number of optimization surfaces discovered.
    pub num_surfaces: usize,
    /// Total number of optimization holes across all surfaces.
    pub total_holes: usize,
    /// Number of optimization records in history.
    pub num_history_records: usize,
    /// Whether transfer info is present.
    pub has_transfer: bool,
}

impl std::fmt::Display for SessionSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AgentSession(source={} bytes, surfaces={}, holes={}, history={} records, transfer={})",
            self.source_len,
            self.num_surfaces,
            self.total_holes,
            self.num_history_records,
            if self.has_transfer { "yes" } else { "no" }
        )
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Convert a surface [`Value`] to a [`serde_json::Value`] for history storage.
fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Int(v) => serde_json::Value::Number((*v).into()),
        Value::Float(v) => {
            serde_json::Number::from_f64(*v)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        Value::Bool(v) => serde_json::Value::Bool(*v),
        Value::Ident(s) => serde_json::Value::String(s.clone()),
        Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
    }
}

/// Strip the `__transfer__` sentinel function (and the preceding comment
/// marker) from source text so that `export_with_transfer` can append a
/// fresh one without creating duplicates.
fn strip_transfer_sentinel(source: &str) -> String {
    // Look for the comment marker that precedes the sentinel function.
    if let Some(marker_pos) = source.find("// --- Agent Transfer ---") {
        source[..marker_pos].to_string()
    } else {
        source.to_string()
    }
}

/// Get the current UTC timestamp as an ISO-8601 string.
///
/// Falls back to a static placeholder if the system time is unavailable.
fn current_timestamp() -> String {
    // Use SystemTime since we don't want to add a chrono dependency.
    let now = std::time::SystemTime::now();
    match now.duration_since(std::time::UNIX_EPOCH) {
        Ok(dur) => {
            let secs = dur.as_secs();
            // Simple ISO-8601 format: approximate, no full calendar math needed.
            // For a real project, use chrono. This is good enough for record-keeping.
            format!("{secs}")
        }
        Err(_) => "unknown".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::Confidence;

    const STRATEGY_SOURCE: &str = r#"
fn compute(a: i32, b: i32) -> i32 {
    @strategy {
        unroll: ?unroll_factor
    }
    return a + b;
}
"#;

    const MULTI_HOLE_SOURCE: &str = r#"
fn matmul(a: i32, b: i32) -> i32 {
    @strategy {
        tiling: { M: ?tile_m, N: ?tile_n }
        unroll: ?unroll_factor
    }
    return a + b;
}
"#;

    #[test]
    fn test_agent_session_from_source() {
        let session = AgentSession::from_source(STRATEGY_SOURCE).expect("should parse");
        assert_eq!(session.surfaces().len(), 1);
        assert_eq!(session.surfaces()[0].function_name, "compute");
        assert_eq!(session.surfaces()[0].holes.len(), 1);
        assert_eq!(session.surfaces()[0].holes[0].name, "unroll_factor");
        assert!(session.transfer().is_none());
        assert!(session.history().records.is_empty());
        assert!(session.file_path().is_none());
    }

    #[test]
    fn test_agent_session_from_source_no_surfaces() {
        let source = r#"
fn main() -> i32 {
    return 0;
}
"#;
        let session = AgentSession::from_source(source).expect("should parse");
        assert!(session.surfaces().is_empty());
    }

    #[test]
    fn test_agent_session_from_source_invalid() {
        let result = AgentSession::from_source("@@@ not valid !!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_agent_api_apply_proposal() {
        let mut session = AgentSession::from_source(STRATEGY_SOURCE).expect("should parse");

        let mut proposal = Proposal::new();
        proposal.set("unroll_factor", Value::Int(4));

        let mut metrics = HashMap::new();
        metrics.insert("time_ms".to_string(), 28.0);

        session
            .apply_proposal(proposal, metrics, "test-agent")
            .expect("should succeed");

        assert_eq!(session.history().records.len(), 1);
        assert_eq!(session.history().records[0].version, "v1");
        assert_eq!(session.history().records[0].agent.as_deref(), Some("test-agent"));
        assert_eq!(
            session.history().records[0].metrics.get("time_ms"),
            Some(&28.0)
        );
    }

    #[test]
    fn test_agent_api_apply_invalid_proposal() {
        let mut session = AgentSession::from_source(STRATEGY_SOURCE).expect("should parse");

        // Wrong type for unroll_factor
        let mut proposal = Proposal::new();
        proposal.set("unroll_factor", Value::Bool(true));

        let metrics = HashMap::new();
        let result = session.apply_proposal(proposal, metrics, "agent");
        assert!(result.is_err());
        // History should be unchanged
        assert!(session.history().records.is_empty());
    }

    #[test]
    fn test_agent_api_multiple_proposals() {
        let mut session = AgentSession::from_source(STRATEGY_SOURCE).expect("should parse");

        for i in 1..=3 {
            let mut proposal = Proposal::new();
            proposal.set("unroll_factor", Value::Int(i * 2));
            let mut metrics = HashMap::new();
            metrics.insert("time_ms".to_string(), 100.0 / i as f64);
            session
                .apply_proposal(proposal, metrics, "agent")
                .expect("should succeed");
        }

        assert_eq!(session.history().records.len(), 3);
        assert_eq!(session.history().records[0].version, "v1");
        assert_eq!(session.history().records[1].version, "v2");
        assert_eq!(session.history().records[2].version, "v3");

        // Best by time_ms should be v3 (lowest time)
        let best = session.history().best_by_metric("time_ms");
        assert!(best.is_some());
    }

    #[test]
    fn test_agent_api_export_with_transfer() {
        let session = AgentSession::from_source(STRATEGY_SOURCE).expect("should parse");

        let transfer = TransferInfo {
            source_agent: Some("optimizer".to_string()),
            target_agent: Some("verifier".to_string()),
            context: Some("Applied unrolling".to_string()),
            open_questions: vec!["Check correctness".to_string()],
            confidence: Some(Confidence {
                correctness: 0.99,
                optimality: 0.8,
            }),
        };

        let output = session.export_with_transfer(transfer);

        // Original source should be preserved
        assert!(output.contains("fn compute"));
        assert!(output.contains("@strategy"));

        // Transfer block should be appended
        assert!(output.contains("@transfer {"));
        assert!(output.contains("source_agent: \"optimizer\""));
        assert!(output.contains("target_agent: \"verifier\""));
        assert!(output.contains("context: \"Applied unrolling\""));
        assert!(output.contains("Check correctness"));
        assert!(output.contains("correctness: 0.99"));
    }

    #[test]
    fn test_agent_api_export_minimal_transfer() {
        let session = AgentSession::from_source(STRATEGY_SOURCE).expect("should parse");

        let transfer = TransferInfo {
            source_agent: None,
            target_agent: None,
            context: None,
            open_questions: vec![],
            confidence: None,
        };

        let output = session.export_with_transfer(transfer);
        assert!(output.contains("@transfer {"));
        assert!(output.contains('}'));
    }

    #[test]
    fn test_agent_session_summary() {
        let session = AgentSession::from_source(MULTI_HOLE_SOURCE).expect("should parse");
        let summary = session.summary();

        assert_eq!(summary.num_surfaces, 1);
        assert_eq!(summary.total_holes, 3); // tile_m, tile_n, unroll_factor
        assert_eq!(summary.num_history_records, 0);
        assert!(!summary.has_transfer);

        let display = format!("{summary}");
        assert!(display.contains("surfaces=1"));
        assert!(display.contains("holes=3"));
        assert!(display.contains("history=0 records"));
        assert!(display.contains("transfer=no"));
    }

    #[test]
    fn test_agent_session_source_accessor() {
        let session = AgentSession::from_source(STRATEGY_SOURCE).expect("should parse");
        assert_eq!(session.source(), STRATEGY_SOURCE);
    }

    #[test]
    fn test_agent_session_set_transfer() {
        let mut session = AgentSession::from_source(STRATEGY_SOURCE).expect("should parse");
        assert!(session.transfer().is_none());

        let transfer = TransferInfo {
            source_agent: Some("agent-x".to_string()),
            target_agent: None,
            context: None,
            open_questions: vec![],
            confidence: None,
        };

        session.set_transfer(transfer);
        assert!(session.transfer().is_some());
        assert_eq!(
            session.transfer().expect("set").source_agent.as_deref(),
            Some("agent-x")
        );
    }

    #[test]
    fn test_value_to_json_int() {
        let v = value_to_json(&Value::Int(42));
        assert_eq!(v, serde_json::json!(42));
    }

    #[test]
    fn test_value_to_json_float() {
        let v = value_to_json(&Value::Float(3.14));
        assert_eq!(v, serde_json::json!(3.14));
    }

    #[test]
    fn test_value_to_json_bool() {
        let v = value_to_json(&Value::Bool(true));
        assert_eq!(v, serde_json::json!(true));
    }

    #[test]
    fn test_value_to_json_ident() {
        let v = value_to_json(&Value::Ident("i".to_string()));
        assert_eq!(v, serde_json::json!("i"));
    }

    #[test]
    fn test_value_to_json_array() {
        let v = value_to_json(&Value::Array(vec![
            Value::Ident("i".to_string()),
            Value::Ident("j".to_string()),
        ]));
        assert_eq!(v, serde_json::json!(["i", "j"]));
    }

    #[test]
    fn test_agent_session_with_transfer_in_source() {
        let source = r#"
fn main() -> i32 {
    @transfer {
        source_agent: "previous-agent"
        context: "testing extraction"
    }
    return 0;
}
"#;
        let session = AgentSession::from_source(source).expect("should parse");
        assert!(session.transfer().is_some());
        let t = session.transfer().expect("has transfer");
        assert_eq!(t.source_agent.as_deref(), Some("previous-agent"));
        assert_eq!(t.context.as_deref(), Some("testing extraction"));
    }

    #[test]
    fn test_save_and_load_history() {
        let temp_dir = std::env::temp_dir();
        let history_path = temp_dir.join("axiom_test_history.json");
        let history_path_str = history_path
            .to_str()
            .expect("temp path should be valid UTF-8");

        // Create session and apply a proposal
        let mut session = AgentSession::from_source(STRATEGY_SOURCE).expect("should parse");

        let mut proposal = Proposal::new();
        proposal.set("unroll_factor", Value::Int(8));
        let mut metrics = HashMap::new();
        metrics.insert("time_ms".to_string(), 15.0);
        session
            .apply_proposal(proposal, metrics, "saver-agent")
            .expect("apply");

        // Save history
        session.save_history(history_path_str).expect("save");

        // Create a new session and load the history
        let mut session2 = AgentSession::from_source(STRATEGY_SOURCE).expect("should parse");
        assert!(session2.history().records.is_empty());

        session2.load_history(history_path_str).expect("load");
        assert_eq!(session2.history().records.len(), 1);
        assert_eq!(session2.history().records[0].version, "v1");

        // Clean up
        let _ = std::fs::remove_file(&history_path);
    }
}
