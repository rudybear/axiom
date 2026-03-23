# AXIOM Inter-Agent Transfer Protocol

Version 0.1 -- matches implemented code as of 2026-03-23.
Source of truth: `crates/axiom-optimize/src/transfer.rs`,
`crates/axiom-optimize/src/agent_api.rs`.

## Overview

AXIOM is designed for multi-agent workflows where different AI agents
specialize in different tasks (code generation, optimization, verification,
benchmarking). The transfer protocol provides a structured way for agents
to hand off work, communicate context, flag open questions, and report
confidence levels.

## `@transfer` Block Syntax

The `@transfer` annotation uses key-value block syntax. It is valid on
functions, modules, and blocks.

```axiom
@transfer {
    source_agent: "optimizer-v1"
    target_agent: "verifier-v2"
    context: "Tiling applied to matmul inner loop"
    open_questions: ["Is prefetch distance optimal?", "Should we try col_major?"]
    confidence: { correctness: 0.95, optimality: 0.7 }
}
```

All fields are optional. Commas between entries are optional.

## Fields

| Field | Type | Description |
|-------|------|-------------|
| `source_agent` | String | Identifier of the agent that produced the current program state |
| `target_agent` | String | Identifier of the agent that should consume this program next |
| `context` | String | Free-form description of what was done or what needs to be done |
| `open_questions` | List of strings | Unresolved issues for the next agent to address |
| `confidence` | Map with `correctness` and `optimality` keys | Confidence scores in `[0.0, 1.0]` range |

### Confidence Scores

| Score | Range | Meaning |
|-------|-------|---------|
| `correctness` | 0.0 -- 1.0 | Confidence that the program is functionally correct |
| `optimality` | 0.0 -- 1.0 | Confidence that the program is near-optimal for its target |

## Rust API: `TransferInfo`

The `TransferInfo` struct is the Rust representation of a `@transfer` block.
It implements `Serialize` and `Deserialize` for JSON interchange.

```rust
pub struct TransferInfo {
    pub source_agent: Option<String>,
    pub target_agent: Option<String>,
    pub context: Option<String>,
    pub open_questions: Vec<String>,
    pub confidence: Option<Confidence>,
}

pub struct Confidence {
    pub correctness: f64,
    pub optimality: f64,
}
```

## Extraction

`extract_transfer(source: &str) -> Option<TransferInfo>` parses an AXIOM
source string, lowers to HIR, and searches all annotations (module-level,
function-level, and block-level) for a `@transfer` block. Returns the first
one found, or `None`.

`extract_transfer_from_hir(module: &HirModule) -> Option<TransferInfo>`
does the same from a pre-lowered HIR module.

## Generation

`generate_transfer(info: &TransferInfo) -> String` produces an AXIOM source
fragment suitable for embedding as an annotation:

```
@transfer {
    source_agent: "optimizer-v1"
    target_agent: "verifier-v2"
    context: "Tiling applied to matmul inner loop"
    open_questions: ["Is prefetch distance optimal?"]
    confidence: { correctness: 0.95, optimality: 0.7 }
}
```

## AgentSession Integration

The `AgentSession` API integrates transfer into the optimization workflow:

- **`AgentSession::from_source(source)`** -- automatically extracts any
  existing `@transfer` block from the source.
- **`session.transfer()`** -- returns the extracted `TransferInfo`, if present.
- **`session.set_transfer(info)`** -- manually sets transfer metadata.
- **`session.export_with_transfer(info)`** -- produces the full source text
  with a `@transfer` block appended inside a sentinel function
  (`fn __transfer__() -> i32 { ... }`). If a previous sentinel exists, it
  is replaced to avoid duplicates.

## Multi-Agent Handoff Workflow

A typical multi-agent optimization workflow:

1. **Generator agent** writes initial AXIOM source with `@strategy` blocks
   and `?holes`. Attaches `@transfer { source_agent: "generator", target_agent: "optimizer" }`.

2. **Optimizer agent** loads the source via `AgentSession::from_source`.
   Reads `session.transfer()` to understand context. Inspects
   `session.surfaces()` to discover holes. Proposes values, benchmarks,
   records results. Exports with updated transfer metadata targeting the
   verifier.

3. **Verifier agent** loads the exported source. Reads the transfer context.
   Checks correctness properties. Updates confidence scores. Exports with
   transfer metadata targeting the next agent or back to the optimizer if
   issues are found.

4. **Repeat** until confidence scores meet thresholds or a fixed iteration
   budget is exhausted.

## Serialization

`TransferInfo` and `Confidence` implement `serde::Serialize` and
`serde::Deserialize`. They can be serialized to JSON for out-of-band
communication between agents:

```json
{
  "source_agent": "optimizer-v1",
  "target_agent": "verifier-v2",
  "context": "Tiling applied to matmul inner loop",
  "open_questions": ["Is prefetch distance optimal?"],
  "confidence": {
    "correctness": 0.95,
    "optimality": 0.7
  }
}
```

Optional fields that are `None` are omitted from the JSON output.
