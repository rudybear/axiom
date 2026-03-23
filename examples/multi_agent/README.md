# Multi-Agent Optimization Workflow

This example demonstrates a 3-agent handoff chain for optimizing an AXIOM
matrix multiplication kernel. Each agent specializes in a different concern,
progressively improving the program and handing off context to the next.

## Agent Chain

```
Agent A (Writer) --> Agent B (Optimizer) --> Agent C (GPU Specialist)
```

### Agent A: "Writer"

- Starts from `matmul_naive.axm`, the unoptimized baseline.
- Extracts the optimization surfaces (`@strategy` block with 7 holes).
- Fills holes with conservative default values (small tiles, no unrolling).
- Measures baseline performance (~100 ms, ~2.5 GFLOPS).
- Generates a `@transfer` block targeting `agent-optimizer` with notes about
  potential improvements (loop reordering, larger tiles).

### Agent B: "Optimizer"

- Loads the exported source from Agent A.
- Reads Agent A's `@transfer` block to understand context and open questions.
- Applies two rounds of optimization:
  1. `ikj` loop order with 64x64 tiling and 4x unroll (42 ms, 6 GFLOPS).
  2. 128x128 tiling with 8x unroll and dual-axis parallelism (28 ms, 9 GFLOPS).
- Queries history to confirm v2 is the best result.
- Hands off to `agent-gpu-specialist` with notes about GPU offload potential.

### Agent C: "GPU Specialist"

- Loads the exported source from Agent B.
- Reads Agent B's `@transfer` block for GPU-specific context.
- Can see that correctness confidence is 0.99 and optimality is 0.7.
- Would proceed with GPU tiling, shared memory, and tensor core evaluation.

## Key API Concepts

### AgentSession

The `AgentSession` is the primary interface. Each agent creates one from
source text (or a file), inspects surfaces, applies proposals, and exports
with transfer metadata.

```rust
let mut session = AgentSession::from_source(source)?;
let surfaces = session.surfaces();
session.apply_proposal(proposal, metrics, "my-agent")?;
let output = session.export_with_transfer(transfer_info);
```

### Transfer Protocol

The `@transfer` block carries structured handoff metadata:

- `source_agent` / `target_agent` — who produced / who should consume.
- `context` — free-form description of what was done.
- `open_questions` — issues for the next agent to address.
- `confidence` — correctness and optimality scores (0.0 to 1.0).

### History

Each session independently tracks optimization history. Records include
parameter values, metrics, agent name, and timestamps. History can be
serialized to JSON and persisted alongside the source.

## Files

- `scenario.axm` — Example output showing what a program looks like after
  passing through multiple agents, with `@transfer` and `@optimization_log`
  annotations.

## Running the Test

```bash
cargo test --package axiom-optimize --test multi_agent
```
