//! Optimization proposal types and validation.
//!
//! A *proposal* is a set of concrete values that an AI agent suggests for the
//! optimization holes in one or more [`OptSurface`]s. Before applying a
//! proposal the compiler validates that every value matches the declared type
//! and falls within the allowed range.
//!
//! # Usage
//!
//! ```
//! use std::collections::HashMap;
//! use axiom_optimize::proposal::{Proposal, validate_proposal};
//! use axiom_optimize::surface::{extract_surfaces, Value};
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
//! let surfaces = extract_surfaces(source).expect("parses");
//! let mut values = HashMap::new();
//! values.insert("unroll_factor".to_string(), Value::Int(4));
//! let proposal = Proposal { values };
//!
//! let result = validate_proposal(&proposal, &surfaces);
//! assert!(result.is_ok());
//! ```

use std::collections::HashMap;
use crate::surface::{HoleType, OptSurface, Value};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A proposed set of concrete values for optimization holes.
#[derive(Debug, Clone)]
pub struct Proposal {
    /// Maps hole name → proposed value.
    pub values: HashMap<String, Value>,
}

impl Proposal {
    /// Create a new empty proposal.
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    /// Insert a value for a hole.
    pub fn set(&mut self, name: impl Into<String>, value: Value) -> &mut Self {
        self.values.insert(name.into(), value);
        self
    }
}

impl Default for Proposal {
    fn default() -> Self {
        Self::new()
    }
}

/// An error encountered while validating a proposal against optimization surfaces.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValidationError {
    /// The proposed value has the wrong type for the target hole.
    #[error("type mismatch for hole `{hole}`: expected {expected}, got {actual}")]
    TypeMismatch {
        /// Hole name.
        hole: String,
        /// Expected type.
        expected: HoleType,
        /// Description of the actual value.
        actual: String,
    },

    /// The proposed value is outside the allowed range.
    #[error("value {value} for hole `{hole}` is outside range [{lo}, {hi}]")]
    OutOfRange {
        /// Hole name.
        hole: String,
        /// Proposed value.
        value: i64,
        /// Lower bound (inclusive).
        lo: i64,
        /// Upper bound (inclusive).
        hi: i64,
    },

    /// A hole required by the surfaces has no proposed value.
    #[error("missing value for required hole `{hole}`")]
    MissingHole {
        /// Hole name.
        hole: String,
    },

    /// The proposal contains a value for a hole that does not exist in any surface.
    #[error("unknown hole `{hole}` in proposal")]
    UnknownHole {
        /// Hole name.
        hole: String,
    },
}


// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate a proposal against a set of optimization surfaces.
///
/// Checks:
/// 1. Every hole in every surface has a proposed value (completeness).
/// 2. Every proposed value has the correct type.
/// 3. Every proposed value is within the declared range (if any).
/// 4. The proposal does not contain values for holes that do not exist.
///
/// # Errors
///
/// Returns `Err(Vec<ValidationError>)` with all validation failures.
pub fn validate_proposal(
    proposal: &Proposal,
    surfaces: &[OptSurface],
) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    // Build a map of all known holes across all surfaces
    let mut known_holes: HashMap<String, &crate::surface::OptHole> = HashMap::new();
    for surface in surfaces {
        for hole in &surface.holes {
            known_holes.insert(hole.name.clone(), hole);
        }
    }

    // Check for unknown holes in the proposal
    for name in proposal.values.keys() {
        if !known_holes.contains_key(name) {
            errors.push(ValidationError::UnknownHole {
                hole: name.clone(),
            });
        }
    }

    // Check for missing and invalid values
    for (name, hole) in &known_holes {
        match proposal.values.get(name.as_str()) {
            None => {
                errors.push(ValidationError::MissingHole {
                    hole: name.clone(),
                });
            }
            Some(value) => {
                // Type check
                if let Err(msg) = check_type(value, &hole.hole_type) {
                    errors.push(ValidationError::TypeMismatch {
                        hole: name.clone(),
                        expected: hole.hole_type.clone(),
                        actual: msg,
                    });
                }

                // Range check
                if let Some((lo, hi)) = hole.range {
                    if let Value::Int(v) = value {
                        if *v < lo || *v > hi {
                            errors.push(ValidationError::OutOfRange {
                                hole: name.clone(),
                                value: *v,
                                lo,
                                hi,
                            });
                        }
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Check that a value is compatible with the expected hole type.
///
/// Returns `Ok(())` if compatible, or `Err(description)` if not.
fn check_type(value: &Value, expected: &HoleType) -> Result<(), String> {
    match (value, expected) {
        (Value::Int(_), HoleType::U32 | HoleType::I32) => Ok(()),
        (Value::Float(_), HoleType::F64) => Ok(()),
        (Value::Bool(_), HoleType::Bool) => Ok(()),
        (Value::Ident(_), HoleType::Ident) => Ok(()),
        (Value::Array(items), HoleType::Array(elem_type)) => {
            for (i, item) in items.iter().enumerate() {
                check_type(item, elem_type)
                    .map_err(|msg| format!("element [{i}]: {msg}"))?;
            }
            Ok(())
        }
        _ => Err(describe_value(value)),
    }
}

/// Produce a human-readable description of a value's type.
fn describe_value(value: &Value) -> String {
    match value {
        Value::Int(v) => format!("int({v})"),
        Value::Float(v) => format!("float({v})"),
        Value::Bool(v) => format!("bool({v})"),
        Value::Ident(s) => format!("ident(\"{s}\")"),
        Value::Array(items) => format!("array(len={})", items.len()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::{extract_surfaces, OptHole, OptSurface, Value};

    #[test]
    fn validate_complete_valid_proposal() {
        let source = r#"
fn compute(a: i32, b: i32) -> i32 {
    @strategy {
        unroll: ?unroll_factor
    }
    return a + b;
}
"#;
        let surfaces = extract_surfaces(source).expect("should parse");
        let mut proposal = Proposal::new();
        proposal.set("unroll_factor", Value::Int(4));

        let result = validate_proposal(&proposal, &surfaces);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_missing_hole() {
        let surfaces = vec![OptSurface {
            function_name: "f".to_string(),
            holes: vec![OptHole {
                name: "x".to_string(),
                hole_type: HoleType::U32,
                range: None,
                current_value: None,
            }],
            strategy: None,
        }];

        let proposal = Proposal::new();
        let result = validate_proposal(&proposal, &surfaces);
        assert!(result.is_err());
        let errs = result.err().expect("should be errors");
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::MissingHole { hole } if hole == "x")));
    }

    #[test]
    fn validate_unknown_hole() {
        let surfaces = vec![OptSurface {
            function_name: "f".to_string(),
            holes: vec![OptHole {
                name: "x".to_string(),
                hole_type: HoleType::U32,
                range: None,
                current_value: None,
            }],
            strategy: None,
        }];

        let mut proposal = Proposal::new();
        proposal.set("x", Value::Int(1));
        proposal.set("y", Value::Int(2)); // unknown

        let result = validate_proposal(&proposal, &surfaces);
        assert!(result.is_err());
        let errs = result.err().expect("should be errors");
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::UnknownHole { hole } if hole == "y")));
    }

    #[test]
    fn validate_type_mismatch() {
        let surfaces = vec![OptSurface {
            function_name: "f".to_string(),
            holes: vec![OptHole {
                name: "x".to_string(),
                hole_type: HoleType::Bool,
                range: None,
                current_value: None,
            }],
            strategy: None,
        }];

        let mut proposal = Proposal::new();
        proposal.set("x", Value::Int(42)); // wrong type

        let result = validate_proposal(&proposal, &surfaces);
        assert!(result.is_err());
        let errs = result.err().expect("should be errors");
        assert!(errs
            .iter()
            .any(|e| matches!(e, ValidationError::TypeMismatch { .. })));
    }

    #[test]
    fn validate_out_of_range() {
        let surfaces = vec![OptSurface {
            function_name: "f".to_string(),
            holes: vec![OptHole {
                name: "x".to_string(),
                hole_type: HoleType::U32,
                range: Some((1, 32)),
                current_value: None,
            }],
            strategy: None,
        }];

        let mut proposal = Proposal::new();
        proposal.set("x", Value::Int(100)); // out of range

        let result = validate_proposal(&proposal, &surfaces);
        assert!(result.is_err());
        let errs = result.err().expect("should be errors");
        assert!(errs.iter().any(
            |e| matches!(e, ValidationError::OutOfRange { hole, value, lo, hi } if hole == "x" && *value == 100 && *lo == 1 && *hi == 32)
        ));
    }

    #[test]
    fn validate_in_range_boundary() {
        let surfaces = vec![OptSurface {
            function_name: "f".to_string(),
            holes: vec![OptHole {
                name: "x".to_string(),
                hole_type: HoleType::U32,
                range: Some((1, 32)),
                current_value: None,
            }],
            strategy: None,
        }];

        // Test lower boundary
        let mut proposal = Proposal::new();
        proposal.set("x", Value::Int(1));
        assert!(validate_proposal(&proposal, &surfaces).is_ok());

        // Test upper boundary
        let mut proposal = Proposal::new();
        proposal.set("x", Value::Int(32));
        assert!(validate_proposal(&proposal, &surfaces).is_ok());
    }

    #[test]
    fn validate_array_type() {
        let surfaces = vec![OptSurface {
            function_name: "f".to_string(),
            holes: vec![OptHole {
                name: "order".to_string(),
                hole_type: HoleType::Array(Box::new(HoleType::Ident)),
                range: None,
                current_value: None,
            }],
            strategy: None,
        }];

        let mut proposal = Proposal::new();
        proposal.set(
            "order",
            Value::Array(vec![
                Value::Ident("i".into()),
                Value::Ident("j".into()),
                Value::Ident("k".into()),
            ]),
        );

        assert!(validate_proposal(&proposal, &surfaces).is_ok());
    }

    #[test]
    fn validate_array_element_type_mismatch() {
        let surfaces = vec![OptSurface {
            function_name: "f".to_string(),
            holes: vec![OptHole {
                name: "order".to_string(),
                hole_type: HoleType::Array(Box::new(HoleType::Ident)),
                range: None,
                current_value: None,
            }],
            strategy: None,
        }];

        let mut proposal = Proposal::new();
        proposal.set(
            "order",
            Value::Array(vec![Value::Int(1), Value::Int(2)]),
        );

        let result = validate_proposal(&proposal, &surfaces);
        assert!(result.is_err());
    }

    #[test]
    fn validate_end_to_end_matmul_style() {
        let source = r#"
fn matmul(a: i32, b: i32) -> i32 {
    @strategy {
        tiling:   { M: ?tile_m, N: ?tile_n, K: ?tile_k }
        order:    ?loop_order
        parallel: ?parallel_dims
        unroll:   ?unroll_factor
        prefetch: ?prefetch_distance
    }
    return a + b;
}
"#;
        let surfaces = extract_surfaces(source).expect("should parse");
        assert_eq!(surfaces.len(), 1);
        assert_eq!(surfaces[0].holes.len(), 7);

        let mut proposal = Proposal::new();
        proposal.set("tile_m", Value::Int(64));
        proposal.set("tile_n", Value::Int(64));
        proposal.set("tile_k", Value::Int(32));
        proposal.set(
            "loop_order",
            Value::Array(vec![
                Value::Ident("i".into()),
                Value::Ident("j".into()),
                Value::Ident("k".into()),
            ]),
        );
        proposal.set(
            "parallel_dims",
            Value::Array(vec![Value::Ident("i".into())]),
        );
        proposal.set("unroll_factor", Value::Int(4));
        proposal.set("prefetch_distance", Value::Int(8));

        let result = validate_proposal(&proposal, &surfaces);
        assert!(result.is_ok(), "errors: {:?}", result.err());
    }

    #[test]
    fn validate_multiple_errors_collected() {
        let surfaces = vec![OptSurface {
            function_name: "f".to_string(),
            holes: vec![
                OptHole {
                    name: "a".to_string(),
                    hole_type: HoleType::U32,
                    range: Some((1, 10)),
                    current_value: None,
                },
                OptHole {
                    name: "b".to_string(),
                    hole_type: HoleType::Bool,
                    range: None,
                    current_value: None,
                },
            ],
            strategy: None,
        }];

        let mut proposal = Proposal::new();
        proposal.set("a", Value::Int(100));   // out of range
        proposal.set("b", Value::Int(42));    // type mismatch
        proposal.set("c", Value::Int(0));     // unknown hole

        let result = validate_proposal(&proposal, &surfaces);
        assert!(result.is_err());
        let errs = result.err().expect("should be errors");
        // Should collect all three error types
        assert!(errs.len() >= 3, "expected at least 3 errors, got {}", errs.len());
    }

    #[test]
    fn proposal_builder() {
        let mut p = Proposal::new();
        p.set("x", Value::Int(1)).set("y", Value::Bool(true));
        assert_eq!(p.values.len(), 2);
    }

    #[test]
    fn empty_surfaces_empty_proposal_ok() {
        let surfaces: Vec<OptSurface> = vec![];
        let proposal = Proposal::new();
        assert!(validate_proposal(&proposal, &surfaces).is_ok());
    }
}
