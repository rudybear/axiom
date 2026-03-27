# AXIOM Annotation Schema

Version 0.4 -- matches implemented parser and HIR as of 2026-03-25.
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

### `@parallel_for`

**Syntax:** `@parallel_for(shared_read: [vars], shared_write: [vars], reduction(op: var), private: [vars])`
**Valid targets:** Block (specifically, the block containing a `for` loop)
**Meaning:** Marks a for loop for parallel execution with explicit data sharing
clauses, following the OpenMP model. This is the safe, correct way to parallelize
loops in AXIOM -- it replaces the unsound combination of `@parallel` + `@pure`
on functions that write through pointers.

**Data sharing clauses:**
- `shared_read: [a, b]` -- Variables that are read (but never written) inside
  the loop body. The compiler ensures these are only loaded, never stored to.
  Enables `readonly` LLVM attributes.
- `shared_write: [out]` -- Variables that are written inside the loop body,
  but with disjoint index access across iterations (i.e., each iteration writes
  to a unique location). The compiler does NOT add `noalias` -- instead it
  relies on disjoint access patterns.
- `reduction(+: total)` -- Declares a reduction variable with an associative
  operator. The compiler generates thread-local accumulators initialized to the
  identity value (0 for `+`, 1 for `*`, INT_MAX for `min`, INT_MIN for `max`),
  accumulates locally per thread, then combines with an atomic operation at the
  end. This is the only correct way to update a shared scalar in a parallel loop.
- `private: [temp]` -- Variables that are private to each iteration (thread-local
  copy). Each thread gets its own independent copy; no synchronization needed.

**Effect on compilation:**
- Emits `fence release` before the parallel region and `fence acquire` after.
- Reduction variables use `atomicrmw add` (or equivalent) for the final combine.
- `!llvm.access.group` and `!llvm.loop.parallel_accesses` metadata on
  proven-parallel memory accesses.
- Does NOT blindly add `noalias` to shared pointers (this was the UB in the old design).

```axiom
@parallel_for(shared_read: [positions, masses], shared_write: [forces], reduction(+: total_energy), private: [dx, dy, dist])
for i: i32 in range(0, n) {
    // Each iteration reads positions/masses, writes forces[i], accumulates total_energy
    let dx: f64 = ptr_read_f64(positions, j * 2) - ptr_read_f64(positions, i * 2);
    let dy: f64 = ptr_read_f64(positions, j * 2 + 1) - ptr_read_f64(positions, i * 2 + 1);
    let dist: f64 = sqrt(dx * dx + dy * dy);
    total_energy = total_energy + compute_potential(masses, i, j, dist);
}
```

### `@lifetime`

**Syntax:** `@lifetime(scope)`, `@lifetime(static)`, `@lifetime(manual)`
**Valid targets:** Block, Function
**Meaning:** Declares the allocation lifetime for heap allocations within the
annotated scope. This enables the compiler to perform escape analysis and
heap-to-stack promotion.

- `scope` -- All heap allocations within this scope do not escape. The compiler
  may promote them to stack allocations (using `alloca` instead of `malloc`),
  eliminating heap overhead entirely. This is verified during compilation; if
  the pointer escapes (e.g., returned or stored to a global), the compiler
  emits an error.
- `static` -- Allocations live for the entire program lifetime. No deallocation
  is needed. The compiler may place them in static data sections.
- `manual` -- Explicit malloc/free semantics. No compiler optimization of
  allocation lifetime. This is the default if no `@lifetime` annotation is present.

**Effect on compilation:**
- `@lifetime(scope)` -> `alloca` instead of `malloc`, no `free` needed.
- `@lifetime(static)` -> global/static allocation.
- `@lifetime(manual)` -> standard `malloc`/`free`.

```axiom
@lifetime(scope)
{
    let buffer: ptr[i32] = heap_alloc(1024, 4);
    // buffer is promoted to stack -- no malloc, no free
    // ... use buffer ...
}  // buffer automatically freed (stack unwind)
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

## Verification Annotations

### `@strict`

**Syntax:** `@strict`
**Valid targets:** Module
**Meaning:** Enforces annotation completeness on all functions in the module.
When `@strict` is present, every function must have `@pure` (or explicitly not
pure), `@intent`, and `@complexity` annotations. Missing annotations produce
compile errors.
**Effect on compilation:** Checked during AST-to-HIR lowering. Functions
without required annotations are flagged as errors. This is the primary
mechanism for ensuring AI-generated code meets quality standards.

```axiom
@strict;

@pure
@intent("Square a number")
@complexity O(1)
fn square(x: i32) -> i32 {
    return x * x;
}
// ERROR if @intent or @complexity is missing
```

### `@precondition`

**Syntax:** `@precondition(expr)`
**Valid targets:** Function
**Meaning:** Declares a boolean expression that must be true when the function
is called. The expression can reference parameter names.
**Effect on compilation:**
- In `--debug` builds: emits a runtime check at function entry. If the
  condition is false, the program aborts with a diagnostic message including
  the source location and the failing expression.
- In release builds: no code is emitted (zero overhead).
- Used by `axiom test --fuzz` to generate valid test inputs.

```axiom
@precondition(n > 0)
@precondition(n <= 1000000)
fn count_primes(n: i32) -> i32 {
    // n is guaranteed > 0 and <= 1000000 in debug builds
    ...
}
```

### `@postcondition`

**Syntax:** `@postcondition(expr)`
**Valid targets:** Function
**Meaning:** Declares a boolean expression that must be true when the function
returns. The expression can reference parameter names and the special name
`result` to refer to the return value.
**Effect on compilation:**
- In `--debug` builds: emits a runtime check before each return statement.
  If the condition is false, the program aborts with a diagnostic message.
- In release builds: no code is emitted (zero overhead).

```axiom
@postcondition(result >= 0)
fn my_abs(x: i32) -> i32 {
    if x < 0 { return 0 - x; }
    return x;
}
```

### `@test`

**Syntax:** `@test { input: (arg1, arg2, ...), expect: value }`
**Valid targets:** Function
**Meaning:** Attaches an inline test case to a function. Multiple `@test`
annotations can be attached to the same function. Each test specifies input
arguments and the expected return value.
**Effect on compilation:** Test annotations are not included in normal
compilation. They are extracted and executed by the `axiom test` command,
which compiles test harnesses, runs them, and reports pass/fail results.
**Fuzzing:** When `axiom test --fuzz` is used, `@precondition` constraints
are analyzed to generate additional random test inputs within valid ranges.

```axiom
@test { input: (5), expect: 120 }
@test { input: (0), expect: 1 }
@test { input: (1), expect: 1 }
fn factorial(n: i32) -> i32 {
    if n <= 1 { return 1; }
    return n * factorial(n - 1);
}
```

### `@requires`

**Syntax:** `@requires(expr)`
**Valid targets:** Function
**Meaning:** Alias for `@precondition` that signals formal verification intent.
Semantically identical to `@precondition` -- emits a runtime check at function
entry in `--debug` builds, zero overhead in release. The name `@requires` is
preferred when the annotation participates in a formal contract specification
alongside `@ensures` and `@invariant`.
**Effect on compilation:** Same as `@precondition`.

```axiom
@requires(n > 0)
fn factorial(n: i32) -> i32 {
    if n <= 1 { return 1; }
    return n * factorial(n - 1);
}
```

### `@ensures`

**Syntax:** `@ensures(expr)`
**Valid targets:** Function
**Meaning:** Alias for `@postcondition` that signals formal verification intent.
Semantically identical to `@postcondition` -- emits a runtime check before each
return statement in `--debug` builds. The expression can reference parameter names
and the special name `result` to refer to the return value.
**Effect on compilation:** Same as `@postcondition`.

```axiom
@ensures(result >= 0)
fn my_abs(x: i32) -> i32 {
    if x < 0 { return 0 - x; }
    return x;
}
```

### `@invariant`

**Syntax:** `@invariant(expr)`
**Valid targets:** Block
**Meaning:** Declares a loop invariant -- a boolean expression that must hold at
the start and end of every loop iteration. In `--debug` builds, the compiler
inserts runtime checks at the loop header and before the back-edge. In release
builds, no code is emitted (zero overhead).
**Effect on compilation:** Runtime checks in debug mode only. Can be used by
future static analysis passes to prove loop correctness.

```axiom
@invariant(i >= 0 and i <= n)
for i in range(0, n) {
    // i is guaranteed in bounds
}
```

### `@trace`

**Syntax:** `@trace`
**Valid targets:** Function
**Meaning:** Instruments the function with ENTER/EXIT trace calls. When the
program is compiled with `--record`, these calls write structured events to a
`.trace.jsonl` file that can be replayed with `axiom replay`.
**Effect on compilation:** Emits `printf`-based trace calls at function entry
and before each return statement, including the function name and arguments.

```axiom
@trace
fn compute(x: i32) -> i32 {
    return x * x;
}
// At runtime prints: ENTER compute(42) / EXIT compute -> 1764
```

### `@link`

**Syntax:** `@link("library_name", "kind")`
**Valid targets:** Function
**Meaning:** Declares that the annotated `extern fn` requires linking against
the named native library. The `kind` parameter specifies the linking strategy:
`"static"` for static linking, `"dynamic"` for dynamic linking, `"framework"`
for macOS frameworks.
**Effect on compilation:** Passes `-l` flags to the linker via clang. Can also
be used with `axiom compile --link-dir` to specify library search paths.

```axiom
@link("m", "dynamic")
extern fn cbrt(x: f64) -> f64;

@link("mylib", "static")
extern fn my_native_func(x: i32) -> i32;
```

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
