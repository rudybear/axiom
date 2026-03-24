# AXIOM

**AI eXchange Intermediate Optimization Medium**

A programming language designed as the canonical transfer format between AI agents, optimized for machine understanding and iterative optimization, that compiles to native code via LLVM.

> **This is NOT a language for humans to program in. This is a language for AI agents to communicate optimized computation through.**

## Why AXIOM Exists

Every existing language was designed for humans. AXIOM is designed for the gap between AI agents: when one AI generates code and another needs to optimize it, they need a format that preserves semantic intent, exposes optimization surfaces, and compiles to the fastest possible native code.

```axiom
@module matmul;
@intent("Dense matrix multiplication for compute benchmarking");
@constraint { correctness: "IEEE 754 compliant" };

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
    }

    // ... implementation ...
}
```

The `?params` are optimization holes that AI agents fill in, benchmark, and iterate on. The `@annotations` carry semantic intent through every compilation stage. No other language does this.

## Benchmark Results

**197 benchmarks** comparing AXIOM against C (clang -O2). Same LLVM backend, but AXIOM generates better-optimized IR.

### Real-World Benchmarks (20 programs)

| Benchmark | AXIOM | C -O2 | Winner |
|-----------|-------|-------|--------|
| Lattice Boltzmann fluid sim | 0.15s | 0.98s | **AXIOM 85% faster** |
| SHA-256 (native bitwise) | 0.07s | 0.15s | **AXIOM 52% faster** |
| Edge detection (Sobel) | 0.09s | 0.06s | C 43% faster |
| LRU cache | 0.07s | 0.08s | **AXIOM 21% faster** |
| Ray tracer | 0.09s | 0.08s | Tie |
| **Total (20 programs)** | **2.0s** | **2.8s** | **AXIOM 31% faster** |

### Memory Benchmarks (30 programs)

| Benchmark | AXIOM | C -O2 | Winner |
|-----------|-------|-------|--------|
| Binary trees (arena) | 0.18s | 0.92s | **AXIOM 80% faster** |
| Dijkstra shortest path | 0.06s | 0.11s | **AXIOM 45% faster** |
| Random alloc/free | 0.09s | 0.12s | **AXIOM 28% faster** |
| Sparse matrix (arena) | 0.06s | 0.08s | **AXIOM 23% faster** |

### How AXIOM Beats C

AXIOM has more information than C and uses it:

| Optimization | What AXIOM knows | What C doesn't | LLVM effect |
|---|---|---|---|
| `@pure` | Function has no side effects | Must assume side effects | `memory(none)`, `fast` math flags |
| `noalias` | No pointer aliasing (by design) | Must assume aliasing | Enables vectorization, reordering |
| `nsw` | No signed integer overflow | Must assume possible overflow | Strength reduction, loop opts |
| Arena allocator | Batch allocation lifetime | Per-object malloc/free | 50-200x allocation throughput |
| `@lifetime(scope)` | Heap can be stack | Must use heap | Zero-cost promotion |
| `fastcc` | Internal calling convention | C calling convention | Fewer register saves |

## Quick Start

```bash
# Build the compiler
cargo build --release

# Compile an AXIOM program
axiom compile examples/numerical/pi.axm -o pi
./pi

# See intermediate representations
axiom compile --emit=tokens examples/sort/bubble_sort.axm
axiom compile --emit=ast examples/sort/bubble_sort.axm
axiom compile --emit=hir examples/sort/bubble_sort.axm
axiom compile --emit=llvm-ir examples/sort/bubble_sort.axm

# Run optimization protocol
axiom optimize examples/matmul/matmul_simple.axm --iterations 5

# Benchmark a program
axiom bench examples/numerical/pi.axm --runs 10

# Start MCP server for AI agent integration
axiom mcp
```

**Requires:** Rust (latest stable), clang (for native binary compilation)

## Compilation Pipeline

```
AXIOM Source (.axm)
       |
       v
   LEXER (63 tests)         Tokens with spans
       |
       v
   PARSER (38 tests)        Typed AST with annotations
       |
       v
   HIR LOWERING (21 tests)  Validated annotations, type checking
       |
       v
   LLVM IR GEN (75 tests)   Optimized IR text with:
       |                     - noalias, nsw, fast-math
       |                     - fastcc, branch hints
       |                     - allocator attributes
       v
   CLANG -O2                 Native binary
```

## Language Features

### Types
```axiom
i8 i16 i32 i64 i128           // Signed integers
u8 u16 u32 u64 u128           // Unsigned integers
f16 bf16 f32 f64              // Floating point
bool                           // Boolean
array[T, N]                    // Fixed-size stack array
ptr[T]                         // Heap pointer
tensor[T, dims...]             // Tensor type (planned)
```

### Annotations
```axiom
@pure                          // No side effects → fast-math, noalias
@const                         // Compile-time evaluable
@inline(always | never | hint) // Inlining control
@complexity O(n^3)             // Algorithmic complexity
@intent("description")         // Semantic intent
@strategy { ... }              // Optimization surface with ?holes
@vectorizable(dims)            // Auto-vectorization hint
@lifetime(scope | static)      // Memory lifetime control
@export                        // C-compatible symbol
@transfer { ... }              // Inter-agent handoff metadata
```

### Memory Management
```axiom
// Stack arrays (zero-cost)
let arr: array[i32, 1000] = array_zeros[i32, 1000];

// Heap allocation
let data: ptr[i32] = heap_alloc(n, 4);
ptr_write_i32(data, i, value);
let val: i32 = ptr_read_i32(data, i);
heap_free(data);

// Arena allocation (50-200x faster than malloc)
let arena: ptr[i32] = arena_create(1048576);  // 1MB arena
let nodes: ptr[i32] = arena_alloc(arena, 10000, 4);
// ... use nodes ...
arena_reset(arena);   // Free ALL allocations instantly
arena_destroy(arena);
```

### Bitwise Operations
```axiom
band(a, b)     // AND        bor(a, b)      // OR
bxor(a, b)     // XOR        bnot(a)        // NOT
shl(a, n)      // Shift left  shr(a, n)      // Shift right
rotl(a, n)     // Rotate left rotr(a, n)     // Rotate right
```

### C Interop
```axiom
extern fn clock() -> i64;

@export
fn compute(data: ptr[f64], n: i32) -> f64 { ... }
```

## AI Agent Integration

### Optimization Protocol
```
1. EXTRACT   →  Discover ?holes and @strategy blocks
2. PROPOSE   →  Fill holes with concrete values
3. VALIDATE  →  Check types, ranges, constraints
4. BENCHMARK →  Compile, run, measure performance
5. RECORD    →  Store results in @optimization_log
```

### Agent Session API (Rust)
```rust
let session = AgentSession::from_file("matmul.axm")?;
let surfaces = session.surfaces();      // Discover optimization holes
session.apply_proposal(proposal, metrics, "agent-name")?;
let exported = session.export_with_transfer(transfer_info);
```

### MCP Server (for Claude, etc.)
```bash
axiom mcp  # Starts JSON-RPC server on stdio
```

Tools: `axiom_load`, `axiom_surfaces`, `axiom_propose`, `axiom_compile`, `axiom_history`

## Project Structure

```
axiom/
├── crates/
│   ├── axiom-lexer/        # Tokenizer (63 tests)
│   ├── axiom-parser/       # Recursive descent + Pratt (38 tests)
│   ├── axiom-hir/          # High-level IR + validation (21 tests)
│   ├── axiom-codegen/      # LLVM IR generation (75 tests)
│   ├── axiom-optimize/     # Optimization protocol + agent API
│   └── axiom-driver/       # CLI + MCP server + compilation
├── spec/                   # Formal language specification
│   ├── grammar.ebnf        # EBNF grammar
│   ├── types.md            # Type system
│   ├── annotations.md      # Annotation schema
│   ├── optimization.md     # Optimization protocol
│   └── transfer.md         # Inter-agent transfer protocol
├── benchmarks/
│   ├── suite/              # 115 simple benchmarks
│   ├── complex/            # 30 complex benchmarks
│   ├── real_world/         # 20 real-world benchmarks
│   ├── memory/             # 30 memory benchmarks
│   ├── fib/                # Recursive fibonacci (from drujensen/fib)
│   └── leibniz/            # Leibniz Pi (from niklas-heer/speed-comparison)
├── examples/               # Example AXIOM programs
│   ├── sort/               # Bubble, insertion, selection sort
│   ├── nbody/              # N-body gravitational simulation
│   ├── numerical/          # Pi, root finding, integration
│   ├── crypto/             # Caesar cipher
│   └── self_host/          # AXIOM lexer written in AXIOM
├── tests/samples/          # 14 test programs
└── docs/                   # Research documents
```

## Stats

- **21,846 lines of Rust** across 7 crates
- **357 tests** (all passing)
- **197 benchmarks** (100% compile rate)
- **70 git commits** across 6 development phases
- **14 sample programs**, **11 example programs**
- **5 formal specification documents**

## Development Pipeline

AXIOM was built using a multi-agent development pipeline with 5 independent agents:

| Agent | Role |
|-------|------|
| **Architect** | Designs specifications and acceptance criteria |
| **Coder** | Implements from spec |
| **Reviewer** | Adversarial code review |
| **Tester** | Runs tests, verifies criteria |
| **Benchmark** | Measures performance, detects regressions |

Each milestone goes through all 5 agents with git branch isolation and retry loops.

## License

MIT
