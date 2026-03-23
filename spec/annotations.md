# AXIOM Annotation Schema

Version 0.1 -- matches implemented parser and HIR as of 2026-03-23.
Source of truth: `crates/axiom-parser/src/ast.rs` (`Annotation` enum),
`crates/axiom-hir/src/hir.rs` (`HirAnnotationKind` enum),
`crates/axiom-hir/src/lower.rs` (`annotation_valid_targets` function).

## Overview

Annotations are first-class structured metadata attached to AXIOM program
elements. They are not strings or comments -- they are typed data that the
compiler preserves through all IR levels. The syntax is `@name`, `@name(args)`,
or `@name { key: value }` depending on the annotation type.

Annotations serve three purposes:
1. **Semantic intent** -- communicate what code does and why.
2. **Optimization guidance** -- declare tunable surfaces and constraints.
3. **Inter-agent transfer** -- carry metadata for multi-agent workflows.

## Annotation Target Types

Each annotation is valid only on specific targets. The compiler validates
placement during AST-to-HIR lowering.

| Target | Description |
|--------|-------------|
| Module | Top-level module (annotations before any item, terminated by `;`) |
| Function | Function definition |
| Param | Function parameter (inline, after the type) |
| StructDef | Struct definition |
| StructField | Individual struct field (inline, after the type) |
| Block | Block of statements (`{ ... }`) |

---

## Semantic Annotations

### `@pure`

**Syntax:** `@pure`
**Valid targets:** Function
**Meaning:** The function has no side effects. The return value depends only on
the arguments. Calls with the same arguments always produce the same result.
**Effect on compilation:** Enables common subexpression elimination, dead call
removal, and memoization at the optimizer's discretion.

```axiom
@pure
fn square(x: i32) -> i32 {
    return x * x;
}
```

### `@const`

**Syntax:** `@const`
**Valid targets:** Function
**Meaning:** The function can be evaluated at compile time. Implies `@pure`.
**Effect on compilation:** The compiler may replace calls with constant
arguments by their computed result during constant folding.

```axiom
@const
fn factorial(n: i32) -> i64 {
    if n <= 1 { return widen(1); }
    return widen(n) * factorial(n - 1);
}
```

### `@inline`

**Syntax:** `@inline(always)`, `@inline(never)`, `@inline(hint)`, or bare `@inline`
**Valid targets:** Function
**Meaning:** Controls inlining behavior.
- `always` -- always inline this function at every call site.
- `never` -- never inline this function.
- `hint` -- suggest inlining but let the optimizer decide (default if bare).
**Effect on compilation:** Maps to LLVM `alwaysinline`, `noinline`, or
`inlinehint` function attributes.

```axiom
@inline(always)
fn fast_abs(x: i32) -> i32 {
    if x < 0 { return 0 - x; }
    return x;
}
```

### `@complexity`

**Syntax:** `@complexity <freeform-text>`
**Valid targets:** Function
**Meaning:** Declares the algorithmic complexity class (e.g., `O(n)`,
`O(n^2)`, `O(n log n)`). Currently stored as a string; structured
complexity expressions are planned.
**Effect on compilation:** Informational. May be used by AI agents to
reason about algorithmic choices.

```axiom
@complexity O(n^3)
fn matmul(a: i32, b: i32) -> i32 { ... }
```

### `@intent`

**Syntax:** `@intent("description string")`
**Valid targets:** Function, Module
**Meaning:** A natural-language description of what this code does. Intended
for AI agent consumption.
**Effect on compilation:** Preserved as metadata. No direct effect on code
generation.

```axiom
@intent("Compute the dot product of two vectors");
fn dot(a: i32, b: i32) -> i32 { ... }
```

### `@export`

**Syntax:** `@export`
**Valid targets:** Function
**Meaning:** The function is externally visible with C calling convention.
The symbol name is not mangled.
**Effect on compilation:** Marks the function as `external` linkage in LLVM IR.

```axiom
@export
fn compute(x: i32) -> i32 { return x * 2; }
```

### `@module`

**Syntax:** `@module <name>`
**Valid targets:** Module
**Meaning:** Declares the module name. At most one `@module` annotation per file.
**Effect on compilation:** Sets the module name in the HIR. Duplicate
`@module` annotations produce an error.

```axiom
@module mylib;
```

---

## Constraint Annotations

### `@constraint`

**Syntax:** `@constraint { key: value, key: value }`
**Valid targets:** Function, Module
**Meaning:** Declares hard constraints that the optimizer must respect.
Keys and values are arbitrary annotation values (strings, ints, floats, etc.).
**Effect on compilation:** Constraints are preserved in the HIR and checked
during optimization proposal validation.

```axiom
@constraint { correctness: "IEEE 754 compliant", max_memory_mb: 256 };
```

### `@target`

**Syntax:** `@target { target1, target2 }` (dotted identifiers inside braces)
**Valid targets:** Function, Module
**Meaning:** Declares which hardware targets this code is designed for.
Targets are dotted identifiers like `cpu.simd`, `gpu.compute`.
**Effect on compilation:** Guides target-specific optimization passes.

```axiom
@target { cpu.simd, gpu.compute };
```

---

## Optimization Annotations

### `@strategy`

**Syntax:** `@strategy { entry: value, entry: value }`
**Valid targets:** Function, Block
**Meaning:** Declares an optimization surface. Each entry is a key mapping
to either a `?hole` (optimization parameter to be filled by an AI agent),
a sub-map of entries, or a concrete value. See `spec/optimization.md` for
the full optimization protocol.
**Effect on compilation:** The optimization surface extractor
(`axiom_optimize::surface`) reads these blocks to build `OptSurface`
descriptors. AI agents then propose values for the holes.

```axiom
@strategy {
    tiling:   { M: ?tile_m, N: ?tile_n, K: ?tile_k }
    order:    ?loop_order
    unroll:   ?unroll_factor
    prefetch: ?prefetch_distance
}
```

### `@vectorizable`

**Syntax:** `@vectorizable(dim1, dim2, ...)`
**Valid targets:** Function
**Meaning:** Declares which loop dimensions can be auto-vectorized.
**Effect on compilation:** Informs the vectorization pass which loops are
safe to vectorize.

```axiom
@vectorizable(i, j, k)
fn matmul(...) -> ... { ... }
```

### `@parallel`

**Syntax:** `@parallel(dim1, dim2, ...)`
**Valid targets:** Function
**Meaning:** Declares which loop dimensions can be parallelized across
threads or SIMD lanes.
**Effect on compilation:** Informs the parallelization pass which loops can
execute concurrently.

```axiom
@parallel(i, j)
fn matmul(...) -> ... { ... }
```

---

## Layout Annotations

### `@layout`

**Syntax:** `@layout(row_major)`, `@layout(col_major)`, `@layout(<custom>)`
**Valid targets:** Param, StructField
**Meaning:** Specifies the memory layout of a tensor or struct field.
- `row_major` -- C-style row-major order (last index varies fastest).
- `col_major` -- Fortran-style column-major order (first index varies fastest).
- Any other identifier is treated as a custom layout name.
**Effect on compilation:** Determines how multi-dimensional data is linearized
in memory. Affects code generation for index calculations.

```axiom
fn matmul(a: tensor[f32, M, K] @layout(row_major)) -> ... { ... }
```

### `@align`

**Syntax:** `@align(<integer>)`
**Valid targets:** Param, StructField
**Meaning:** Specifies the minimum alignment in bytes for this parameter or
field. Typical values: 16, 32, 64.
**Effect on compilation:** Emits alignment attributes in LLVM IR. Enables
aligned SIMD load/store instructions.

```axiom
fn matmul(a: tensor[f32, M, K] @align(64)) -> ... { ... }
```

---

## Transfer Annotations

### `@transfer`

**Syntax:** `@transfer { key: value, key: value }`
**Valid targets:** Function, Module, Block
**Meaning:** Carries metadata for inter-agent handoff. See `spec/transfer.md`
for the full protocol.
**Recognized fields:**
- `source_agent: "name"` -- agent that produced this state
- `target_agent: "name"` -- agent that should consume it
- `context: "description"` -- what was done or needs to be done
- `open_questions: ["q1", "q2"]` -- unresolved issues
- `confidence: { correctness: 0.95, optimality: 0.7 }` -- confidence scores
**Effect on compilation:** Preserved as metadata. Extractable via
`axiom_optimize::transfer::extract_transfer`.

```axiom
@transfer {
    source_agent: "optimizer-v1"
    target_agent: "verifier-v2"
    context: "Tiling applied to inner loop"
    open_questions: ["Is prefetch distance optimal?"]
    confidence: { correctness: 0.95, optimality: 0.7 }
}
```

### `@optimization_log`

**Syntax:** `@optimization_log { ... }` (list of versioned entries)
**Valid targets:** Function
**Meaning:** Records the history of optimization attempts on this function.
Each entry contains a version label, parameter values, measured metrics,
the agent name, target architecture, and date. See `spec/optimization.md`.
**Effect on compilation:** Informational. The optimization protocol reads
the log to avoid re-exploring parameter values that have already been tried.

---

## Custom Annotations

### `@<name>` (unrecognized names)

**Syntax:** `@name`, `@name(args)`, `@name { key: value }`
**Valid targets:** Function, Module, Param, StructDef, StructField, Block
**Meaning:** Any annotation name not in the built-in set is treated as a
custom annotation. Arguments are parsed as a list of `AnnotationValue`s or
a key-value map.
**Effect on compilation:** Preserved in the AST and HIR as
`Annotation::Custom(name, args)`. No built-in semantic meaning, but
available to AI agents and downstream tools.

---

## Annotation Value Types

Annotation arguments and key-value pairs use the following value types:

| Type | Syntax | Example |
|------|--------|---------|
| String | `"text"` | `"IEEE 754 compliant"` |
| Integer | `42`, `0xFF` | `64`, `256` |
| Float | `3.14` | `0.95` |
| Boolean | `true`, `false` | `true` |
| Identifier | bare word | `row_major` |
| List | `[v1, v2]` | `["q1", "q2"]` |
| Map | `{ k: v }` | `{ correctness: 0.95 }` |
