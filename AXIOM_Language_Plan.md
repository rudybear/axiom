# AXIOM: An AI-Native Language for LLVM
## Research Report & Language Design Plan

---

## Part 1: Landscape Analysis — What Already Exists

### 1.1 The Closest Thing: Klar (March 2026)

The most directly relevant project is **Klar** by MrPhil (Philip Ludington), published just one week ago. Klar is designed "from the ground up so that AI can write correct code on the first attempt." Key design choices:

- **Radical explicitness** — every line is self-contained; no type inference, explicit returns, explicit conversions
- **Three conversion operators** (`as#`, `to#`, `trunc#`) with different safety guarantees
- **Words over symbols** — `and`, `or`, `not` instead of `&&`, `||`, `!`
- **Meta layer** — compiler-validated annotations that embed *why* a decision was made, queryable by AI
- **Three backends** — tree-walking interpreter, bytecode VM, and LLVM native compiler
- **Written in Zig**, targeting application-level programming (the C#/Go/TypeScript space)

**Where Klar falls short for your vision**: Klar solves AI *writing* code for humans to read. It doesn't address AI-to-AI transfer, iterative machine optimization, or structured representation that AI can systematically improve. It's still fundamentally a human programming language that happens to be AI-friendly.

### 1.2 Mojo & MLIR (Chris Lattner / Modular)

**Mojo** is the most ambitious new systems language, built on MLIR (Multi-Level Intermediate Representation):

- Python-compatible syntax with systems-level performance
- Built on MLIR from the ground up — not LLVM IR directly, but MLIR which *lowers to* LLVM IR
- Exposes every accelerator instruction (tensor cores, TMAs) in Python-familiar syntax
- Write-once, retarget to H100, MI300X, Blackwell, etc.
- 10–100x faster than CPython; competitive with Rust

**Chris Lattner's position on AI languages**: He stated that "a new programming language for LLMs doesn't make sense" — arguing that readability matters more than writability, and that expressive, readable languages like Python (and Mojo) are what AI agents should use. *However*, this is about AI writing code for humans. Your vision is fundamentally different: AI writing code for AI to optimize further.

**MLIR is critical infrastructure**: MLIR is the most important technology for this project. Unlike LLVM IR (which is a single low-level abstraction), MLIR supports multiple abstraction levels via "dialects" — custom operations, types, and attributes for specific domains. It progressively lowers from high-level semantics down to LLVM IR and machine code. This multi-level approach is exactly what an AI-optimizable language needs.

### 1.3 LLVM IR as a "Language"

LLVM IR is already human-readable and can be treated as a programming language:

```llvm
define i32 @add(i32 %a, i32 %b) {
entry:
  %result = add i32 %a, %b
  ret i32 %result
}
```

- SSA form (Static Single Assignment) — every variable assigned exactly once
- Architecture-neutral — integer types aren't confined to hardware widths
- Well-documented, somewhat stable spec
- Text format (.ll) and binary format (bitcode/.bc)

**Why raw LLVM IR isn't enough**: It's too low-level. It's lost all semantic information — no loops, no data structures, no intent. An AI can't meaningfully *improve* LLVM IR without understanding what the code was *trying to do*. The optimization space is too local.

### 1.4 AI-Assisted Compiler Optimization (Research)

The academic community is actively exploring AI + compilers:

- Reinforcement learning for compiler pass ordering
- ML-based inlining heuristics (17% execution time reduction in Jikes RVM)
- AI-driven phase ordering in LLVM's modular pass infrastructure
- LLM-generated compiler commits already landing in LLVM (Alex Gaynor, June 2025)
- Full optimizing compilers built by AI (Darklang compiler by Paul Biggar, Christmas 2025 — what would have taken 2 years of human work)

### 1.5 The Gap: What Doesn't Exist Yet

Nobody is building what you're describing — a language designed as the **canonical transfer format between AI agents**, optimized for machine understanding and iterative improvement, that also compiles to LLVM. The missing piece is:

| Feature | Klar | Mojo | LLVM IR | MLIR | **Your Vision** |
|---|---|---|---|---|---|
| Human-readable | ✅ | ✅ | ⚠️ | ⚠️ | ✅ |
| AI-writable | ✅ | ✅ | ✅ | ✅ | ✅ |
| AI-to-AI transfer | ❌ | ❌ | ❌ | ⚠️ | ✅ |
| Semantic preservation | ✅ | ✅ | ❌ | ✅ | ✅ |
| Iterative AI optimization | ❌ | ❌ | ⚠️ | ⚠️ | ✅ |
| Compiles to native | ✅ | ✅ | ✅ | ✅ | ✅ |
| Optimization metadata | ⚠️ | ❌ | ❌ | ⚠️ | ✅ |
| Multi-level abstraction | ❌ | ✅ | ❌ | ✅ | ✅ |

---

## Part 2: The AXIOM Language Design

**AXIOM** = **A**I e**X**change **I**ntermediate **O**ptimization **M**edium

### 2.1 Core Philosophy

AXIOM isn't a language for humans to *program* in (like Klar) or for AI to *serve* humans (like Mojo). It's a **structured semantic representation** that AI agents use to communicate, transfer, and iteratively optimize code — while remaining human-auditable.

Think of it as: **"What if LLVM IR preserved all semantic intent, and was designed for AI agents to hand back and forth while making it progressively faster?"**

Three fundamental principles:

1. **Semantic Transparency** — Every construct carries its *intent*, not just its *mechanics*
2. **Optimization Surface** — The language exposes explicit optimization knobs that AI can systematically explore
3. **Deterministic Roundtrip** — Any AXIOM program can be lowered to LLVM IR and raised back without information loss

### 2.2 Language Architecture

```
┌──────────────────────────────────────────┐
│           AXIOM Source (.axm)             │
│  Human-readable, semantically rich       │
├──────────────────────────────────────────┤
│        AXIOM HIR (High-level IR)         │
│  Structured intent, optimization hints   │
├──────────────────────────────────────────┤
│        AXIOM MIR (Mid-level IR)          │
│  Lowered control flow, typed operations  │
├──────────────────────────────────────────┤
│        MLIR Dialects                     │
│  Custom AXIOM dialect + standard ones    │
├──────────────────────────────────────────┤
│        LLVM IR                           │
│  Standard optimization passes            │
├──────────────────────────────────────────┤
│     Native Code (x86, ARM, RISC-V...)    │
└──────────────────────────────────────────┘
```

The key insight: **AXIOM is not one level — it's a stack**. AI agents can operate at whatever level is appropriate. High-level refactoring? Work at AXIOM Source. Micro-optimization? Drop to AXIOM MIR. The levels are connected by well-defined lowering/raising passes.

### 2.3 Syntax Design — AXIOM Source

Every element in AXIOM carries three layers: **what**, **why**, and **how to optimize it**.

```axiom
// Module-level semantic declarations
@module matrix_multiply
@intent "Dense matrix multiplication for ML inference"
@constraint { latency < 2ms, memory < 64MB }
@target { cpu.simd, gpu.compute }

// Function with full semantic annotation
@pure                           // No side effects — safe to parallelize
@complexity O(n^3)              // AI knows the algorithmic class
@vectorizable dim(i, j, k)     // Explicit vectorization surfaces
fn matmul(
    a: tensor[f32, M, K] @layout(row_major) @align(64),
    b: tensor[f32, K, N] @layout(col_major) @align(64),
) -> tensor[f32, M, N] @layout(row_major) {
    
    // The 'strategy' block is the optimization surface
    @strategy {
        tiling:   { M: ?tile_m, N: ?tile_n, K: ?tile_k }  // AI fills ?params
        order:    ?loop_order                                // AI chooses
        parallel: ?parallel_dims                             // AI decides
        unroll:   { inner: ?unroll_factor }                  // AI tunes
    }
    
    let result = tensor.zeros[f32, M, N]
    
    for i in range(M), j in range(N) {
        let acc: f32 = 0.0
        for k in range(K) {
            acc += a[i, k] * b[k, j]
        }
        result[i, j] = acc
    }
    
    return result
}
```

Key syntax features:

- **`?param`** — Optimization holes. AI agents fill these with concrete values, measure performance, iterate. This is the core of the "constantly improveable" design.
- **`@annotations`** — Structured metadata that's machine-parseable, not comments. The compiler validates them.
- **`@strategy` blocks** — Explicit, structured optimization surfaces. AI doesn't have to *discover* that tiling is possible; the language tells it "here is where you can tile, and here are the knobs."
- **`@constraint`** — Hard requirements the optimizer must respect.
- **`@intent`** — What the code is trying to achieve at a semantic level.

### 2.4 The Optimization Protocol

This is what makes AXIOM fundamentally different. The language includes a first-class **optimization protocol** — a structured way for AI agents to:

1. **Analyze** — Read the semantic annotations and constraints
2. **Propose** — Fill in `?params` with concrete values
3. **Validate** — Compiler checks that constraints are still met
4. **Measure** — Built-in benchmarking harness
5. **Record** — Optimization history is stored in the source file

```axiom
// Optimization history — machine-generated, human-auditable
@optimization_log {
    v1: { tile_m: 32, tile_n: 32, tile_k: 8, loop_order: [i,j,k] }
        -> 45.2ms, cache_miss_rate: 0.23
    v2: { tile_m: 64, tile_n: 64, tile_k: 16, loop_order: [j,i,k] }
        -> 28.1ms, cache_miss_rate: 0.11
    v3: { tile_m: 64, tile_n: 32, tile_k: 32, loop_order: [i,k,j], parallel: [i] }
        -> 12.7ms, cache_miss_rate: 0.04
    // Agent: claude-opus-4.6 | Target: x86_64-avx512 | Date: 2026-03-21
}
```

Each AI agent that touches the code can see what was tried before, what worked, and why. No wasted exploration. No repeated mistakes.

### 2.5 Inter-Agent Transfer Format

When one AI agent passes AXIOM code to another, the transfer includes:

```axiom
@transfer {
    source_agent: "claude-opus-4.6"
    target_agent: "any"
    context: "Optimized tiling for AVX-512. Memory layout changes may yield 
              further gains. Consider data prefetch strategy."
    open_questions: [
        "Is col_major optimal for b given downstream consumers?",
        "Explore GPU offload for M > 4096"
    ]
    confidence: { correctness: 0.99, optimality: 0.7 }
}
```

This is the "handoff protocol" — structured, not prose. Any AI agent can parse this, understand the current state, and continue the optimization process.

### 2.6 Type System

AXIOM's type system is designed for machine reasoning:

```axiom
// Primitive types — explicit widths, always
i8, i16, i32, i64, i128          // Signed integers
u8, u16, u32, u64, u128          // Unsigned integers  
f16, bf16, f32, f64              // Floating point (bf16 for ML)
bool                              // Boolean

// Tensor types — first class, with shape information
tensor[f32, 3, 224, 224]          // Static shape
tensor[f32, ?, ?, 3]              // Dynamic dimensions with ?
tensor[f32, N, M] where N > 0    // Constrained dimensions

// Memory layout is part of the type
tensor[f32, M, N] @layout(row_major) @align(64)

// Sum types for control flow
type Result[T, E] = Ok(T) | Err(E)

// No implicit conversions — ever
let x: i32 = 42
let y: i64 = widen(x)            // Explicit, named conversion
let z: i16 = narrow(x)           // Compiler warns, AI sees the risk
```

### 2.7 Memory Model

Explicit, machine-reasoned memory:

```axiom
// Allocation strategy is declared, not implicit
let buffer = alloc[f32, 1024] @lifetime(scope) @arena(scratch)

// Ownership is explicit but simpler than Rust
let owned_data = tensor.new[f32, 256, 256]     // This function owns it
let view = owned_data.view[0:128, 0:128]       // Borrowing — no copy
let copy = owned_data.clone()                    // Explicit copy

// Memory transfer between devices
let gpu_data = owned_data.to(@device(gpu.0))    // Explicit device transfer
```

---

## Part 3: Implementation Plan

### Phase 1 — Foundation (Months 1–3)

**Goal**: Minimal viable language that can express, compile, and optimize a single compute kernel.

| Week | Deliverable |
|------|-------------|
| 1–2 | Language specification v0.1 — syntax, type system, annotation schema |
| 3–4 | Lexer + Parser (written in Rust, targeting speed and correctness) |
| 5–6 | AXIOM HIR — internal representation with semantic annotations preserved |
| 7–8 | Lowering from HIR → MLIR (custom AXIOM dialect) |
| 9–10 | Lowering from MLIR → LLVM IR |
| 11–12 | End-to-end: `.axm` source → native binary for a simple kernel |

**Key decision**: Build the compiler in **Rust** using the `inkwell` crate (safe LLVM bindings) or directly target MLIR via `melior` (Rust MLIR bindings). MLIR is the better long-term choice because it gives us multi-level optimization and dialect extensibility.

### Phase 2 — AI Loop (Months 4–6)

**Goal**: The optimization protocol works. An AI agent can iteratively improve AXIOM code.

| Week | Deliverable |
|------|-------------|
| 13–14 | `?param` (optimization holes) — parser support, compiler validation |
| 15–16 | `@strategy` blocks — structured optimization surface extraction |
| 17–18 | Built-in benchmarking harness — compile, run, measure, report |
| 19–20 | Optimization history recording in source files |
| 21–22 | CLI tool: `axiom optimize --agent=claude --target=x86_64-avx512` |
| 23–24 | First end-to-end demo: AI agent iteratively optimizes matmul from naive to near-optimal |

**Critical milestone**: The matmul demo. If an AI agent can take a naive AXIOM matmul, fill in strategy params, benchmark, iterate, and converge on performance within 80% of hand-tuned code — the concept is proven.

### Phase 3 — Transfer Protocol (Months 7–9)

**Goal**: Multiple AI agents can work on the same AXIOM codebase, building on each other's work.

| Week | Deliverable |
|------|-------------|
| 25–28 | `@transfer` blocks — structured inter-agent handoff |
| 29–32 | Agent API — programmatic interface for AI to read/write AXIOM |
| 33–36 | Multi-agent demo: Agent A writes naive code → Agent B optimizes for CPU → Agent C offloads hot paths to GPU |

### Phase 4 — Ecosystem (Months 10–12)

**Goal**: AXIOM is usable for real workloads.

| Week | Deliverable |
|------|-------------|
| 37–40 | Standard library — tensor ops, collections, I/O |
| 41–44 | Interop — call C/Rust/Python from AXIOM, call AXIOM from Python |
| 45–48 | MCP server — expose AXIOM optimization as a tool AI agents can use |
| 48 | Open source release with documentation and examples |

### Phase 5 — Self-Improvement (Month 12+)

The endgame: **AXIOM's compiler is written in AXIOM**, and AI agents optimize the compiler itself. The language optimizes itself. The compiler gets faster at making code faster. This is the "perfectly iteratable" vision realized.

---

## Part 4: Technical Decisions

### 4.1 Why Target MLIR, Not LLVM IR Directly

- MLIR's dialect system lets us define AXIOM-specific operations that preserve semantics during lowering
- MLIR has existing dialects for tensors (`linalg`), GPU (`gpu`), async (`async`), etc.
- MLIR's progressive lowering means we can optimize at multiple levels
- MLIR → LLVM IR lowering is already solved and maintained by hundreds of engineers
- Mojo proves this architecture works at production scale

### 4.2 Why Write the Compiler in Rust

- Memory safety without GC — important for a bootstrapping compiler
- `melior` crate provides safe MLIR bindings
- `inkwell` crate provides safe LLVM bindings (if we need direct LLVM access)
- Excellent ecosystem for parser combinators (`nom`, `chumsky`)
- AI agents are extremely proficient at writing Rust (high training data quality)
- Rust itself compiles via LLVM, so we understand the target intimately

### 4.3 File Format

AXIOM source files (`.axm`) are UTF-8 text. The format is designed so that:

- Any valid AXIOM file can be parsed by a simple recursive descent parser
- Annotations are structured (not free-form comments) — machine-readable
- Optimization history is embedded in the source, version-controlled with it
- The file is simultaneously a program, a specification, and a performance log

### 4.4 AI Agent Interface

AXIOM provides a programmatic API (not just CLI):

```python
import axiom

# Load and analyze
module = axiom.load("matmul.axm")
surfaces = module.optimization_surfaces()     # List of ?params
history = module.optimization_history()        # What's been tried
constraints = module.constraints()             # Hard requirements

# Propose optimization
proposal = axiom.Proposal(
    tile_m=64, tile_n=32, tile_k=32,
    loop_order=["i", "k", "j"],
    parallel=["i"]
)

# Validate and benchmark
result = module.apply_and_benchmark(proposal, target="x86_64-avx512")
# result.time_ms, result.cache_miss_rate, result.instruction_count

# Record and save
module.record_optimization(proposal, result, agent="claude-opus-4.6")
module.save("matmul.axm")
```

This is the interface an AI agent's tool-use would call. It's structured, typed, and deterministic — not "generate the whole file and hope it compiles."

---

## Part 5: What Makes This Different

| Existing approach | AXIOM's approach |
|---|---|
| AI generates code in Python/Rust/C++ | AI operates on a purpose-built representation |
| Optimization is implicit (compiler flags) | Optimization surfaces are explicit (`?params`, `@strategy`) |
| Code carries no history | Optimization history is embedded and structured |
| One agent works alone | Structured handoff between agents (`@transfer`) |
| "Make it faster" (vague) | `@constraint { latency < 2ms }` (precise) |
| AI guesses what can be optimized | Language declares what *should* be optimized |
| Semantic intent is lost at IR level | Intent preserved through all lowering levels |
| Each iteration starts from scratch | Each iteration builds on recorded measurements |

---

## Appendix: Name Alternatives

If AXIOM doesn't land, consider:

- **FORGE** — Formally Optimizable Representation for Generative Engineering
- **NEXIR** — Next-gen Extensible Intermediate Representation
- **LATTICE** — Language for AI-Transferable, Typed, Iterative Code Engineering
- **PRISM** — Progressive Representation for Iterative System Machines

---

*Document generated 2026-03-21. This is a living plan — the first step is validating the core idea with a minimal prototype targeting the matmul optimization loop.*
