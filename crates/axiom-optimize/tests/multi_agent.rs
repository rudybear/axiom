//! Integration tests for the multi-agent optimization workflow.
//!
//! Demonstrates a 3-agent handoff chain:
//!   Agent A ("Writer")         — starts from naive matmul, fills defaults
//!   Agent B ("Optimizer")      — refines for CPU cache locality
//!   Agent C ("GPU Specialist") — receives the optimized program for GPU offload

use std::collections::HashMap;

use axiom_optimize::agent_api::AgentSession;
use axiom_optimize::proposal::Proposal;
use axiom_optimize::surface::Value;
use axiom_optimize::transfer::{Confidence, TransferInfo};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a full matmul proposal with the given tile sizes, loop order, and
/// unroll/prefetch knobs.
fn matmul_proposal(
    tile_m: i64,
    tile_n: i64,
    tile_k: i64,
    loop_order: &[&str],
    parallel_dims: &[&str],
    unroll_factor: i64,
    prefetch_distance: i64,
) -> Proposal {
    let mut proposal = Proposal::new();
    proposal.set("tile_m", Value::Int(tile_m));
    proposal.set("tile_n", Value::Int(tile_n));
    proposal.set("tile_k", Value::Int(tile_k));
    proposal.set(
        "loop_order",
        Value::Array(loop_order.iter().map(|s| Value::Ident(s.to_string())).collect()),
    );
    proposal.set(
        "parallel_dims",
        Value::Array(parallel_dims.iter().map(|s| Value::Ident(s.to_string())).collect()),
    );
    proposal.set("unroll_factor", Value::Int(unroll_factor));
    proposal.set("prefetch_distance", Value::Int(prefetch_distance));
    proposal
}

// ---------------------------------------------------------------------------
// Test: full multi-agent workflow with 3-agent chain
// ---------------------------------------------------------------------------

#[test]
fn test_multi_agent_workflow() {
    // =================================================================
    // Agent A: "Writer" — starts with the naive matmul and fills defaults
    // =================================================================
    let source = include_str!("../../../tests/samples/matmul_naive.axm");
    let mut session_a = AgentSession::from_source(source).expect("Agent A: parse source");

    // Agent A discovers optimisation surfaces
    let surfaces = session_a.surfaces();
    assert!(!surfaces.is_empty(), "Agent A should discover surfaces");
    assert_eq!(surfaces[0].function_name, "matmul");
    assert_eq!(surfaces[0].holes.len(), 7, "matmul has 7 holes");

    // Agent A makes a conservative first proposal (small tiles, default order)
    let proposal_a = matmul_proposal(
        32,               // tile_m
        32,               // tile_n
        32,               // tile_k
        &["i", "j", "k"], // loop_order — naive row-major
        &["i"],           // parallel_dims — parallelise outer loop
        1,                // unroll_factor — no unrolling yet
        0,                // prefetch_distance — no prefetching
    );

    let mut metrics_a = HashMap::new();
    metrics_a.insert("time_ms".to_string(), 100.0);
    metrics_a.insert("gflops".to_string(), 2.5);

    session_a
        .apply_proposal(proposal_a, metrics_a, "agent-writer")
        .expect("Agent A: apply proposal");

    assert_eq!(session_a.history().records.len(), 1);
    assert_eq!(
        session_a.history().records[0].agent.as_deref(),
        Some("agent-writer")
    );

    // Agent A generates a transfer handoff → Agent B
    let transfer_a = TransferInfo {
        source_agent: Some("agent-writer".into()),
        target_agent: Some("agent-optimizer".into()),
        context: Some(
            "Initial implementation. Optimization holes filled with conservative defaults."
                .into(),
        ),
        open_questions: vec![
            "Consider loop reordering for cache locality".into(),
            "Tile sizes are small — experiment with 64 and 128".into(),
        ],
        confidence: Some(Confidence {
            correctness: 0.95,
            optimality: 0.3,
        }),
    };
    let exported_a = session_a.export_with_transfer(transfer_a);

    // Verify the export contains the transfer block
    assert!(
        exported_a.contains("agent-writer"),
        "exported source should mention agent-writer"
    );
    assert!(
        exported_a.contains("agent-optimizer"),
        "exported source should mention agent-optimizer"
    );

    // =================================================================
    // Agent B: "Optimizer" — picks up from Agent A
    // =================================================================
    let mut session_b =
        AgentSession::from_source(&exported_a).expect("Agent B: parse exported source");

    // Agent B can see Agent A's transfer info
    let transfer_b = session_b.transfer();
    assert!(
        transfer_b.is_some(),
        "Agent B should see transfer info from Agent A"
    );
    let tb = transfer_b.unwrap();
    assert_eq!(tb.source_agent.as_deref(), Some("agent-writer"));
    assert_eq!(tb.target_agent.as_deref(), Some("agent-optimizer"));
    assert!(tb
        .context
        .as_deref()
        .unwrap()
        .contains("conservative defaults"));

    // Agent B discovers the same surfaces
    let surfaces_b = session_b.surfaces();
    assert_eq!(surfaces_b.len(), 1);
    assert_eq!(surfaces_b[0].holes.len(), 7);

    // Agent B makes an improved proposal (cache-friendly order, larger tiles)
    let proposal_b = matmul_proposal(
        64,                // tile_m — doubled
        64,                // tile_n — doubled
        32,                // tile_k — keep at 32 for L1 fit
        &["i", "k", "j"], // loop_order — ikj for better cache locality
        &["i"],           // parallel_dims
        4,                // unroll_factor — 4x unroll
        8,                // prefetch_distance
    );

    let mut metrics_b = HashMap::new();
    metrics_b.insert("time_ms".to_string(), 42.0);
    metrics_b.insert("gflops".to_string(), 6.0);

    session_b
        .apply_proposal(proposal_b, metrics_b, "agent-optimizer")
        .expect("Agent B: apply first proposal");

    // Agent B tries a second proposal with even larger tiles
    let proposal_b2 = matmul_proposal(
        128,               // tile_m — even larger
        128,               // tile_n
        64,                // tile_k
        &["i", "k", "j"], // loop_order — keep ikj
        &["i", "j"],      // parallel_dims — parallel on i and j
        8,                 // unroll_factor
        4,                 // prefetch_distance
    );

    let mut metrics_b2 = HashMap::new();
    metrics_b2.insert("time_ms".to_string(), 28.0);
    metrics_b2.insert("gflops".to_string(), 9.0);

    session_b
        .apply_proposal(proposal_b2, metrics_b2, "agent-optimizer")
        .expect("Agent B: apply second proposal");

    // Agent B now has 2 records; can query the best
    assert_eq!(session_b.history().records.len(), 2);
    let best = session_b
        .history()
        .best_by_metric("time_ms")
        .expect("should find best");
    assert_eq!(best.version, "v2", "v2 (128x128x64) should be faster");

    // Agent B hands off to Agent C
    let transfer_b_out = TransferInfo {
        source_agent: Some("agent-optimizer".into()),
        target_agent: Some("agent-gpu-specialist".into()),
        context: Some(
            "CPU-optimized with ikj loop order and 128x128 tiling. \
             Consider GPU offload for large matrices."
                .into(),
        ),
        open_questions: vec![
            "For GPU: consider shared memory tiling".into(),
            "Evaluate whether tensor cores can be used".into(),
        ],
        confidence: Some(Confidence {
            correctness: 0.99,
            optimality: 0.7,
        }),
    };
    let exported_b = session_b.export_with_transfer(transfer_b_out);

    // =================================================================
    // Agent C: "GPU Specialist" — picks up from Agent B
    // =================================================================
    let session_c =
        AgentSession::from_source(&exported_b).expect("Agent C: parse exported source");

    // Agent C can see Agent B's transfer info
    let transfer_c = session_c.transfer();
    assert!(
        transfer_c.is_some(),
        "Agent C should see transfer info from Agent B"
    );
    let tc = transfer_c.unwrap();
    assert_eq!(tc.source_agent.as_deref(), Some("agent-optimizer"));
    assert_eq!(tc.target_agent.as_deref(), Some("agent-gpu-specialist"));
    assert!(tc.context.as_deref().unwrap().contains("GPU offload"));

    // Confidence should have increased through the chain
    let conf = tc.confidence.as_ref().expect("should have confidence");
    assert!(
        conf.optimality > 0.3,
        "optimality should improve from Writer to Optimizer"
    );

    // =================================================================
    // Verify the full chain: writer → optimizer → gpu-specialist
    // =================================================================
    let final_output = exported_b;
    assert!(
        final_output.contains("agent-optimizer"),
        "final output should reference agent-optimizer"
    );
    assert!(
        final_output.contains("agent-gpu-specialist"),
        "final output should reference agent-gpu-specialist"
    );
    assert!(
        final_output.contains("@transfer"),
        "final output should contain a @transfer block"
    );
}

// ---------------------------------------------------------------------------
// Test: transfer metadata survives re-parse
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_survives_reparse() {
    let source = include_str!("../../../tests/samples/matmul_naive.axm");
    let session = AgentSession::from_source(source).expect("parse");

    let transfer = TransferInfo {
        source_agent: Some("alpha".into()),
        target_agent: Some("beta".into()),
        context: Some("round-trip test".into()),
        open_questions: vec!["Does re-parse preserve everything?".into()],
        confidence: Some(Confidence {
            correctness: 0.88,
            optimality: 0.55,
        }),
    };

    let exported = session.export_with_transfer(transfer);

    // Re-parse the exported text
    let session2 = AgentSession::from_source(&exported).expect("re-parse exported");
    let t = session2.transfer().expect("should preserve transfer");
    assert_eq!(t.source_agent.as_deref(), Some("alpha"));
    assert_eq!(t.target_agent.as_deref(), Some("beta"));
    assert_eq!(t.context.as_deref(), Some("round-trip test"));
}

// ---------------------------------------------------------------------------
// Test: each agent can independently track history
// ---------------------------------------------------------------------------

#[test]
fn test_independent_histories() {
    let source = r#"
fn compute(a: i32, b: i32) -> i32 {
    @strategy {
        unroll: ?unroll_factor
    }
    return a + b;
}
"#;

    // Agent A
    let mut session_a = AgentSession::from_source(source).expect("parse");
    let mut prop = Proposal::new();
    prop.set("unroll_factor", Value::Int(2));
    let mut m = HashMap::new();
    m.insert("time_ms".to_string(), 50.0);
    session_a
        .apply_proposal(prop, m, "agent-a")
        .expect("apply");
    assert_eq!(session_a.history().records.len(), 1);

    // Agent B starts from the same source — independent history
    let mut session_b = AgentSession::from_source(source).expect("parse");
    assert_eq!(
        session_b.history().records.len(),
        0,
        "Agent B starts with empty history"
    );

    let mut prop2 = Proposal::new();
    prop2.set("unroll_factor", Value::Int(8));
    let mut m2 = HashMap::new();
    m2.insert("time_ms".to_string(), 30.0);
    session_b
        .apply_proposal(prop2, m2, "agent-b")
        .expect("apply");

    // Both agents have exactly 1 record each
    assert_eq!(session_a.history().records.len(), 1);
    assert_eq!(session_b.history().records.len(), 1);

    // Different agents are recorded
    assert_eq!(
        session_a.history().records[0].agent.as_deref(),
        Some("agent-a")
    );
    assert_eq!(
        session_b.history().records[0].agent.as_deref(),
        Some("agent-b")
    );
}

// ---------------------------------------------------------------------------
// Test: session summary reflects multi-agent state
// ---------------------------------------------------------------------------

#[test]
fn test_multi_agent_summary() {
    let source = include_str!("../../../tests/samples/matmul_naive.axm");
    let mut session = AgentSession::from_source(source).expect("parse");

    let summary = session.summary();
    assert_eq!(summary.num_surfaces, 1);
    assert_eq!(summary.total_holes, 7);
    assert_eq!(summary.num_history_records, 0);
    assert!(!summary.has_transfer);

    // Apply a proposal
    let proposal = matmul_proposal(32, 32, 32, &["i", "j", "k"], &["i"], 1, 0);
    let mut m = HashMap::new();
    m.insert("time_ms".to_string(), 100.0);
    session
        .apply_proposal(proposal, m, "agent-writer")
        .expect("apply");

    // Set transfer
    session.set_transfer(TransferInfo {
        source_agent: Some("agent-writer".into()),
        target_agent: None,
        context: None,
        open_questions: vec![],
        confidence: None,
    });

    let summary2 = session.summary();
    assert_eq!(summary2.num_history_records, 1);
    assert!(summary2.has_transfer);

    // Display format includes all info
    let display = format!("{summary2}");
    assert!(display.contains("surfaces=1"));
    assert!(display.contains("holes=7"));
    assert!(display.contains("history=1 records"));
    assert!(display.contains("transfer=yes"));
}
