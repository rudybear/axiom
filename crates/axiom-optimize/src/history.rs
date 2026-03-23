//! Optimization history — records of optimization attempts and results.
//!
//! An [`OptHistory`] stores a sequence of [`OptRecord`]s, each representing
//! one optimization iteration. Records capture parameter values, measured
//! metrics, the agent that produced the proposal, and the target architecture.
//!
//! History is serializable to/from JSON via `serde` so it can be persisted
//! alongside the AXIOM source file.
//!
//! # Example
//!
//! ```
//! use axiom_optimize::history::{OptHistory, OptRecord};
//! use std::collections::HashMap;
//!
//! let mut history = OptHistory::new();
//! assert_eq!(history.next_version(), "v1");
//!
//! let mut params = HashMap::new();
//! params.insert("tile_m".to_string(), serde_json::Value::Number(64.into()));
//!
//! let mut metrics = HashMap::new();
//! metrics.insert("time_ms".to_string(), 28.1);
//!
//! history.add_record(OptRecord {
//!     version: "v1".to_string(),
//!     params,
//!     metrics,
//!     agent: Some("axiom-optimizer".to_string()),
//!     target: Some("native".to_string()),
//!     timestamp: "2026-03-22T00:00:00Z".to_string(),
//! });
//!
//! assert_eq!(history.next_version(), "v2");
//! assert_eq!(history.records.len(), 1);
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// One optimization attempt record.
///
/// Each record captures a complete snapshot of a single optimization
/// iteration: the parameter values tried, the resulting metrics, and
/// metadata about the agent and target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptRecord {
    /// Version label: `"v1"`, `"v2"`, etc.
    pub version: String,
    /// Parameter values tried in this iteration (hole name -> value).
    pub params: HashMap<String, serde_json::Value>,
    /// Measured metrics (metric name -> value), e.g. `"time_ms" -> 28.1`.
    pub metrics: HashMap<String, f64>,
    /// The agent that produced this proposal, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The target architecture, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// ISO-8601 timestamp of when this record was created.
    pub timestamp: String,
}

/// Optimization history for a program — an ordered list of [`OptRecord`]s.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptHistory {
    /// All records, in chronological order.
    pub records: Vec<OptRecord>,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when working with optimization history.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    /// JSON serialization or deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl OptHistory {
    /// Create a new, empty history.
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    /// Append a record to the history.
    pub fn add_record(&mut self, record: OptRecord) {
        self.records.push(record);
    }

    /// Find the record with the best (lowest) value for the given metric.
    ///
    /// Returns `None` if the history is empty or no record contains the
    /// requested metric.
    pub fn best_by_metric(&self, metric: &str) -> Option<&OptRecord> {
        self.records
            .iter()
            .filter_map(|r| r.metrics.get(metric).map(|v| (r, *v)))
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(r, _)| r)
    }

    /// Serialize the history to a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`HistoryError::Json`] if serialization fails (should not
    /// happen for well-formed data).
    pub fn to_json(&self) -> Result<String, HistoryError> {
        serde_json::to_string_pretty(self).map_err(HistoryError::from)
    }

    /// Deserialize a history from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`HistoryError::Json`] if the input is not valid JSON or does
    /// not match the expected schema.
    pub fn from_json(json: &str) -> Result<Self, HistoryError> {
        serde_json::from_str(json).map_err(HistoryError::from)
    }

    /// Return the next version label (e.g., `"v1"`, `"v2"`, ...).
    ///
    /// The version number is `records.len() + 1`.
    pub fn next_version(&self) -> String {
        format!("v{}", self.records.len() + 1)
    }
}

impl Default for OptHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Convenience constructors for OptRecord
// ---------------------------------------------------------------------------

impl OptRecord {
    /// Create a new record with the given version and empty params/metrics.
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            params: HashMap::new(),
            metrics: HashMap::new(),
            agent: None,
            target: None,
            timestamp: String::new(),
        }
    }

    /// Set a parameter value.
    pub fn with_param(mut self, name: impl Into<String>, value: serde_json::Value) -> Self {
        self.params.insert(name.into(), value);
        self
    }

    /// Set a metric value.
    pub fn with_metric(mut self, name: impl Into<String>, value: f64) -> Self {
        self.metrics.insert(name.into(), value);
        self
    }

    /// Set the agent.
    pub fn with_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }

    /// Set the target.
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// Set the timestamp.
    pub fn with_timestamp(mut self, ts: impl Into<String>) -> Self {
        self.timestamp = ts.into();
        self
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record(version: &str, time_ms: f64) -> OptRecord {
        let mut params = HashMap::new();
        params.insert(
            "tile_m".to_string(),
            serde_json::Value::Number(serde_json::Number::from(64)),
        );

        let mut metrics = HashMap::new();
        metrics.insert("time_ms".to_string(), time_ms);

        OptRecord {
            version: version.to_string(),
            params,
            metrics,
            agent: Some("test-agent".to_string()),
            target: Some("native".to_string()),
            timestamp: "2026-03-22T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn new_history_is_empty() {
        let h = OptHistory::new();
        assert!(h.records.is_empty());
    }

    #[test]
    fn default_history_is_empty() {
        let h = OptHistory::default();
        assert!(h.records.is_empty());
    }

    #[test]
    fn add_record_increases_count() {
        let mut h = OptHistory::new();
        h.add_record(sample_record("v1", 28.1));
        assert_eq!(h.records.len(), 1);
        h.add_record(sample_record("v2", 25.0));
        assert_eq!(h.records.len(), 2);
    }

    #[test]
    fn next_version_increments() {
        let mut h = OptHistory::new();
        assert_eq!(h.next_version(), "v1");
        h.add_record(sample_record("v1", 28.1));
        assert_eq!(h.next_version(), "v2");
        h.add_record(sample_record("v2", 25.0));
        assert_eq!(h.next_version(), "v3");
    }

    #[test]
    fn best_by_metric_finds_minimum() {
        let mut h = OptHistory::new();
        h.add_record(sample_record("v1", 28.1));
        h.add_record(sample_record("v2", 15.0));
        h.add_record(sample_record("v3", 22.5));

        let best = h.best_by_metric("time_ms").expect("should find best");
        assert_eq!(best.version, "v2");
    }

    #[test]
    fn best_by_metric_returns_none_for_empty() {
        let h = OptHistory::new();
        assert!(h.best_by_metric("time_ms").is_none());
    }

    #[test]
    fn best_by_metric_returns_none_for_unknown_metric() {
        let mut h = OptHistory::new();
        h.add_record(sample_record("v1", 28.1));
        assert!(h.best_by_metric("flops").is_none());
    }

    #[test]
    fn best_by_metric_single_record() {
        let mut h = OptHistory::new();
        h.add_record(sample_record("v1", 42.0));
        let best = h.best_by_metric("time_ms").expect("should find best");
        assert_eq!(best.version, "v1");
    }

    #[test]
    fn json_roundtrip() {
        let mut h = OptHistory::new();
        h.add_record(sample_record("v1", 28.1));
        h.add_record(sample_record("v2", 15.0));

        let json = h.to_json().expect("serialization should succeed");
        let h2 = OptHistory::from_json(&json).expect("deserialization should succeed");

        assert_eq!(h, h2);
    }

    #[test]
    fn json_roundtrip_empty_history() {
        let h = OptHistory::new();
        let json = h.to_json().expect("serialization should succeed");
        let h2 = OptHistory::from_json(&json).expect("deserialization should succeed");
        assert_eq!(h, h2);
    }

    #[test]
    fn from_json_rejects_invalid() {
        let result = OptHistory::from_json("not valid json {{{");
        assert!(result.is_err());
    }

    #[test]
    fn from_json_rejects_wrong_schema() {
        let result = OptHistory::from_json(r#"{"foo": "bar"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn json_contains_expected_fields() {
        let mut h = OptHistory::new();
        h.add_record(sample_record("v1", 28.1));

        let json = h.to_json().expect("serialization should succeed");
        assert!(json.contains("\"version\""));
        assert!(json.contains("\"v1\""));
        assert!(json.contains("\"params\""));
        assert!(json.contains("\"metrics\""));
        assert!(json.contains("\"time_ms\""));
        assert!(json.contains("\"agent\""));
        assert!(json.contains("\"target\""));
        assert!(json.contains("\"timestamp\""));
    }

    #[test]
    fn record_without_optional_fields_serializes() {
        let mut h = OptHistory::new();
        h.add_record(OptRecord {
            version: "v1".to_string(),
            params: HashMap::new(),
            metrics: HashMap::new(),
            agent: None,
            target: None,
            timestamp: "2026-03-22T00:00:00Z".to_string(),
        });

        let json = h.to_json().expect("serialization should succeed");
        // agent and target should be skipped when None
        assert!(!json.contains("\"agent\""));
        assert!(!json.contains("\"target\""));

        // Still roundtrips correctly
        let h2 = OptHistory::from_json(&json).expect("deserialization should succeed");
        assert_eq!(h, h2);
    }

    #[test]
    fn record_builder_pattern() {
        let record = OptRecord::new("v1")
            .with_param("tile_m", serde_json::Value::Number(64.into()))
            .with_metric("time_ms", 28.1)
            .with_agent("test-agent")
            .with_target("x86_64")
            .with_timestamp("2026-03-22T00:00:00Z");

        assert_eq!(record.version, "v1");
        assert_eq!(
            record.params.get("tile_m"),
            Some(&serde_json::Value::Number(64.into()))
        );
        assert_eq!(record.metrics.get("time_ms"), Some(&28.1));
        assert_eq!(record.agent.as_deref(), Some("test-agent"));
        assert_eq!(record.target.as_deref(), Some("x86_64"));
    }

    #[test]
    fn best_by_metric_with_nan_values() {
        let mut h = OptHistory::new();
        // Add a record with NaN metric — should not crash
        let mut r = sample_record("v1", f64::NAN);
        r.metrics.insert("time_ms".to_string(), f64::NAN);
        h.add_record(r);
        h.add_record(sample_record("v2", 10.0));

        // Should still find the non-NaN record as best
        let best = h.best_by_metric("time_ms");
        assert!(best.is_some());
    }

    #[test]
    fn multiple_metrics_per_record() {
        let mut metrics = HashMap::new();
        metrics.insert("time_ms".to_string(), 28.1);
        metrics.insert("flops".to_string(), 1_000_000.0);
        metrics.insert("memory_mb".to_string(), 256.0);

        let record = OptRecord {
            version: "v1".to_string(),
            params: HashMap::new(),
            metrics,
            agent: None,
            target: None,
            timestamp: String::new(),
        };

        let mut h = OptHistory::new();
        h.add_record(record);

        let json = h.to_json().expect("serialization should succeed");
        let h2 = OptHistory::from_json(&json).expect("deserialization should succeed");
        assert_eq!(h2.records[0].metrics.len(), 3);
    }
}
