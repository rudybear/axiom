# AXIOM Language Design Document v0.1

This is the living design document for AXIOM. It summarizes the current
implementation state, references the formal specification files, and tracks
design decisions and open questions.

See `CLAUDE.md` for project structure, conventions, and development workflow.

## Formal Specification

The `spec/` directory contains the formal language specification:

| File | Contents |
|------|----------|
| [`spec/grammar.ebnf`](spec/grammar.ebnf) | Complete EBNF grammar -- all productions the parser implements |
| [`spec/types.md`](spec/types.md) | Type system -- primitives, compounds, validation rules, explicit conversions |
| [`spec/annotations.md`](spec/annotations.md) | Annotation schema -- every annotation, valid targets, syntax, semantics |
| [`spec/optimization.md`](spec/optimization.md) | Optimization protocol -- holes, strategy blocks, the propose/validate/benchmark loop |
| [`spec/transfer.md`](spec/transfer.md) | Inter-agent transfer protocol -- `@transfer` blocks, AgentSession API |

## Implementation Status

### Lexer (`axiom-lexer`) -- Complete

- All token types defined: keywords, identifiers, literals (int with base
  and suffix, float with suffix, string, bool), operators (including wrapping
  `+%`, `-%`, `*%` and saturating `+|`, `-|`), delimiters, annotations (`@name`),
  optimization holes (`?name`), comments, and error recovery tokens.
- `Span` tracks byte offsets. `LineIndex` provides line/column lookup.
- Error recovery: invalid characters produce `TokenKind::Error` and lexing continues.

### Parser (`axiom-parser`) -- Complete

- Hand-written recursive descent parser with Pratt expression parsing.
- Produces typed `Module` AST with `Spanned<T>` wrappers on all nodes.
- Items: `Function`, `ExternFunction`, `StructDef`, `TypeAlias`, `ImportDecl`.
- Statements: `Let` (with `mut`), `Assign`, `Return`, `If`/`Else`, `For`, `While`, `Expr`.
- Expressions: literals, identifiers, `?holes`, binary ops (12 arithmetic + 6
  comparison + 2 logical), unary ops (`-`, `not`), function calls, indexing,
  field access, method calls, `array_zeros[T, N]`, conversion keywords
  (`widen`, `narrow`, `truncate`), parenthesized grouping.
- Type expressions: all 15 primitive types, `tensor[T, dims]`, `array[T, N]`,
  `slice[T]`, `ptr[T]`, `fn(T) -> R`, tuples `(T1, T2)`, named types.
- Annotations: all built-in annotations parsed with specialized syntax; unknown
  annotations handled as `Custom(name, args)`.
- Strategy blocks with `?hole` values, nested sub-maps, and concrete values.
- Transfer blocks with all five fields.
- Error recovery: skips to synchronization points (`;`, `}`, statement/item
  keywords) and collects all errors. Max nesting depth of 256.

### HIR (`axiom-hir`) -- Complete

- Every HIR node carries a unique `NodeId` and `Span`.
- Two-pass lowering: first pass collects struct and type alias names; second
  pass lowers all items with full type knowledge.
- Type validation: primitive names resolved to `PrimitiveType` enum; user-defined
  names checked against known set; unknown types produce errors but lowering
  continues with `HirType::Unknown`.
- Annotation target validation: each annotation is checked against its set of
  valid targets (Module, Function, Param, StructDef, StructField, Block).
  Invalid placement produces errors but lowering continues.
- Duplicate detection for functions, structs, and type aliases.
- Re-exports AST types that are identical between AST and HIR (BinOp, UnaryOp,
  InlineHint, LayoutKind, AnnotationValue, StrategyBlock, StrategyValue,
  TransferBlock, OptLogEntry).

### Codegen (`axiom-codegen`) -- Implemented

- HIR-to-LLVM-IR generation via `inkwell` for a core subset: functions with
  `i32`/`i64`/`f32`/`f64` parameters and returns, arithmetic, if/else, for
  loops, while loops, return, function calls, extern function declarations.
- `@export` functions use external linkage and C calling convention.

### Optimization Protocol (`axiom-optimize`) -- Complete

- **Surface extraction**: `extract_surfaces` parses source through HIR and
  walks all functions for `@strategy` annotations and `?hole` expressions.
  Produces `OptSurface` descriptors with holes, types, ranges, and strategy info.
- **Proposal validation**: `validate_proposal` checks completeness, type
  correctness, range validity, and unknown holes. Collects all errors.
- **Benchmarking**: `benchmark_binary` and `benchmark_source` run compiled
  binaries with warmup + measurement, computing median/mean/min/max/stddev.
- **History**: `OptHistory` stores `OptRecord`s with version, params, metrics,
  agent, target, and timestamp. Serializable to/from JSON.
- **Transfer**: `TransferInfo` extraction, generation, and round-trip through
  AXIOM source.
- **Agent API**: `AgentSession` wraps the full workflow -- load source, inspect
  surfaces, apply proposals, manage history, export with transfer metadata.

### Driver (`axiom-driver`) -- Exists

- CLI frontend. Handles `axiom compile` with `--emit=hir`, `--emit=llvm-ir`,
  and binary output.

## Core Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| No type inference | Every binding has explicit type | Zero ambiguity for AI agents |
| No implicit conversions | `widen`/`narrow`/`truncate` | Named conversions are unambiguous |
| No operator overloading | `+` always means numeric add | Predictable semantics |
| Semicolons required | Every statement ends with `;` | No ASI ambiguity |
| Braces for blocks | No significant whitespace | Unambiguous parse from any position |
| Words for logic | `and`, `or`, `not` (not `&&`, `\|\|`, `!`) | Readability for AI agents |
| `?` prefix for holes | `?tile_m`, `?unroll_factor` | Distinct from all other syntax |
| `@` prefix for annotations | `@pure`, `@strategy { ... }` | Machine-parseable structured metadata |
| Hand-written parser | Recursive descent + Pratt | Precise error recovery and span tracking |
| Rust compiler | Memory safety, LLVM bindings | AI writes Rust well; strong ecosystem |
| inkwell for initial codegen | Direct LLVM IR generation | Simpler bootstrap than full MLIR |

## Architecture

```
AXIOM Source (.axm)           -- AI agents read/write here
       |
       v
AXIOM HIR (High-level IR)     -- Semantic intent preserved, optimization holes visible
       |
       v
LLVM IR (via inkwell)         -- Standard LLVM optimization passes
       |
       v
Native binary                 -- x86_64, AArch64, RISC-V
```

The planned MLIR integration (via `melior` crate) will add an intermediate
MIR (Mid-level IR) stage between HIR and LLVM IR, enabling custom dialect
operations for tensor, GPU, and async patterns.

## LLM Self-Optimization Pipeline

AXIOM's core differentiator: the compiler can feed source + LLVM IR + assembly + benchmarks to an LLM, which analyzes the generated code and suggests improvements.

```
Source (.axm) → Compile → LLVM IR + Assembly
                              ↓
                    LLM (Claude API / CLI)
                    Analyzes: IR patterns, asm bottlenecks,
                              cache behavior, vectorization misses
                              ↓
                    Suggestion: ?param values, code restructuring,
                                new @annotations
                              ↓
                    Apply → Recompile → Re-benchmark → Record
                              ↓
                    Iterate (LLM sees history of what worked)
```

**Demonstrated:** LLM analyzed `divl` bottleneck in prime-counting assembly, suggested 6k±1 wheel factorization → 37% speedup. Both AXIOM and C produce identical output at identical speed (1.00x).

**Commands:**
- `axiom optimize program.axm --iterations 5` — full LLM optimization loop
- `axiom optimize program.axm --dry-run` — preview the prompt
- `axiom profile program.axm` — compile, benchmark, extract surfaces

## Platform-Specific Optimization (Planned)

AXIOM should detect and utilize the host CPU's full feature set:

```axiom
@target { cpu: "native" }  // Use all available CPU features
@target { cpu: "x86_64-v4" }  // Require AVX-512
@constraint { optimize_for: "performance" }  // vs "memory" vs "size"
```

The compiler would:
1. Query CPUID / equivalent for available features (AVX2, AVX-512, etc.)
2. Pass `-march=native` or specific `-mavx512f` flags to clang
3. The LLM optimizer could also observe "this loop isn't vectorized" and suggest `@vectorizable` or restructuring
4. `@constraint { optimize_for: "memory" }` would prefer `-Os`, smaller tile sizes, in-place algorithms

## Constraint-Driven Compilation (Planned)

Different optimization goals require different strategies:

```axiom
@constraint { optimize_for: "performance" }  // -O3, aggressive inlining, large tiles
@constraint { optimize_for: "memory" }        // -Os, minimal allocations, streaming
@constraint { optimize_for: "size" }           // -Oz, no loop unrolling
@constraint { optimize_for: "latency" }        // minimize worst-case, avoid allocations
@constraint { budget: "frame_time < 16.6ms" }  // game: must hit 60fps
```

The LLM optimizer would use these constraints to guide its suggestions — not just "make it fast" but "make it fast within 64KB of memory" or "minimize worst-case latency."

## Known Correctness Issues

### Multithreading (CRITICAL — see docs/MULTITHREADING_ANALYSIS.md)

The current job system is **unsound**:
- `@pure` on functions that write through pointers emits `memory(argmem: read)` → LLVM can delete stores (UB)
- `noalias` on shared array pointers across threads is incorrect
- No memory fences, no reduction support, no dependency tracking

Fix plan: OpenMP-style `@parallel_for` with explicit data sharing clauses, then ownership-based slices, then dependency graphs.

## Open Questions

- **Sum types**: Parsed but not codegen'd. `Pipe` token exists.
- **Pattern matching**: No `match` statement yet.
- **Generic dimensions**: Tensor named dimensions (`M`, `N`) have no binding system.
- **MLIR integration**: MIR layer planned but not implemented.
- **Unsafe blocks**: `unsafe` keyword reserved but not enforced.
- **Real Vulkan**: Stub renderer needs replacement with ash-based Rust crate (see `docs/RENDERING_PLAN.md`).

## Research Documents

| Document | Lines | Topic |
|----------|-------|-------|
| `docs/OPTIMIZATION_RESEARCH.md` | 1,600 | 20 LLVM optimization techniques to beat C |
| `docs/MEMORY_ALLOCATION_RESEARCH.md` | 1,200 | Arena, bump, pool, escape analysis |
| `docs/GAME_ENGINE_RESEARCH.md` | 2,014 | ECS, jobs, Vulkan, Lux integration |
| `docs/MULTITHREADING_ANALYSIS.md` | 1,513 | LLVM memory model, safe parallelism, 3 correct designs |
| `docs/LUX_INTEGRATION_RESEARCH.md` | 1,055 | Lux shader language convergence |
| `docs/VULKAN_INTEGRATION_PLAN.md` | 200 | 5-phase Vulkan renderer plan |
| `docs/PHASE7_GAME_ENGINE_PLAN.md` | 300 | 9-milestone game engine roadmap |
| `docs/AXIOM_Language_Plan.md` | 400 | Original 5-phase language design |
