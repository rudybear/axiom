# AXIOM Optimization Protocol Specification

Version 0.1 -- matches implemented code as of 2026-03-23.
Source of truth: `crates/axiom-optimize/src/surface.rs`,
`crates/axiom-optimize/src/proposal.rs`,
`crates/axiom-optimize/src/history.rs`,
`crates/axiom-optimize/src/benchmark.rs`,
`crates/axiom-optimize/src/agent_api.rs`.

## Overview

AXIOM programs expose explicit **optimization surfaces** -- structured
declarations of tunable parameters that AI agents can systematically
explore. The optimization protocol defines how these surfaces are declared
in source, extracted by the compiler, proposed by agents, validated,
benchmarked, and recorded.

## Optimization Holes

An optimization hole is written `?name` in AXIOM source. It represents a
parameter whose concrete value will be supplied by an AI agent during the
optimization loop. Holes appear inside `@strategy` blocks and, less
commonly, as standalone expressions in function bodies.

Syntax: `?` followed by an identifier (no space). The lexer produces an
`OptHole` token. In the AST, it becomes `Expr::OptHole(name)`.

Examples:
- `?tile_m` -- a tiling dimension
- `?unroll_factor` -- how many times to unroll a loop
- `?loop_order` -- the order in which to traverse loop dimensions
- `?prefetch_distance` -- how far ahead to prefetch memory

## Strategy Blocks

A `@strategy` block declares the full optimization surface for a function
or block. It maps named entries to either holes, sub-maps, or concrete values.

```axiom
@strategy {
    tiling:   { M: ?tile_m, N: ?tile_n, K: ?tile_k }
    order:    ?loop_order
    parallel: ?parallel_dims
    unroll:   ?unroll_factor
    prefetch: ?prefetch_distance
}
```

**Placement:** Valid on functions and blocks. When placed on a function, it
applies to the function's entire body. When placed on a block (e.g., inside
a for-loop body), it applies to that specific block.

**Entry syntax:** Each entry is `key: value` where value is one of:
- `?hole_name` -- an optimization hole
- `{ subkey: subvalue, ... }` -- a nested map of entries
- A concrete annotation value (integer, string, etc.)

Commas between entries are optional. The parser accepts both `key: value,`
and `key: value` followed by a newline.

## Hole Types

The surface extractor infers a type for each hole based on its context:

| Inferred Type | When |
|---------------|------|
| `u32` | Default for numeric parameters; keys containing `factor`, `distance`, `prefetch`, `unroll`; tiling sub-keys (`M`, `N`, `K`) |
| `array[ident]` | Keys containing `order` or `dims`; key `parallel` |
| `i32` | Explicitly declared signed integer holes |
| `f64` | Explicitly declared floating-point holes |
| `bool` | Explicitly declared boolean holes |
| `ident` | Explicitly declared identifier holes |

## Range Constraints

The extractor also infers default ranges for common hole names:

| Key pattern | Default range |
|-------------|---------------|
| Contains `unroll` or `factor` | `[1, 32]` |
| Contains `prefetch` or `distance` | `[0, 16]` |
| Sub-keys `M`, `N`, `K` (tiling dimensions) | `[1, 512]` |
| All others | No range constraint |

Ranges are inclusive on both ends. A proposed value outside the range is
rejected during validation.

## The Optimization Loop

The full optimization protocol follows this cycle:

### 1. Extract

The `extract_surfaces` function parses AXIOM source, lowers to HIR, and
walks the HIR looking for `@strategy` annotations and `?hole` expressions.
It produces a list of `OptSurface` descriptors, each containing:

- `function_name` -- which function this surface belongs to
- `holes` -- all discovered holes with name, type, range, and current value
- `strategy` -- structured information from the `@strategy` block

### 2. Propose

An AI agent examines the surfaces and proposes concrete values for every
hole. This is represented as a `Proposal` -- a map from hole name to
`Value`. Value types are:

| Variant | Rust type | Example |
|---------|-----------|---------|
| `Int`   | `i64`     | `Value::Int(64)` |
| `Float` | `f64`     | `Value::Float(3.14)` |
| `Bool`  | `bool`    | `Value::Bool(true)` |
| `Ident` | `String`  | `Value::Ident("i".into())` |
| `Array` | `Vec<Value>` | `Value::Array(vec![Value::Ident("i".into())])` |

### 3. Validate

The `validate_proposal` function checks:

1. **Completeness** -- every hole in every surface has a proposed value.
2. **Type correctness** -- each value matches the hole's declared type.
3. **Range validity** -- integer values fall within the hole's range.
4. **No unknowns** -- the proposal does not contain values for holes that
   do not exist in any surface.

All validation errors are collected (not just the first). Error types:

| Error | Meaning |
|-------|---------|
| `MissingHole` | A required hole has no proposed value |
| `TypeMismatch` | Value type does not match hole type |
| `OutOfRange` | Integer value is outside `[lo, hi]` range |
| `UnknownHole` | Proposal contains a hole name not in any surface |

### 4. Benchmark

The `benchmark_source` function compiles the AXIOM source (with concrete
hole values substituted) and runs the resulting binary multiple times.
Configuration:

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `warmup_runs` | 3 | Untimed warmup iterations |
| `measurement_runs` | 5 | Timed measurement iterations |
| `timeout_ms` | 30000 | Per-run timeout in milliseconds |

Alternatively, `benchmark_binary` benchmarks a pre-compiled binary.

Results include: `median_ms`, `mean_ms`, `min_ms`, `max_ms`, `stddev_ms`,
and the raw `times_ms` vector.

### 5. Record

The result is recorded as an `OptRecord` in the `OptHistory`:

```json
{
  "version": "v1",
  "params": { "tile_m": 64, "tile_n": 64, "unroll_factor": 4 },
  "metrics": { "time_ms": 28.1 },
  "agent": "optimizer-v1",
  "target": "x86_64",
  "timestamp": "1711152000"
}
```

History supports:
- `add_record` -- append a new record
- `best_by_metric(name)` -- find the record with the lowest value for a metric
- `next_version()` -- generate the next version label (`"v1"`, `"v2"`, ...)
- `to_json` / `from_json` -- serialize/deserialize for persistence

## Agent Session API

The `AgentSession` struct is the high-level entry point for AI agents. It
wraps the full optimization workflow:

```
AgentSession::from_source(source)    -- parse and extract surfaces
session.surfaces()                    -- get all optimization surfaces
session.apply_proposal(proposal, metrics, agent)
                                      -- validate + record
session.history()                     -- get optimization history
session.transfer()                    -- get transfer metadata
session.export_with_transfer(info)    -- emit source with @transfer block
session.save_history(path)            -- persist history to JSON file
session.load_history(path)            -- restore history from JSON file
session.summary()                     -- get session diagnostics
```

See `spec/transfer.md` for the inter-agent handoff protocol.

## `@optimization_log` Format

The `@optimization_log` annotation on a function records previous optimization
attempts inline in the source. Each entry is an `OptLogEntry` with:

| Field | Type | Description |
|-------|------|-------------|
| `version` | `String` | Version label (e.g., `"v1"`) |
| `params` | `Vec<(String, AnnotationValue)>` | Parameter values tried |
| `metrics` | `Vec<(String, f64)>` | Measured performance metrics |
| `agent` | `Option<String>` | Agent that produced this entry |
| `target` | `Option<String>` | Target architecture |
| `date` | `Option<String>` | Date of the optimization attempt |

This annotation is informational and does not affect code generation.
