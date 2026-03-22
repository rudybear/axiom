# AXIOM — AI eXchange Intermediate Optimization Medium

## Project Identity

AXIOM is a new programming language designed as the canonical transfer format between AI agents, optimized for machine understanding and iterative optimization, that compiles to native code via MLIR → LLVM IR. It is human-readable but AI-first: every construct carries semantic intent, explicit optimization surfaces, and structured metadata that AI agents can systematically explore and improve.

**This is NOT a language for humans to program in. This is a language for AI agents to communicate optimized computation through.**

## Performance Goal

AXIOM must achieve top-tier (high or highest) scores on well-known language comparison benchmarks. This is a hard project requirement, not aspirational.

**Target benchmarks include (but are not limited to):**
- [The Computer Language Benchmarks Game](https://benchmarksgame-team.pages.debian.net/benchmarksgame/) — the classic cross-language shootout (n-body, spectral-norm, mandelbrot, fasta, binary-trees, etc.)
- [Programming Language and Compiler Benchmarks](https://programming-language-benchmarks.vercel.app/) — broader multi-language comparison
- Any other widely-recognized, reproducible language performance benchmarks

**Rules:**
1. **No benchmark-specific cheating.** The compiler and runtime must be general-purpose. It is strictly forbidden to detect benchmark programs and apply special-case optimizations that would not benefit arbitrary user code.
2. **No hard-coded results.** The compiler must not precompute or embed benchmark answers.
3. **General optimizations only.** Every optimization that helps a benchmark must be a general optimization pass available to all AXIOM programs. If tiling helps matmul in a benchmark, it must help any matmul written in AXIOM.
4. **Idiomatic AXIOM solutions.** Benchmark implementations should use natural AXIOM constructs (annotations, optimization holes, strategy blocks) — not contorted workarounds. The benchmarks serve as proof that AXIOM's design enables performance, not that someone gamed the scoring.
5. **Reproducible results.** All benchmark runs must be reproducible with documented hardware, OS, compiler version, and flags.

**Why this matters:** AXIOM's entire thesis is that AI agents can produce better-optimized code through explicit optimization surfaces. If AXIOM can't compete with C/C++/Rust on standard benchmarks using its own optimization protocol, the language has failed its core mission.

---

## Architecture Overview

```
AXIOM Source (.axm)           ← AI agents read/write here
       │
       ▼
AXIOM HIR (High-level IR)     ← Semantic intent preserved, optimization holes visible
       │
       ▼
AXIOM MIR (Mid-level IR)      ← Lowered control flow, typed SSA operations
       │
       ▼
MLIR (custom axiom dialect)   ← Leverage existing tensor/gpu/async dialects
       │
       ▼
LLVM IR                       ← Standard LLVM optimization passes
       │
       ▼
Native binary                 ← x86_64, AArch64, RISC-V
```

## Technology Stack

- **Compiler language**: Rust
- **MLIR bindings**: `melior` crate (safe Rust MLIR bindings)
- **LLVM bindings**: `inkwell` crate (fallback for direct LLVM access)
- **Parser**: Hand-written recursive descent (not parser combinator — we need precise error recovery and span tracking for AI feedback)
- **Build system**: Cargo workspace
- **Testing**: Rust's built-in test framework + FileCheck-style lit tests for IR
- **Benchmarking**: Built-in harness using `criterion` patterns, plus `perf`/`valgrind --tool=callgrind` for instruction counting

## Repository Structure

```
axiom/
├── CLAUDE.md                    # This file — orchestration prompt
├── DESIGN.md                    # Language specification (living document)
├── Cargo.toml                   # Workspace root
├── crates/
│   ├── axiom-lexer/             # Tokenizer
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── token.rs         # Token types
│   │       └── lexer.rs         # Lexer implementation
│   ├── axiom-parser/            # Parser → AST
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── ast.rs           # AST node definitions
│   │       ├── parser.rs        # Recursive descent parser
│   │       └── error.rs         # Parse error types with spans
│   ├── axiom-hir/               # High-level IR
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── hir.rs           # HIR node definitions
│   │       ├── lower.rs         # AST → HIR lowering
│   │       ├── annotations.rs   # @annotation processing
│   │       └── optimize.rs      # HIR-level optimization surfaces
│   ├── axiom-mir/               # Mid-level IR (SSA form)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── mir.rs           # MIR definitions (SSA)
│   │       └── lower.rs         # HIR → MIR lowering
│   ├── axiom-codegen/           # MLIR/LLVM code generation
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── mlir.rs          # MIR → MLIR lowering
│   │       └── llvm.rs          # Direct LLVM fallback
│   ├── axiom-driver/            # CLI frontend (`axiom` binary)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs
│   └── axiom-optimize/          # AI optimization protocol
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── surface.rs       # Optimization surface extraction
│           ├── proposal.rs      # Optimization proposal types
│           ├── benchmark.rs     # Built-in benchmarking harness
│           └── history.rs       # Optimization history read/write
├── tests/
│   ├── samples/                 # .axm sample programs
│   │   ├── hello.axm
│   │   ├── matmul_naive.axm
│   │   ├── matmul_optimized.axm
│   │   └── fibonacci.axm
│   └── lit/                     # FileCheck-style IR tests
│       ├── parse/
│       ├── hir/
│       ├── mir/
│       └── codegen/
├── spec/                        # Formal language specification
│   ├── grammar.ebnf             # EBNF grammar
│   ├── types.md                 # Type system specification
│   ├── annotations.md           # Annotation schema
│   ├── optimization.md          # Optimization protocol spec
│   └── transfer.md              # Inter-agent transfer protocol spec
└── examples/
    ├── matmul/                  # Matrix multiply optimization demo
    ├── sort/                    # Sorting algorithm optimization
    └── nbody/                   # N-body simulation (compute-heavy)
```

---

## Language Specification Summary

### Core Syntax Rules

1. **Every type is explicit.** No type inference. AI agents never guess.
2. **Every annotation is structured.** `@name(args)` or `@name { key: value }` — never free-form.
3. **Optimization holes use `?`prefix.** `?tile_size`, `?loop_order` — these are what AI fills in.
4. **No implicit conversions.** `widen(x)`, `narrow(x)`, `truncate(x)` — named and explicit.
5. **No operator overloading.** `+` always means numeric add. `tensor.add(a, b)` for tensors.
6. **Words over symbols for logic.** `and`, `or`, `not` — not `&&`, `||`, `!`.
7. **Explicit returns.** Every function body ends with `return expr`. No implicit last-expression.
8. **Semicolons are required.** No ASI ambiguity. Every statement ends with `;`.
9. **Braces for blocks.** No significant whitespace. Unambiguous parse from any starting point.
10. **UTF-8 source.** Identifiers are ASCII alphanumeric + underscore. Strings are UTF-8.

### Type System

```
Primitives:    i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f16 bf16 f32 f64 bool
Tensors:       tensor[element_type, dim0, dim1, ...]    // dims can be ? for dynamic
Arrays:        array[element_type, length]               // fixed-size, stack-allocated
Slices:        slice[element_type]                       // fat pointer (ptr + len)
Pointers:      ptr[element_type]                         // raw pointer (unsafe blocks only)
Tuples:        (T1, T2, T3)
Functions:     fn(T1, T2) -> R
Sum types:     type Name = Variant1(T) | Variant2(T)
Structs:       struct Name { field: Type, ... }
```

### Annotation Schema

```
// Semantic annotations (preserved through all IR levels)
@pure                                   // No side effects
@const                                  // Compile-time evaluable
@inline(always | never | hint)          // Inlining guidance
@complexity(expr)                       // Algorithmic complexity class
@intent("description")                  // What this code does semantically
@constraint { key: value, ... }         // Hard performance/correctness constraints

// Optimization annotations (consumed by the optimization protocol)
@strategy { ... }                       // Optimization surface declaration
@vectorizable(dims)                     // Which dimensions can be vectorized
@parallel(dims)                         // Which dimensions can be parallelized
@target(device_class)                   // Target hardware class

// Transfer annotations (inter-agent communication)
@transfer { ... }                       // Agent handoff metadata
@optimization_log { ... }              // History of optimization attempts

// Layout annotations (memory)
@layout(row_major | col_major | custom) // Memory layout
@align(bytes)                           // Alignment requirement
@arena(name)                            // Allocation arena
@lifetime(scope | static | manual)      // Lifetime strategy
```

### Optimization Hole Syntax

```axiom
@strategy {
    tiling:   { M: ?tile_m, N: ?tile_n, K: ?tile_k }
    order:    ?loop_order          // type: array[ident]
    parallel: ?parallel_dims       // type: array[ident]
    unroll:   ?unroll_factor       // type: u32, range: 1..32
    prefetch: ?prefetch_distance   // type: u32, range: 0..16
}
```

The `?name` holes have optional type constraints and value ranges. The compiler validates that any proposed value is within range. The optimization protocol systematically explores this space.

---

## Development Workflow

### Session Start Protocol

Every coding session begins with:

1. Read `CLAUDE.md` (this file) to load full project context
2. Read `DESIGN.md` for current language spec state
3. Check `git log --oneline -20` for recent changes
4. Run `cargo test` to verify baseline is green
5. Check for any `TODO(next)` markers in the codebase

### Commit Conventions

```
feat(lexer): add tensor type token support
fix(parser): handle nested annotation blocks
refactor(hir): split annotation processing into own module
test(codegen): add matmul LLVM IR output test
spec(types): define tensor dimension constraints
docs(DESIGN): update optimization hole syntax
bench(matmul): add baseline naive implementation benchmark
```

Format: `type(scope): description`
Types: `feat`, `fix`, `refactor`, `test`, `spec`, `docs`, `bench`, `chore`

### Testing Strategy

**Unit tests**: Every crate has `#[cfg(test)]` modules. Test the happy path AND error cases.

**Integration tests**: In `tests/` directory. Parse `.axm` files, lower through the pipeline, verify output.

**Snapshot tests**: For IR output. Use `insta` crate for snapshot testing — when IR changes, we review diffs explicitly.

**Lit tests**: FileCheck-style tests for IR verification:
```
// RUN: axiom compile --emit=hir %s | FileCheck %s
// CHECK: @pure
// CHECK: fn matmul
// CHECK: @strategy
```

**Benchmarks**: In `examples/` directory. Each example has a `bench.axm` that the optimization protocol can target.

### Build Dependencies

The project requires:
- Rust stable (latest)
- LLVM 18+ (for MLIR/LLVM libraries)
- `mlir-opt` and `llc` on PATH
- CMake (for LLVM build if from source)

On Ubuntu/Debian:
```bash
# LLVM and MLIR
wget https://apt.llvm.org/llvm.sh && chmod +x llvm.sh && sudo ./llvm.sh 18
sudo apt install libmlir-18-dev mlir-18-tools

# Or build from source for latest MLIR features
git clone https://github.com/llvm/llvm-project.git
cmake -S llvm-project/llvm -B build -G Ninja \
  -DLLVM_ENABLE_PROJECTS="mlir" \
  -DLLVM_TARGETS_TO_BUILD="host" \
  -DCMAKE_BUILD_TYPE=Release
ninja -C build
```

---

## Phase 1 Milestones (Current Phase)

### Milestone 1.1: Lexer ✅ → 🔲
- [ ] Define all token types in `token.rs`
- [ ] Implement lexer that handles: keywords, identifiers, numbers (int/float with width suffixes), strings, operators, annotations (`@name`), optimization holes (`?name`), brackets/braces/parens, comments (`//` and `/* */`)
- [ ] Error recovery: on invalid token, record error and skip to next valid token start
- [ ] Span tracking: every token carries `(start_offset, end_offset, line, col)`
- [ ] Unit tests for every token type + edge cases

### Milestone 1.2: Parser ✅ → 🔲
- [ ] Define AST types in `ast.rs` covering: modules, functions, structs, type aliases, let bindings, assignments, if/else, for loops, while loops, return, expressions (binary, unary, call, index, field access, method call), tensor literals, annotations, strategy blocks, optimization holes
- [ ] Recursive descent parser with Pratt parsing for expressions
- [ ] Annotation parsing: `@name`, `@name(args)`, `@name { key: value }`
- [ ] Strategy block parsing with `?param` holes
- [ ] Error recovery: skip to next statement on parse error, collect all errors
- [ ] Pretty-printer: AST → AXIOM source (roundtrip fidelity)

### Milestone 1.3: HIR ✅ → 🔲
- [ ] HIR node types that preserve all semantic annotations
- [ ] AST → HIR lowering: desugar syntactic sugar, resolve basic names
- [ ] Annotation validation: check that annotations reference valid targets
- [ ] Type checking: explicit types mean this is mostly validation, not inference
- [ ] HIR pretty-printer

### Milestone 1.4: Codegen ✅ → 🔲
- [ ] HIR → LLVM IR for a minimal subset: functions, i32/i64/f32/f64, arithmetic, if/else, for loops, return
- [ ] Use `inkwell` for initial LLVM IR generation (simpler than full MLIR setup)
- [ ] Compile and link to produce executable binary
- [ ] Test: `fibonacci.axm` compiles and produces correct output
- [ ] Test: `hello.axm` prints to stdout

### Milestone 1.5: End-to-End Demo ✅ → 🔲
- [ ] `axiom compile examples/fibonacci.axm -o fib && ./fib` works
- [ ] `axiom compile --emit=hir examples/fibonacci.axm` prints HIR
- [ ] `axiom compile --emit=llvm-ir examples/fibonacci.axm` prints LLVM IR
- [ ] README with build instructions and example

---

## Code Style & Patterns

### Rust Conventions
- Use `thiserror` for error types, `miette` for diagnostic display
- Every public API has doc comments with examples
- Prefer `&str` over `String` in parser internals (arena allocation later)
- Use `newtype` pattern for IDs: `struct FuncId(u32);`
- Keep crate APIs minimal — `pub` only what's needed cross-crate

### AST/IR Node Pattern
```rust
/// Every node carries a span and optional annotations
#[derive(Debug, Clone)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

/// Annotations are first-class, typed data — not strings
#[derive(Debug, Clone)]
pub enum Annotation {
    Pure,
    Const,
    Inline(InlineHint),
    Complexity(ComplexityExpr),
    Intent(String),
    Constraint(BTreeMap<String, ConstraintValue>),
    Strategy(StrategyBlock),
    Transfer(TransferBlock),
    // ...
}
```

### Error Pattern
```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ParseError {
    #[error("expected {expected}, found {found}")]
    #[diagnostic(code(axiom::parse::unexpected_token))]
    UnexpectedToken {
        expected: String,
        found: String,
        #[label("here")]
        span: SourceSpan,
    },
    // ...
}
```

---

## Critical Design Decisions Log

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Compiler language | Rust | Memory safety, LLVM/MLIR bindings, AI writes it well |
| Parser strategy | Hand-written recursive descent | Better error recovery, precise spans, no parser generator dependency |
| Initial backend | inkwell (LLVM) | Simpler bootstrap; MLIR migration in Phase 2-3 |
| Type inference | None | Explicit types = zero ambiguity for AI agents |
| Semicolons | Required | Eliminates ASI ambiguity entirely |
| Significant whitespace | No | Braces are unambiguous from any parse position |
| Operator overloading | None | `+` always means the same thing |
| Optimization holes | `?name` syntax | Distinct from all other syntax; impossible to confuse |
| Annotation syntax | `@name(...)` / `@name { ... }` | Structured, machine-parseable, validatable |
| Source format | Single file per module | Simple mental model for AI agents |

---

## Anti-Patterns to Avoid

1. **Don't add type inference.** Ever. Not even "obvious" cases. The whole point is explicitness.
2. **Don't use implicit returns.** Every function has `return`. Period.
3. **Don't add operator overloading.** Named methods for non-primitive operations.
4. **Don't generate code without tests.** Every feature ships with unit tests.
5. **Don't skip annotations in the AST.** They are first-class data, not decorations.
6. **Don't optimize prematurely.** Get correctness first. The language is about *enabling* optimization, not being fast to compile.
7. **Don't make the parser error-intolerant.** AI agents will generate broken code. The parser must recover gracefully and report ALL errors, not just the first one.
8. **Don't use string types for structured data.** Annotations, types, constraints — all have proper Rust types.

---

## Session Handoff Template

When ending a session, leave a `SESSION_STATE.md` file:

```markdown
# Session State — [DATE]

## Completed This Session
- [what was done]

## Current State
- All tests passing: yes/no
- Crates building: yes/no
- Blocking issues: [any]

## Next Steps (in priority order)
1. [highest priority next task]
2. [second priority]
3. [third priority]

## Open Questions
- [any design questions that need resolution]

## Files Modified
- [list of files changed]
```

---

## Quick Reference: Example AXIOM Programs

### Hello World
```axiom
@module hello;
@intent("Print greeting to stdout");

fn main() -> i32 {
    print("Hello from AXIOM!");
    return 0;
}
```

### Fibonacci
```axiom
@module fibonacci;
@intent("Compute Nth Fibonacci number iteratively");

@pure
@complexity O(n)
fn fib(n: i32) -> i64 {
    if n <= 1 {
        return widen(n);
    }
    let a: i64 = 0;
    let b: i64 = 1;
    for i: i32 in range(2, n + 1) {
        let temp: i64 = b;
        b = a + b;
        a = temp;
    }
    return b;
}

fn main() -> i32 {
    let result: i64 = fib(40);
    print_i64(result);
    return 0;
}
```

### Matrix Multiply (with optimization surfaces)
```axiom
@module matmul;
@intent("Dense matrix multiplication for compute benchmarking");
@constraint { correctness: "IEEE 754 compliant" };
@target { cpu.simd, gpu.compute };

@pure
@complexity O(n^3)
@vectorizable(i, j, k)
fn matmul(
    a: tensor[f32, M, K] @layout(row_major) @align(64),
    b: tensor[f32, K, N] @layout(col_major) @align(64),
) -> tensor[f32, M, N] @layout(row_major) {

    @strategy {
        tiling:   { M: ?tile_m, N: ?tile_n, K: ?tile_k }
        order:    ?loop_order
        parallel: ?parallel_dims
        unroll:   ?unroll_factor
        prefetch: ?prefetch_distance
    }

    let result: tensor[f32, M, N] = tensor.zeros[f32, M, N];

    for i: i32 in range(M) {
        for j: i32 in range(N) {
            let acc: f32 = 0.0;
            for k: i32 in range(K) {
                acc = acc + a[i, k] * b[k, j];
            }
            result[i, j] = acc;
        }
    }

    return result;
}
```
