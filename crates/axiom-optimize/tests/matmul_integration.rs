//! Integration tests for the matmul optimization demo.
//!
//! These tests exercise the full optimization protocol:
//!   surface extraction -> proposal -> validation -> history recording.

use std::collections::HashMap;

use axiom_optimize::history::{OptHistory, OptRecord};
use axiom_optimize::proposal::{validate_proposal, Proposal};
use axiom_optimize::surface::{extract_surfaces, HoleType, Value};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Workspace root relative to CARGO_MANIFEST_DIR for axiom-optimize.
fn workspace_root() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.join("..").join("..")
}

fn load_sample(name: &str) -> String {
    let path = workspace_root().join("tests").join("samples").join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Test: matmul_naive.axm surfaces
// ---------------------------------------------------------------------------

#[test]
fn test_matmul_naive_surfaces() {
    let source = load_sample("matmul_naive.axm");
    let surfaces = extract_surfaces(&source).expect("matmul_naive.axm should parse and lower");

    // There should be exactly one function with a strategy block: `matmul`
    assert_eq!(surfaces.len(), 1, "expected 1 surface, got {}", surfaces.len());
    let surface = &surfaces[0];
    assert_eq!(surface.function_name, "matmul");

    // The strategy block has 7 holes:
    //   tiling: { M: ?tile_m, N: ?tile_n, K: ?tile_k }
    //   order:    ?loop_order
    //   parallel: ?parallel_dims
    //   unroll:   ?unroll_factor
    //   prefetch: ?prefetch_distance
    assert_eq!(
        surface.holes.len(),
        7,
        "expected 7 holes, got {}. holes: {:?}",
        surface.holes.len(),
        surface.holes.iter().map(|h| &h.name).collect::<Vec<_>>()
    );

    // Verify each hole by name, type, and range
    let hole_map: HashMap<&str, _> = surface
        .holes
        .iter()
        .map(|h| (h.name.as_str(), h))
        .collect();

    // Tiling holes
    for dim in &["tile_m", "tile_n", "tile_k"] {
        let hole = hole_map
            .get(dim)
            .unwrap_or_else(|| panic!("missing hole: {dim}"));
        assert_eq!(hole.hole_type, HoleType::U32, "{dim} should be U32");
        assert_eq!(hole.range, Some((1, 512)), "{dim} should have range [1, 512]");
    }

    // Loop order hole
    let loop_order = hole_map.get("loop_order").expect("missing loop_order");
    assert_eq!(
        loop_order.hole_type,
        HoleType::Array(Box::new(HoleType::Ident))
    );

    // Parallel dims hole
    let parallel = hole_map.get("parallel_dims").expect("missing parallel_dims");
    assert_eq!(
        parallel.hole_type,
        HoleType::Array(Box::new(HoleType::Ident))
    );

    // Unroll factor hole
    let unroll = hole_map.get("unroll_factor").expect("missing unroll_factor");
    assert_eq!(unroll.hole_type, HoleType::U32);
    assert_eq!(unroll.range, Some((1, 32)));

    // Prefetch distance hole
    let prefetch = hole_map
        .get("prefetch_distance")
        .expect("missing prefetch_distance");
    assert_eq!(prefetch.hole_type, HoleType::U32);
    assert_eq!(prefetch.range, Some((0, 16)));

    // Strategy info should be present
    let strategy = surface.strategy.as_ref().expect("should have strategy info");
    assert_eq!(
        strategy.entries.len(),
        5,
        "strategy should have 5 top-level entries (tiling, order, parallel, unroll, prefetch)"
    );
}

// ---------------------------------------------------------------------------
// Test: matmul_simple.axm surfaces
// ---------------------------------------------------------------------------

#[test]
fn test_matmul_simple_surfaces() {
    let source = load_sample("matmul_simple.axm");
    let surfaces = extract_surfaces(&source).expect("matmul_simple.axm should parse and lower");

    // matmul_simple has one strategy block; main has none.
    assert_eq!(surfaces.len(), 1, "expected 1 surface, got {}", surfaces.len());
    let surface = &surfaces[0];
    assert_eq!(surface.function_name, "matmul_simple");

    // Single hole: ?unroll_factor
    assert_eq!(surface.holes.len(), 1);
    let hole = &surface.holes[0];
    assert_eq!(hole.name, "unroll_factor");
    assert_eq!(hole.hole_type, HoleType::U32);
    assert_eq!(hole.range, Some((1, 32)));

    // Strategy should be present
    assert!(surface.strategy.is_some());
}

// ---------------------------------------------------------------------------
// Test: full optimization flow
// ---------------------------------------------------------------------------

#[test]
fn test_optimization_flow() {
    // ---------------------------------------------------------------
    // Step 1: Extract surfaces from matmul_naive.axm
    // ---------------------------------------------------------------
    let source = load_sample("matmul_naive.axm");
    let surfaces = extract_surfaces(&source).expect("should extract surfaces");
    assert_eq!(surfaces.len(), 1);
    assert_eq!(surfaces[0].holes.len(), 7);

    // ---------------------------------------------------------------
    // Step 2: Build a proposal (simulating an AI agent)
    // ---------------------------------------------------------------
    let mut proposal = Proposal::new();
    proposal.set("tile_m", Value::Int(64));
    proposal.set("tile_n", Value::Int(64));
    proposal.set("tile_k", Value::Int(32));
    proposal.set(
        "loop_order",
        Value::Array(vec![
            Value::Ident("i".into()),
            Value::Ident("k".into()),
            Value::Ident("j".into()),
        ]),
    );
    proposal.set(
        "parallel_dims",
        Value::Array(vec![Value::Ident("i".into())]),
    );
    proposal.set("unroll_factor", Value::Int(4));
    proposal.set("prefetch_distance", Value::Int(8));

    // ---------------------------------------------------------------
    // Step 3: Validate the proposal
    // ---------------------------------------------------------------
    validate_proposal(&proposal, &surfaces)
        .expect("proposal should be valid");

    // ---------------------------------------------------------------
    // Step 4: Record in history (simulating a benchmark result)
    // ---------------------------------------------------------------
    let mut history = OptHistory::new();
    assert_eq!(history.next_version(), "v1");

    // First iteration record
    let record_v1 = OptRecord::new("v1")
        .with_param("tile_m", serde_json::Value::Number(64.into()))
        .with_param("tile_n", serde_json::Value::Number(64.into()))
        .with_param("tile_k", serde_json::Value::Number(32.into()))
        .with_param(
            "loop_order",
            serde_json::json!(["i", "k", "j"]),
        )
        .with_param(
            "parallel_dims",
            serde_json::json!(["i"]),
        )
        .with_param("unroll_factor", serde_json::Value::Number(4.into()))
        .with_param("prefetch_distance", serde_json::Value::Number(8.into()))
        .with_metric("time_ms", 28.1)
        .with_agent("test-optimizer")
        .with_target("native")
        .with_timestamp("2026-03-22T00:00:00Z");

    history.add_record(record_v1);
    assert_eq!(history.next_version(), "v2");

    // ---------------------------------------------------------------
    // Step 5: Simulate a second iteration with different values
    // ---------------------------------------------------------------
    let mut proposal_v2 = Proposal::new();
    proposal_v2.set("tile_m", Value::Int(128));
    proposal_v2.set("tile_n", Value::Int(128));
    proposal_v2.set("tile_k", Value::Int(64));
    proposal_v2.set(
        "loop_order",
        Value::Array(vec![
            Value::Ident("j".into()),
            Value::Ident("i".into()),
            Value::Ident("k".into()),
        ]),
    );
    proposal_v2.set(
        "parallel_dims",
        Value::Array(vec![
            Value::Ident("i".into()),
            Value::Ident("j".into()),
        ]),
    );
    proposal_v2.set("unroll_factor", Value::Int(8));
    proposal_v2.set("prefetch_distance", Value::Int(4));

    // Validate second proposal
    validate_proposal(&proposal_v2, &surfaces)
        .expect("second proposal should also be valid");

    let record_v2 = OptRecord::new("v2")
        .with_param("tile_m", serde_json::Value::Number(128.into()))
        .with_param("tile_n", serde_json::Value::Number(128.into()))
        .with_param("tile_k", serde_json::Value::Number(64.into()))
        .with_param(
            "loop_order",
            serde_json::json!(["j", "i", "k"]),
        )
        .with_param(
            "parallel_dims",
            serde_json::json!(["i", "j"]),
        )
        .with_param("unroll_factor", serde_json::Value::Number(8.into()))
        .with_param("prefetch_distance", serde_json::Value::Number(4.into()))
        .with_metric("time_ms", 19.5)
        .with_agent("test-optimizer")
        .with_target("native")
        .with_timestamp("2026-03-22T00:01:00Z");

    history.add_record(record_v2);
    assert_eq!(history.next_version(), "v3");
    assert_eq!(history.records.len(), 2);

    // ---------------------------------------------------------------
    // Step 6: Query history for best result
    // ---------------------------------------------------------------
    let best = history
        .best_by_metric("time_ms")
        .expect("should find best");
    assert_eq!(best.version, "v2", "v2 should be faster");
    assert!((best.metrics["time_ms"] - 19.5).abs() < f64::EPSILON);

    // ---------------------------------------------------------------
    // Step 7: Verify history JSON round-trip
    // ---------------------------------------------------------------
    let json = history.to_json().expect("should serialize");
    let restored = OptHistory::from_json(&json).expect("should deserialize");
    assert_eq!(restored.records.len(), 2);
    assert_eq!(restored, history);

    // ---------------------------------------------------------------
    // Step 8: Verify that invalid proposals are rejected
    // ---------------------------------------------------------------

    // 8a: Out-of-range value
    let mut bad_proposal = Proposal::new();
    bad_proposal.set("tile_m", Value::Int(64));
    bad_proposal.set("tile_n", Value::Int(64));
    bad_proposal.set("tile_k", Value::Int(32));
    bad_proposal.set(
        "loop_order",
        Value::Array(vec![
            Value::Ident("i".into()),
            Value::Ident("j".into()),
            Value::Ident("k".into()),
        ]),
    );
    bad_proposal.set(
        "parallel_dims",
        Value::Array(vec![Value::Ident("i".into())]),
    );
    bad_proposal.set("unroll_factor", Value::Int(999)); // out of range [1, 32]
    bad_proposal.set("prefetch_distance", Value::Int(8));

    let err = validate_proposal(&bad_proposal, &surfaces);
    assert!(err.is_err(), "proposal with out-of-range unroll should fail");

    // 8b: Missing holes
    let incomplete = Proposal::new(); // no values at all
    let err = validate_proposal(&incomplete, &surfaces);
    assert!(err.is_err(), "empty proposal should fail for matmul");
    let errors = err.unwrap_err();
    assert_eq!(
        errors.len(),
        7,
        "should have 7 missing-hole errors, got {}",
        errors.len()
    );
}

// ---------------------------------------------------------------------------
// Test: simplified matmul optimization flow
// ---------------------------------------------------------------------------

#[test]
fn test_matmul_simple_optimization_flow() {
    let source = load_sample("matmul_simple.axm");
    let surfaces = extract_surfaces(&source).expect("should parse");
    assert_eq!(surfaces.len(), 1);

    // Valid proposal
    let mut proposal = Proposal::new();
    proposal.set("unroll_factor", Value::Int(4));
    validate_proposal(&proposal, &surfaces).expect("valid proposal");

    // Record in history
    let mut history = OptHistory::new();
    let record = OptRecord::new(history.next_version())
        .with_param("unroll_factor", serde_json::Value::Number(4.into()))
        .with_metric("time_ms", 50.0)
        .with_agent("test-optimizer")
        .with_timestamp("2026-03-22T00:00:00Z");
    history.add_record(record);

    // Second iteration
    let mut proposal2 = Proposal::new();
    proposal2.set("unroll_factor", Value::Int(8));
    validate_proposal(&proposal2, &surfaces).expect("valid proposal");

    let record2 = OptRecord::new(history.next_version())
        .with_param("unroll_factor", serde_json::Value::Number(8.into()))
        .with_metric("time_ms", 42.0)
        .with_agent("test-optimizer")
        .with_timestamp("2026-03-22T00:01:00Z");
    history.add_record(record2);

    // Best should be v2
    let best = history.best_by_metric("time_ms").unwrap();
    assert_eq!(best.version, "v2");

    // JSON round-trip
    let json = history.to_json().unwrap();
    let restored = OptHistory::from_json(&json).unwrap();
    assert_eq!(restored, history);

    // Invalid: out of range
    let mut bad = Proposal::new();
    bad.set("unroll_factor", Value::Int(100));
    assert!(validate_proposal(&bad, &surfaces).is_err());

    // Invalid: wrong type
    let mut wrong_type = Proposal::new();
    wrong_type.set("unroll_factor", Value::Bool(true));
    assert!(validate_proposal(&wrong_type, &surfaces).is_err());
}
