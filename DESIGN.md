# AXIOM Language Design Document v0.4

This is the living design document for AXIOM. It summarizes the current
implementation state, references the formal specification files, and tracks
design decisions and open questions.

**Project stats:** 138 commits, 35,802 LOC, 481 tests, 197 benchmarks, 27 examples, 24 samples. ALL 47 milestones COMPLETE across 8 tracks.

**FINAL benchmark result:** AXIOM beats C turbo (-O3 -march=native -ffast-math) by 3% overall (0.97x total wall clock) across 20 real-world benchmarks. 2 AXIOM wins (JPEG DCT 56% faster, RLE 16% faster), 9 ties, 9 C wins. Optimization Knowledge Base: 10 rules + 5 anti-patterns, grows with each LLM session.

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

### Lexer (`axiom-lexer`) -- Complete (63 tests)

- All token types defined: keywords, identifiers, literals (int with base
  and suffix, float with suffix, string, bool), operators (including wrapping
  `+%`, `-%`, `*%` and saturating `+|`, `-|`), delimiters, annotations (`@name`),
  optimization holes (`?name`), comments, and error recovery tokens.
- Keywords: `fn`, `let`, `mut`, `return`, `if`, `else`, `for`, `while`, `in`,
  `struct`, `type`, `module`, `import`, `pub`, `unsafe`, `extern`, `and`, `or`, `not`.
- Type keywords: `i8`, `i16`, `i32`, `i64`, `i128`, `u8`, `u16`, `u32`, `u64`, `u128`,
  `f16`, `bf16`, `f32`, `f64`, `bool`, `tensor`, `array`, `slice`, `ptr`,
  `readonly_ptr`, `writeonly_ptr`.
- Conversion keywords: `widen`, `narrow`, `truncate`.
- `Span` tracks byte offsets. `LineIndex` provides line/column lookup.
- Error recovery: invalid characters produce `TokenKind::Error` and lexing continues.

### Parser (`axiom-parser`) -- Complete (50 tests)

- Hand-written recursive descent parser with Pratt expression parsing.
- Produces typed `Module` AST with `Spanned<T>` wrappers on all nodes.
- Items: `Function`, `ExternFunction`, `StructDef`, `TypeAlias`, `ImportDecl`.
- Statements: `Let` (with `mut`), `Assign`, `Return`, `If`/`Else`, `For`, `While`, `Expr`.
- Expressions: literals, identifiers, `?holes`, binary ops (12 arithmetic + 6
  comparison + 2 logical), unary ops (`-`, `not`), function calls, indexing,
  field access, method calls, `array_zeros[T, N]`, conversion keywords
  (`widen`, `narrow`, `truncate`), parenthesized grouping.
- Type expressions: all 15 primitive types, `tensor[T, dims]`, `array[T, N]`,
  `slice[T]`, `ptr[T]`, `readonly_ptr[T]`, `writeonly_ptr[T]`, `fn(T) -> R`,
  tuples `(T1, T2)`, named types.
- Annotations: all built-in annotations parsed with specialized syntax including
  `@parallel_for` with data sharing clauses and `@lifetime` with scope/static/manual.
  Unknown annotations handled as `Custom(name, args)`.
- Strategy blocks with `?hole` values, nested sub-maps, and concrete values.
- Transfer blocks with all five fields.
- Error recovery: skips to synchronization points (`;`, `}`, statement/item
  keywords) and collects all errors. Max nesting depth of 256.

### HIR (`axiom-hir`) -- Complete (25 tests)

- Every HIR node carries a unique `NodeId` and `Span`.
- Two-pass lowering: first pass collects struct and type alias names; second
  pass lowers all items with full type knowledge.
- Type validation: primitive names resolved to `PrimitiveType` enum; user-defined
  names checked against known set; unknown types produce errors but lowering
  continues with `HirType::Unknown`.
- Types: `Primitive`, `UserDefined`, `Tensor`, `Array`, `Slice`, `Ptr`,
  `ReadonlyPtr`, `WriteonlyPtr`, `Vec2`, `Vec3`, `Vec4`, `Tuple`, `Fn`, `Unknown`.
- Annotation target validation: each annotation is checked against its set of
  valid targets (Module, Function, Param, StructDef, StructField, Block).
  Invalid placement produces errors but lowering continues.
- Annotations: `Pure`, `Const`, `Inline`, `Complexity`, `Intent`, `Module`,
  `Constraint`, `Target`, `Strategy`, `Transfer`, `Vectorizable`, `Parallel`,
  `Layout`, `Align`, `OptimizationLog`, `Export`, `Lifetime`, `ParallelFor`, `Custom`.
- Duplicate detection for functions, structs, and type aliases.
- Re-exports AST types that are identical between AST and HIR (BinOp, UnaryOp,
  InlineHint, LayoutKind, AnnotationValue, StrategyBlock, StrategyValue,
  TransferBlock, OptLogEntry, ParallelForConfig).

### Codegen (`axiom-codegen`) -- Complete (140 tests)

- HIR-to-LLVM-IR text generation for the full language subset: functions with
  all primitive types, arithmetic (with `nsw`), if/else, for loops, while loops,
  return, function calls, extern function declarations, `@export` functions.
- 137 builtin functions covering: I/O, math (26 functions incl. trig, log, exp),
  SIMD vector construction & math (vec2/vec3/vec4, dot, cross, length, normalize,
  reflect, lerp), conversions, bitwise, heap memory, arena allocation, file I/O,
  system, coroutines, threading, atomics, mutex, job system (with dependency graph),
  renderer/Vulkan FFI, option, string, vec, function pointers, result/error handling,
  CPU feature detection, input system, audio, and GPU operations.
- `vec2`/`vec3`/`vec4` -> LLVM `<N x double>` vector types with native SIMD
  `fadd`/`fmul`/`fsub`/`fdiv` instructions and `shufflevector`/`extractelement`.
- `@pure` -> `memory(none)` or `memory(argmem: read)`, `readnone`/`readonly`,
  fast-math flags on float operations.
- `@const` -> `speculatable` + compile-time evaluation (supports recursive
  `@const` functions with depth-limited interpretation).
- `@inline` -> `alwaysinline`/`noinline`/`inlinehint`.
- `@export` -> external linkage + C calling convention.
- `@lifetime(scope)` -> stack promotion of heap allocations.
- `@vectorizable` -> `!llvm.loop` vectorize metadata with SIMD width hints.
- `@parallel_for` -> correct parallel codegen with fences, atomics, and
  thread-local accumulation for reductions.
- `readonly_ptr[T]` -> LLVM `readonly` parameter attribute.
- `writeonly_ptr[T]` -> LLVM `writeonly` parameter attribute.
- `noalias` on all pointer parameters (language-level guarantee).
- `nsw` on all signed integer arithmetic.
- `fastcc` on all non-exported functions.
- `!prof` branch weights on `@pure` function base cases.
- `allockind`/`alloc-family` on all allocator builtins.
- `fence release`/`fence acquire` around parallel regions.
- DWARF debug metadata (`!dbg` references) for source-level debugging.

### Optimization Protocol (`axiom-optimize`) -- Complete (119 tests)

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
- **LLM Optimizer**: Builds constraint-driven prompts incorporating source,
  LLVM IR, assembly, benchmark data, `@constraint` annotations, and optimization
  history. Supports Claude API (via curl), Claude CLI, and dry-run modes.

### Driver (`axiom-driver`) -- Complete (72 tests)

- CLI frontend with 12 subcommands:
  - `axiom compile` -- full compilation (.axm -> native binary), with `--emit` for
    intermediate stages (tokens, ast, hir, llvm-ir) and `--target` for CPU arch.
  - `axiom lex` -- debug tokenizer output.
  - `axiom bench` -- benchmark a program (warmup + measurement runs).
  - `axiom mcp` -- start MCP JSON-RPC server on stdio.
  - `axiom optimize` -- LLM-driven optimization loop (iterations, dry-run, API key).
  - `axiom profile` -- compile, benchmark, extract surfaces, suggest tuning.
  - `axiom fmt` -- format source (parse -> HIR -> pretty-print).
  - `axiom doc` -- generate documentation from `@intent` and doc comments.
  - `axiom pgo` -- profile-guided optimization bootstrap.
  - `axiom watch` -- watch mode, recompile on file changes.
  - `axiom build` -- build project with dependency resolution.
  - `axiom rewrite` -- source-to-source AI rewriter.
  - `axiom lsp` -- LSP server for editor integration.
- C runtime (`axiom_rt.c`): I/O, nanosecond clock, coroutines (OS fibers/ucontext),
  threads, atomics, mutexes, thread-pool job system with dependency graph,
  Vulkan renderer, input system, audio playback.

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
| LLVM IR text gen | Direct text output to clang | Simpler bootstrap than inkwell/MLIR |

## Architecture

```
AXIOM Source (.axm)           -- AI agents read/write here
       |
       v
AXIOM Lexer (63 tests)       -- Tokenizer with error recovery
       |
       v
AXIOM Parser (50 tests)      -- Recursive descent + Pratt expressions
       |
       v
AXIOM HIR (25 tests)         -- Annotation validation, type checking, NodeIds
       |
       v
LLVM IR Text (140 tests)     -- Optimized IR with noalias, nsw, fast-math,
       |                        SIMD vec2/vec3/vec4, fastcc, fences,
       |                        readonly/writeonly, DWARF debug
       v
clang -O2                    -- Native binary (x86_64, AArch64)
```

The planned MLIR integration (via `melior` crate) will add an intermediate
MIR (Mid-level IR) stage between HIR and LLVM IR, enabling custom dialect
operations for tensor, GPU, and async patterns.

## LLM Self-Optimization Pipeline

AXIOM's core differentiator: the compiler can feed source + LLVM IR + assembly + benchmarks to an LLM, which analyzes the generated code and suggests improvements.

```
Source (.axm) -> Compile -> LLVM IR + Assembly
                              |
                    LLM (Claude API / CLI)
                    Analyzes: IR patterns, asm bottlenecks,
                              cache behavior, vectorization misses
                    Reads: @constraint { optimize_for: X }
                              |
                    Suggestion: ?param values, code restructuring,
                                new @annotations
                              |
                    Apply -> Recompile -> Re-benchmark -> Record
                              |
                    Iterate (LLM sees history of what worked)
```

**Constraint-driven prompts:** The `@constraint { optimize_for: "performance" }` annotation (and variants like `"memory"`, `"size"`, `"latency"`) is extracted from the source and injected into the LLM prompt. This changes the LLM's reasoning strategy -- "make it fast" vs "make it fit in 64KB" vs "minimize worst-case latency."

**Demonstrated:** LLM analyzed `divl` bottleneck in prime-counting assembly, suggested 6k+-1 wheel factorization -> 37% speedup. Both AXIOM and C produce identical output at identical speed (1.00x).

**Final benchmark:** AXIOM beats C turbo (-O3 -march=native -ffast-math) by 3% overall across 20 real-world programs (0.97x wall clock). Key wins: JPEG DCT 56% faster, RLE 16% faster. Key techniques: `calloc` zero-page trick, `@inline(always)` -> `alwaysinline`, arena allocators, `noalias` everywhere.

**Commands:**
- `axiom optimize program.axm --iterations 5` -- full LLM optimization loop
- `axiom optimize program.axm --dry-run` -- preview the prompt
- `axiom profile program.axm` -- compile, benchmark, extract surfaces, suggest tuning
- `axiom pgo program.axm` -- profile-guided optimization bootstrap
- `axiom rewrite program.axm` -- source-to-source AI rewriter

## Feature Inventory

| Category | Count | Details |
|----------|-------|---------|
| Primitive types | 15 | i8-i128, u8-u128, f16, bf16, f32, f64, bool |
| Compound types | 11 | array, ptr, readonly_ptr, writeonly_ptr, slice, vec2, vec3, vec4, tensor, tuple, fn |
| Annotations | 19 | pure, const, inline, complexity, intent, module, constraint, target, strategy, vectorizable, parallel, parallel_for, layout, align, lifetime, export, transfer, optimization_log, custom |
| Builtin functions | 137 | I/O, math, vectors, conversions, bitwise, memory, arena, file, system, coroutines, threads, atomics, mutex, jobs, renderer, option, string, vec, fn_ptr, result, cpu, input, audio, gpu |
| CLI commands | 12 | compile, lex, bench, mcp, optimize, profile, fmt, doc, pgo, watch, build, rewrite, lsp |
| Keywords | 21 | fn, let, mut, return, if, else, for, while, in, struct, type, module, import, pub, unsafe, extern, and, or, not, true, false |
| Type keywords | 25 | i8-i128 (5), u8-u128 (5), f16, bf16, f32, f64, bool, tensor, array, slice, ptr, readonly_ptr, writeonly_ptr, vec2, vec3, vec4 |
| Operators | 16 | +, -, *, /, %, +%, +\|, -%, -\|, *%, ==, !=, <, >, <=, >= |
| Milestones | 47/47 | ALL COMPLETE across 8 tracks (MT, LLM, Platform, Language, Ecosystem, Renderer, Engine, Self-Improvement) |

## Resolved Questions

- **Pattern matching**: Option/Result use builtin functions. While-let/if-let codegen complete.
- **Generics**: Parsed with monomorphization codegen implemented.
- **Module system**: `import` parsed, lowered, and separate compilation implemented.
- **Real Vulkan**: Production renderer with ash crate, instancing, multi-light, depth buffer.
- **ECS**: Archetype-based storage in `lib/ecs.axm`.
- **Input system**: Keyboard/mouse via Win32/Vulkan surface events.
- **Audio**: WAV playback via C runtime.
- **Self-hosting**: AXIOM parser written in AXIOM.
- **PGO**: Compiler self-optimization via profile-guided bootstrap.
- **Source-to-source optimization**: `axiom rewrite` -- LLM rewrites AXIOM source.

## Open Questions

- **Generic dimensions**: Tensor named dimensions (`M`, `N`) have no binding system.
- **MLIR integration**: MIR layer planned but not implemented.
- **Unsafe blocks**: `unsafe` keyword reserved but not enforced.

## Research Documents

| Document | Lines | Topic |
|----------|-------|-------|
| `docs/OPTIMIZATION_RESEARCH.md` | 1,600 | 20 LLVM optimization techniques to beat C |
| `docs/MEMORY_ALLOCATION_RESEARCH.md` | 1,200 | Arena, bump, pool, escape analysis |
| `docs/GAME_ENGINE_RESEARCH.md` | 2,014 | ECS, jobs, Vulkan, Lux integration |
| `docs/MULTITHREADING_ANALYSIS.md` | 1,513 | LLVM memory model, safe parallelism, 3 correct designs |
| `docs/LUX_INTEGRATION_RESEARCH.md` | 1,055 | Lux shader language convergence |
| `docs/AXIOM_Language_Plan.md` | 400 | Original 5-phase language design |
| `docs/MASTER_TASK_LIST.md` | 110 | 47-milestone task tracker across 8 tracks (ALL COMPLETE) |
