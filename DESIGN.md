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

## Open Questions

- **Sum types**: The spec mentions `type Name = Variant1(T) | Variant2(T)`
  but the parser does not yet implement sum type definitions. The `Pipe`
  token exists in the lexer.
- **`@arena` and `@lifetime`**: Mentioned in `CLAUDE.md` annotation list
  but not yet implemented in parser or HIR.
- **Pattern matching**: No `match` statement or pattern destructuring yet.
- **Generic dimensions**: Tensor types support named dimensions (`M`, `N`)
  but there is no generic/parametric polymorphism system to bind them.
- **Complexity expressions**: Currently stored as freeform strings.
  Structured complexity types (`O(n)`, `O(n log n)`, `O(n^k)`) are planned.
- **MLIR integration**: The MIR layer and MLIR codegen are defined in the
  project structure but not yet implemented.
- **Unsafe blocks**: The `unsafe` keyword is reserved but unsafe blocks are
  not yet parsed or enforced for `ptr[T]` usage.
