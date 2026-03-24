# AXIOM — AI eXchange Intermediate Optimization Medium

## Project Identity

AXIOM is a programming language designed as the canonical transfer format between AI agents, optimized for machine understanding and iterative optimization, that compiles to native code via LLVM IR. It is human-readable but AI-first: every construct carries semantic intent, explicit optimization surfaces, and structured metadata that AI agents can systematically explore and improve.

**This is NOT a language for humans to program in. This is a language for AI agents to communicate optimized computation through.**

**Repository:** https://github.com/rudybear/axiom

## Performance Goal

AXIOM must achieve top-tier scores on language comparison benchmarks. This is a hard requirement.

**Proven results (197 benchmarks):**
- Real-world benchmarks: **AXIOM 31% faster than C** (clang -O2) across 20 programs
- Binary trees (Benchmarks Game classic): **AXIOM 80% faster** with arena allocator
- Lattice Boltzmann fluid sim: **85% faster** | SHA-256: **52% faster**
- See `BENCHMARKS.md` for full results

**Rules:** No benchmark-specific cheating. General optimizations only. Reproducible results.

**Why AXIOM beats C:** `@pure` → fast-math + `memory(none)` | `noalias` on all pointer params (Fortran advantage) | `nsw` on integer arithmetic | Arena allocator (50-200x faster than malloc) | `@lifetime(scope)` heap-to-stack promotion | LLVM allocator attributes

---

## Architecture Overview

```
AXIOM Source (.axm)           ← AI agents read/write here
       │
       ▼
AXIOM Lexer (63 tests)        ← Tokenizer with error recovery
       │
       ▼
AXIOM Parser (38 tests)       ← Recursive descent + Pratt expressions
       │
       ▼
AXIOM HIR (24 tests)          ← Annotation validation, type checking
       │
       ▼
LLVM IR Text Gen (78 tests)   ← Optimized IR with noalias, nsw, fast-math,
       │                         fastcc, branch hints, allocator attributes
       ▼
clang -O2                     ← Native binary (x86_64, AArch64)
```

## Technology Stack

- **Compiler language**: Rust (21,846 lines, 7 crates)
- **Backend**: LLVM IR text generation → clang -O2 (no inkwell dependency)
- **Parser**: Hand-written recursive descent with Pratt parsing
- **Build system**: Cargo workspace
- **Testing**: 357 tests (unit + integration + doc-tests)
- **Benchmarks**: 197 programs (115 simple + 30 complex + 20 real-world + 30 memory + 2 GitHub repos)

## Current Feature Set

### Types
```
Primitives:    i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f16 bf16 f32 f64 bool
Arrays:        array[T, N]              // fixed-size, stack-allocated
Pointers:      ptr[T]                   // heap pointer
Tensors:       tensor[T, dims...]       // planned
Tuples:        (T1, T2, T3)
Functions:     fn(T1, T2) -> R
Sum types:     type Name = V1(T) | V2(T)  // parsed, codegen planned
Structs:       struct Name { field: Type } // parsed, codegen planned
```

### Annotations (all implemented)
```
@pure                          // No side effects → fast-math, memory(none), noalias
@const                         // Compile-time evaluable → speculatable
@inline(always | never | hint) // Inlining control
@complexity(expr)              // Algorithmic complexity class
@intent("description")         // Semantic intent
@constraint { key: value }     // Hard performance constraints
@strategy { ... }              // Optimization surface with ?holes
@vectorizable(dims)            // Loop vectorization hints
@parallel(dims)                // Parallelization hints
@target(device_class)          // Target hardware
@layout(row_major | col_major) // Memory layout
@align(bytes)                  // Alignment
@lifetime(scope | static)      // Lifetime control → heap-to-stack promotion
@export                        // C ABI export
@transfer { ... }              // Inter-agent handoff
@optimization_log { ... }      // Optimization history
```

### Memory Management
```
// Stack arrays (zero-cost)
let arr: array[i32, N] = array_zeros[i32, N];

// Heap allocation (malloc/free with LLVM allocator attributes)
let p: ptr[T] = heap_alloc(count, elem_size);
ptr_write_i32(p, index, value);
heap_free(p);

// Arena allocation (50-200x faster than malloc)
let arena: ptr[i32] = arena_create(size_bytes);
let data: ptr[i32] = arena_alloc(arena, count, elem_size);
arena_reset(arena);    // Free ALL allocations instantly
arena_destroy(arena);
```

### Bitwise Operations
```
band(a,b) bor(a,b) bxor(a,b) bnot(a)
shl(a,n) shr(a,n) lshr(a,n) rotl(a,n) rotr(a,n)
```

### Standard Library Builtins
```
// I/O
print(str) print_i32(n) print_i64(n) print_f64(x)

// Math
abs(x) abs_f64(x) min(a,b) max(a,b) min_f64(a,b) max_f64(a,b) sqrt(x) pow(x,y)

// Conversions
widen(i32→i64) narrow(i64→i32) truncate(f64→i32) to_f64(i32→f64) to_f64_i64(i64→f64)
```

### C Interop
```axiom
extern fn clock() -> i64;
@export fn compute(data: ptr[f64], n: i32) -> f64 { ... }
```

### AI Agent Integration
- **Optimization protocol**: extract surfaces → propose values → validate → benchmark → record
- **AgentSession API**: Rust API for AI agents to load/analyze/optimize AXIOM programs
- **MCP server**: 5 tools over JSON-RPC stdio (load, surfaces, propose, compile, history)
- **@transfer blocks**: Structured inter-agent handoff with confidence scores

## LLVM Optimizations Applied

| Optimization | LLVM Effect | Enabled By |
|---|---|---|
| `noalias` on all ptr params | Eliminates alias analysis overhead | No aliasing by design |
| `memory(none)` / `memory(argmem: read)` | CSE, hoisting, dead call elimination | `@pure` annotation |
| `nsw` on integer arithmetic | Strength reduction, loop opts | Defined semantics |
| `fast` math flags | FMA, reassociation, reciprocal | `@pure` on float functions |
| `fastcc` calling convention | Fewer register saves on internal functions | Non-exported functions |
| `speculatable` | Constant folding | `@const` annotation |
| `!prof` branch weights | Branch prediction hints | `@pure` base case detection |
| `!llvm.loop` vectorize hints | Auto-vectorization | `@vectorizable` annotation |
| `allockind` / `alloc-family` | Dead alloc elimination, heap-to-stack | All allocator calls |

---

## Repository Structure

```
axiom/
├── CLAUDE.md                    # This file — project context
├── README.md                    # GitHub README
├── BENCHMARKS.md                # Performance results
├── DESIGN.md                    # Living design document
├── Cargo.toml                   # Workspace root
├── crates/
│   ├── axiom-lexer/             # Tokenizer (63 tests)
│   ├── axiom-parser/            # Parser → AST (38 tests)
│   ├── axiom-hir/               # HIR + lowering (24 tests)
│   ├── axiom-mir/               # Mid-level IR (stub)
│   ├── axiom-codegen/           # LLVM IR generation (78 tests)
│   ├── axiom-optimize/          # Optimization protocol + agent API
│   └── axiom-driver/            # CLI + MCP server + compilation
├── spec/                        # Formal language specification
│   ├── grammar.ebnf
│   ├── types.md
│   ├── annotations.md
│   ├── optimization.md
│   └── transfer.md
├── benchmarks/
│   ├── suite/                   # 115 simple benchmarks
│   ├── complex/                 # 30 complex benchmarks
│   ├── real_world/              # 20 real-world benchmarks
│   ├── memory/                  # 30 memory benchmarks
│   ├── fib/                     # From drujensen/fib (908 stars)
│   └── leibniz/                 # From niklas-heer/speed-comparison
├── examples/                    # Example programs
│   ├── sort/                    # Bubble, insertion, selection sort
│   ├── nbody/                   # N-body gravitational simulation
│   ├── numerical/               # Pi, roots, integration
│   ├── crypto/                  # Caesar cipher
│   ├── multi_agent/             # Multi-agent handoff demo
│   └── self_host/               # AXIOM lexer written in AXIOM
├── tests/samples/               # 14 test programs
├── docs/                        # Research documents
│   ├── AXIOM_Language_Plan.md
│   ├── OPTIMIZATION_RESEARCH.md
│   ├── MEMORY_ALLOCATION_RESEARCH.md
│   └── GAME_ENGINE_RESEARCH.md
└── .pipeline/                   # Multi-agent development pipeline
```

---

## Completed Phases

### Phase 1 — Foundation ✅
Lexer → Parser → HIR → LLVM IR Codegen → E2E compilation to native binary.

### Phase 2 — AI Optimization Loop ✅
Optimization surface extraction, benchmarking harness, `axiom optimize` CLI, matmul demo.

### Phase 3 — Transfer Protocol ✅
`@transfer` blocks, AgentSession API, multi-agent handoff demo (Writer → Optimizer → GPU Specialist).

### Phase 4 — Ecosystem ✅
12 stdlib builtins, C FFI (`extern fn` + `@export`), MCP server (5 tools).

### Phase 5 — Self-Improvement ✅
Self-hosted AXIOM lexer subset written in AXIOM.

### Phase 6 — Memory Management ✅
Heap allocation (malloc/free), arena allocator (bump allocation), `@lifetime(scope)` heap-to-stack promotion, LLVM allocator attributes, 9 bitwise operators.

---

## Next Phase — Game Engine / Real-World Performance

**Vision:** Build a perfectly optimized game demo — zero per-frame allocations, parallel job system, efficient CPU/GPU synchronization via Vulkan.

### Planned features:
- **C ABI conformance** — `@repr(C)` struct layout, full calling convention support
- **Rust interop** — call Rust libraries via C ABI
- **Vulkan integration** — FFI to Vulkan API, GPU command buffer generation
- **SPIR-V codegen** — generate GPU shaders from AXIOM (via MLIR SPIR-V dialect)
- **Job system** — `@job` annotation for parallel task execution, work-stealing scheduler
- **SIMD intrinsics** — `@simd` annotation for explicit vectorization
- **SOA layout** — `@layout(soa)` for cache-friendly data-oriented design
- **I/O primitives** — file reading, memory-mapped I/O
- **Multithreading** — thread spawn/join, atomics, lock-free structures
- **Frame allocator** — ring-buffer arena for zero-allocation game loops
- **Hot reload** — recompile functions while program runs

See `docs/GAME_ENGINE_RESEARCH.md` for the full 2,014-line research document.

---

## Development Workflow

### Commit Conventions
```
type(scope): description
```
Types: `feat`, `fix`, `refactor`, `test`, `spec`, `docs`, `bench`, `chore`, `perf`

### Testing Strategy
- **Unit tests**: `#[cfg(test)]` modules in every crate
- **Integration tests**: Parse `.axm` files through the full pipeline
- **E2E tests**: Compile to binary and verify output
- **Benchmark tests**: Compare AXIOM vs C performance

### Multi-Agent Development Pipeline
The project uses a 5-agent pipeline (`.pipeline/`):
1. **Architect** — designs specifications
2. **Coder** — implements from spec
3. **Reviewer** — adversarial code review
4. **Tester** — runs tests, verifies criteria
5. **Benchmark** — measures performance

---

## Code Style & Patterns

### Rust Conventions
- `thiserror` for error types, `miette` for diagnostic display
- Every public API has doc comments
- `Spanned<T>` wrapper for AST/IR nodes with source location
- Newtype pattern for IDs: `struct NodeId(u32);`
- Minimal `pub` surface

### Anti-Patterns to Avoid
1. **No type inference.** Every type is explicit.
2. **No implicit returns.** Every function has `return`.
3. **No operator overloading.** `+` always means numeric add.
4. **No code without tests.** Every feature ships with tests.
5. **Annotations are first-class data, not decorations.**
6. **Parser must recover gracefully** — report ALL errors.
7. **No string types for structured data.**

---

## Quick Reference: Key Commands

```bash
# Build
cargo build --release

# Run tests
cargo test --workspace

# Compile AXIOM program
axiom compile program.axm -o output
axiom compile --emit=tokens|ast|hir|llvm-ir program.axm

# Optimization
axiom optimize program.axm --iterations 5
axiom bench program.axm --runs 10

# MCP server
axiom mcp

# Run benchmarks
python benchmarks/run_all.py --runs 3
```
