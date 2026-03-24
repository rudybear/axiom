# AXIOM

**AI eXchange Intermediate Optimization Medium**

A programming language designed as the canonical transfer format between AI agents, optimized for machine understanding and iterative optimization, that compiles to native code via LLVM.

> **This is NOT a language for humans to program in. This is a language for AI agents to communicate optimized computation through.**

> **AXIOM beats C (-O3 -march=native -ffast-math) by 3% overall across 20 real-world benchmarks.** 120 commits. 30,124 LOC. 450 tests. 197 benchmarks.

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

**AXIOM beats C (-O3 -march=native -ffast-math) by 3% overall across 20 real-world benchmarks.**

**197 benchmarks** comparing AXIOM against C turbo (clang -O3 -march=native -ffast-math). Same LLVM backend, but AXIOM generates better-optimized IR.

### Real-World Benchmarks (20 programs) -- vs C Turbo

| Benchmark | AXIOM | C Turbo | Winner |
|-----------|-------|---------|--------|
| JPEG DCT | -- | -- | **AXIOM 56% faster** |
| RLE compression | -- | -- | **AXIOM 16% faster** |
| ... | ... | ... | ... |
| **Total (20 programs)** | **0.97x** | **1.00x** | **AXIOM 3% faster (2 wins, 9 ties, 9 C wins)** |

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
| `fence` | Release/acquire semantics | No memory model | Correct concurrency |
| `readonly`/`writeonly` | Pointer access direction | Must assume read+write | Alias analysis, dead store elim |
| `calloc` for zeroed alloc | Zero-init via OS page trick | `malloc` + `memset` | Kernel-level zero pages, skips user-space memset |
| `@inline(always)` | Force-inline hot paths | Heuristic-only inlining | `alwaysinline` attribute, eliminates call overhead |

### Optimization Knowledge Base

AXIOM maintains an Optimization Knowledge Base that grows with each LLM optimization session: **10 rules + 5 anti-patterns** discovered so far. Rules capture what works (e.g., "arena allocators beat malloc by 50-200x for tree structures"), anti-patterns capture what doesn't (e.g., "marking I/O functions as @pure breaks correctness"). The knowledge base is fed into future LLM prompts, so the compiler gets smarter over time.

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

# Target a specific CPU architecture
axiom compile --target=x86-64-v4 program.axm -o program

# Run optimization protocol
axiom optimize examples/matmul/matmul_simple.axm --iterations 5

# Benchmark a program
axiom bench examples/numerical/pi.axm --runs 10

# Profile a program (compile + time + surface extraction + suggestions)
axiom profile program.axm --iterations 10

# Format an AXIOM source file (parse -> HIR -> pretty-print)
axiom fmt program.axm

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
   PARSER (50 tests)        Typed AST with annotations
       |
       v
   HIR LOWERING (25 tests)  Validated annotations, type checking
       |
       v
   LLVM IR GEN (128 tests)  Optimized IR text with:
       |                     - noalias, nsw, fast-math
       |                     - fastcc, branch hints
       |                     - allocator attributes
       |                     - fence release/acquire
       |                     - readonly/writeonly pointer attrs
       |                     - DWARF debug metadata
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
readonly_ptr[T]                // Read-only pointer
writeonly_ptr[T]               // Write-only pointer
slice[T]                       // Fat pointer (ptr + length)
tensor[T, dims...]             // Tensor type (planned)
(T1, T2, T3)                  // Tuple
fn(T1, T2) -> R               // Function type
```

### Annotations
```axiom
@pure                          // No side effects -> fast-math, noalias
@const                         // Compile-time evaluable
@inline(always | never | hint) // Inlining control
@complexity O(n^3)             // Algorithmic complexity
@intent("description")         // Semantic intent
@strategy { ... }              // Optimization surface with ?holes
@constraint { key: value }     // Hard performance constraints
@vectorizable(dims)            // Auto-vectorization hint
@parallel(dims)                // Parallelization hints
@parallel_for(shared_read: [...], shared_write: [...], reduction(+: var), private: [...])
                               // Data-parallel for loop with sharing clauses
@lifetime(scope | static | manual)  // Memory lifetime control
@layout(row_major | col_major) // Memory layout
@align(bytes)                  // Alignment
@target(device_class)          // Target hardware
@export                        // C-compatible symbol
@transfer { ... }              // Inter-agent handoff metadata
@optimization_log { ... }      // Optimization history
```

### Memory Management
```axiom
// Stack arrays (zero-cost)
let arr: array[i32, 1000] = array_zeros[i32, 1000];

// Heap allocation
let data: ptr[i32] = heap_alloc(n, 4);
let data_z: ptr[i32] = heap_alloc_zeroed(n, 4);
let data2: ptr[i32] = heap_realloc(data, new_n, 4);
ptr_write_i32(data, i, value);
let val: i32 = ptr_read_i32(data, i);
heap_free(data);

// Arena allocation (50-200x faster than malloc)
let arena: ptr[i32] = arena_create(1048576);  // 1MB arena
let nodes: ptr[i32] = arena_alloc(arena, 10000, 4);
// ... use nodes ...
arena_reset(arena);   // Free ALL allocations instantly
arena_destroy(arena);

// Dynamic arrays (vec)
let v: ptr[i32] = vec_new(4);   // elem_size = 4
vec_push_i32(v, 42);
let x: i32 = vec_get_i32(v, 0);
vec_set_i32(v, 0, 99);
let n: i32 = vec_len(v);
vec_free(v);

// Option (tagged union packed into i64)
let none_val: i64 = option_none();
let some_val: i64 = option_some(42);
let is_some: i32 = option_is_some(some_val);
let inner: i32 = option_unwrap(some_val);

// Result (error handling, tagged union packed into i64)
let ok_val: i64 = result_ok(42);
let err_val: i64 = result_err(1);
let is_ok: i32 = result_is_ok(ok_val);
let value: i32 = result_unwrap(ok_val);
let code: i32 = result_err_code(err_val);

// Strings (fat pointer: ptr + len)
let s: ptr[i32] = string_from_literal("hello");
let len: i32 = string_len(s);
let eq: i32 = string_eq(s1, s2);
string_print(s);
```

### Bitwise Operations
```axiom
band(a, b)     // AND        bor(a, b)      // OR
bxor(a, b)     // XOR        bnot(a)        // NOT
shl(a, n)      // Shift left  shr(a, n)      // Arithmetic shift right
lshr(a, n)     // Logical shift right
rotl(a, n)     // Rotate left rotr(a, n)     // Rotate right
```

### Math
```axiom
abs(x)         // Integer absolute value
abs_f64(x)     // Float absolute value
min(a, b)      // Integer min       max(a, b)      // Integer max
min_f64(a, b)  // Float min         max_f64(a, b)  // Float max
sqrt(x)        // Square root       pow(x, y)      // Power
to_f64(x)      // i32 -> f64        to_f64_i64(x)  // i64 -> f64
widen(x)       // Widen type         narrow(x)      // Narrow type
truncate(x)    // Float -> integer truncation
```

### Concurrency
```axiom
// Threads
let tid: i32 = thread_create(func, arg);
thread_join(tid);

// Atomics
let val: i32 = atomic_load(ptr);
atomic_store(ptr, val);
let old: i32 = atomic_add(ptr, delta);
let old: i32 = atomic_cas(ptr, expected, desired);

// Mutex
let mtx: ptr[i32] = mutex_create();
mutex_lock(mtx);
mutex_unlock(mtx);
mutex_destroy(mtx);

// Job system (thread pool)
jobs_init(num_cores());
job_dispatch(func, data, total_items);
job_wait();
let handle: i32 = job_dispatch_handle(func, data, total_items);
let handle2: i32 = job_dispatch_after(func, data, total_items, handle);
job_wait_handle(handle2);
jobs_shutdown();

// Coroutines (stackful, via OS fibers/ucontext)
let coro: i32 = coro_create(func, arg);
let val: i32 = coro_resume(coro);
coro_yield(value);
let done: i32 = coro_is_done(coro);
coro_destroy(coro);
```

### I/O and System
```axiom
print("hello");               // Print string
print_i32(42);                // Print i32
print_i64(100);               // Print i64
print_f64(3.14);              // Print f64
file_read(path)               // Read entire file
file_write(path, data, len)   // Write bytes to file
file_size(path)               // Get file size
clock_ns()                    // Nanosecond wall clock
get_argc()                    // Argument count
get_argv(i)                   // Argument string
cpu_features()                // CPUID feature bitmask
```

### Function Pointers
```axiom
let fp: ptr[i32] = fn_ptr(my_function);
let result: i32 = call_fn_ptr_i32(fp, arg);
let result: f64 = call_fn_ptr_f64(fp, arg);
```

### Renderer (Stub/Vulkan FFI)
```axiom
renderer_create(width, height, title)
renderer_destroy(r)
renderer_begin_frame(r)
renderer_end_frame(r)
renderer_should_close(r)
renderer_clear(r, r, g, b)
renderer_draw_triangles(r, verts, count)
renderer_draw_points(r, data, count)
renderer_get_time(r)
shader_load(r, path)
pipeline_create(r, vert_shader, frag_shader)
renderer_bind_pipeline(r, pipeline)
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
1. EXTRACT   ->  Discover ?holes and @strategy blocks
2. PROPOSE   ->  Fill holes with concrete values
3. VALIDATE  ->  Check types, ranges, constraints
4. BENCHMARK ->  Compile, run, measure performance
5. RECORD    ->  Store results in @optimization_log
```

### LLM Self-Optimization Pipeline (The Core Differentiator)

The `axiom optimize` command feeds source + LLVM IR + assembly + benchmark data to an LLM, which analyzes the generated code and suggests improvements. The LLM prompt includes `@constraint` annotations (e.g., `optimize_for: "performance"` vs `"memory"` vs `"latency"`) to steer the optimization direction.

```bash
# Dry run -- see the prompt the LLM would receive
axiom optimize program.axm --dry-run

# Full optimization loop with Claude API
ANTHROPIC_API_KEY=sk-... axiom optimize program.axm --iterations 5

# Profile a program (compile + time + surface extraction)
axiom profile program.axm --iterations 10
```

**Demonstrated result:** The LLM analyzed the assembly output of a prime-counting program, identified a `divl` bottleneck (~25 cycles per integer division), and suggested wheel factorization (6k+-1). Result: **37% speedup**, identical output, verified against C.

```
v1 (naive):  18.7ms  ->  v2 (LLM-optimized):  13.6ms  =  1.37x faster
Both: AXIOM matches C exactly (1.00x on both algorithms)
```

The optimization loop:
1. Compile -> LLVM IR + assembly
2. Benchmark -> timing data
3. Build prompt (source + IR + asm + timing + ?params + history + constraints)
4. LLM analyzes, suggests ?param values and code changes
5. Apply, recompile, re-benchmark, record in @optimization_log
6. Repeat -- LLM sees history of what worked and what didn't

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
├── .github/
│   └── workflows/
│       └── ci.yml              # GitHub Actions CI pipeline
├── crates/
│   ├── axiom-lexer/            # Tokenizer (63 tests)
│   ├── axiom-parser/           # Recursive descent + Pratt (50 tests)
│   ├── axiom-hir/              # High-level IR + validation (25 tests)
│   ├── axiom-codegen/          # LLVM IR generation (128 tests)
│   ├── axiom-optimize/         # Optimization protocol + agent API (115 tests)
│   ├── axiom-mir/              # Mid-level IR (stub)
│   └── axiom-driver/           # CLI + MCP server + compilation (57 tests)
│       └── runtime/
│           └── axiom_rt.c      # C runtime (I/O, coroutines, threads, jobs, renderer)
├── spec/                       # Formal language specification
│   ├── grammar.ebnf            # EBNF grammar
│   ├── types.md                # Type system
│   ├── annotations.md          # Annotation schema
│   ├── optimization.md         # Optimization protocol
│   └── transfer.md             # Inter-agent transfer protocol
├── benchmarks/
│   ├── suite/                  # 115 simple benchmarks
│   ├── complex/                # 30 complex benchmarks
│   ├── real_world/             # 20 real-world benchmarks
│   ├── memory/                 # 30 memory benchmarks
│   ├── fib/                    # Recursive fibonacci (from drujensen/fib)
│   └── leibniz/                # Leibniz Pi (from niklas-heer/speed-comparison)
├── examples/                   # 20 example AXIOM programs
│   ├── sort/                   # Bubble, insertion, selection sort
│   ├── nbody/                  # N-body gravitational simulation
│   ├── numerical/              # Pi, root finding, integration
│   ├── crypto/                 # Caesar cipher
│   ├── matmul/                 # Matrix multiplication demos
│   ├── ecs/                    # Entity-Component-System game demo
│   ├── vulkan/                 # Triangle rendering (stub -> Vulkan planned)
│   ├── particle_galaxy/        # 10K particle galaxy (windowed renderer)
│   ├── game_loop/              # Frame allocator, zero per-frame allocs
│   ├── self_opt/               # LLM optimization demos (primes, matmul)
│   ├── multi_agent/            # Multi-agent handoff demo
│   └── self_host/              # AXIOM lexer written in AXIOM
├── tests/samples/              # 24 test programs
├── docs/                       # Research documents
│   ├── MASTER_TASK_LIST.md     # 47-milestone task tracker
│   ├── OPTIMIZATION_RESEARCH.md
│   ├── MEMORY_ALLOCATION_RESEARCH.md
│   ├── GAME_ENGINE_RESEARCH.md
│   ├── MULTITHREADING_ANALYSIS.md
│   ├── LUX_INTEGRATION_RESEARCH.md
│   └── AXIOM_Language_Plan.md
├── CLAUDE.md                   # Project context for AI agents
├── DESIGN.md                   # Living design document
├── BENCHMARKS.md               # Performance results
└── Cargo.toml                  # Workspace root
```

## Stats

- **30,124 lines of Rust** across 7 crates
- **450 tests** (all passing)
- **197 benchmarks** (100% compile rate)
- **120 git commits** across 8 development phases
- **97 builtin functions** (I/O, math, memory, concurrency, rendering, collections)
- **20 example programs**, **24 sample programs**
- **5 formal specification documents**
- **6 research documents** (optimization, memory, game engine, multithreading, Lux integration, language plan)

## Roadmap

### Completed Phases

- **Phase A:** MT-1 -- Fixed UB/soundness: removed incorrect `@pure`/`noalias`/`nosync` on shared pointers, added fences, fixed `@pure` semantics for write-through-ptr
- **Phase B:** MT-2, MT-3 -- `@parallel_for` with data clauses (private, shared_read, shared_write, reduction), HIR validation, correct LLVM IR with atomics/fences, thread-local accumulation + final combine
- **Phase C:** L1, L3, P1, P4 -- Constraint-driven LLM prompts (`@constraint { optimize_for: X }` threaded into LLM prompt), recursive `@const` evaluation, `@target { cpu: "native" }` with `-march=native`, constraint-to-clang-flag mapping
- **Phase D:** MT-4, MT-5, MT-6 -- `readonly_ptr[T]`/`writeonly_ptr[T]` ownership types, job dependency graph (`job_dispatch_handle`, `job_dispatch_after`, `job_wait_handle`), LLVM parallel metadata
- **Phase E:** F1, F2, F3, F5 -- Option/Result sum type builtins, string builtins (fat pointer), vec (dynamic array) builtins, function pointer builtins (`fn_ptr`, `call_fn_ptr_i32`, `call_fn_ptr_f64`)
- **Phase F:** L2, P2, P3 -- Hardware counter integration (perf data fed to LLM), `cpu_features()` CPUID detection, SIMD width metadata on vectorizable loops
- **Phase G:** F4, F6, F7, F8 -- (partial) Generics parsed, module system parsed, Result type implemented as builtins, while-let/if-let parsed
- **Phase H:** E1, E2, E3 -- GitHub Actions CI (`ci.yml`), DWARF debug info in LLVM IR, `axiom fmt` formatter, `axiom profile` profiler

### Remaining Phases

- **Phase I:** R1-R5 -- Real Vulkan renderer (ash + winit + gpu-allocator, GPU buffers, SPIR-V shaders, descriptor sets, production renderer)
- **Phase J:** G1-G5 -- Game engine (archetype ECS, input system, audio, hot reload, 10K particle demo with real Vulkan)
- **Phase K:** S1-S3 -- Self-improvement (self-hosted parser, compiler self-optimization, source-to-source AI optimizer)

## Development Pipeline

AXIOM was built using a multi-agent development pipeline with 7 independent agents:

| Agent | Role |
|-------|------|
| **Architect** | Designs specifications and acceptance criteria |
| **Optimistic Design Reviewer** | Reviews spec for completeness and ambition |
| **Pessimistic Design Reviewer** | Reviews spec for risks and missing edge cases |
| **Coder** | Implements from spec |
| **QA** | Runs tests, verifies acceptance criteria |
| **Optimistic Code Reviewer** | Reviews code for quality and patterns |
| **Pessimistic Code Reviewer** | Adversarial review for bugs and UB |

Each milestone goes through all 7 agents with git branch isolation and retry loops.

## License

MIT
