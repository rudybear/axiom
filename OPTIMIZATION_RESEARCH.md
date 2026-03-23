# AXIOM Optimization Research: Techniques to Generate Faster Code Than C/clang -O2

## Executive Summary

AXIOM's key advantage over C is **information asymmetry**: AXIOM knows things about the
program that C's compiler must conservatively assume are unknown. Every `@pure`, `@const`,
`@layout`, `@vectorizable` annotation is a **proof** that the programmer (or AI agent)
provides to the compiler, eliminating analysis that LLVM must otherwise perform
speculatively or give up on entirely.

The techniques below are prioritized by **impact-to-effort ratio**: how much real
performance you gain divided by how hard it is to implement. Each technique lists the
LLVM features to use and expected speedup ranges from published benchmarks and papers.

---

## TIER 1: HIGH IMPACT, LOW-TO-MEDIUM EFFORT
*Implement these first. They compound with each other.*

---

### 1. Alias Analysis via @pure + Ownership Semantics (noalias everywhere)

**What it is:**
In C, the compiler must assume any two pointers might alias the same memory unless it can
prove otherwise. The `restrict` keyword helps but is opt-in, error-prone, and not part of
C++. AXIOM can guarantee no-aliasing by design: `@pure` functions have no side effects,
and explicit ownership/borrowing means the compiler always knows which references are
unique.

**Why AXIOM beats C:**
- C's `restrict` is function-argument-scoped only. AXIOM's ownership model provides
  whole-program aliasing guarantees.
- Fortran is historically faster than C for numerical code *precisely* because of this:
  Fortran forbids argument aliasing by default. AXIOM can do the same.
- Rust attempted this with `&mut T` -> `noalias`, but LLVM had bugs that forced Rust to
  disable it for years. AXIOM, being AI-generated, can be stricter than Rust: every
  mutable reference is provably unique.
- A Nim language RFC showed **25x speedups** from no-aliasing annotations in numerical
  code. C `restrict` typically gives **2-3x** on pointer-heavy code.

**How to implement (LLVM):**
1. Emit `noalias` on every function parameter where AXIOM's type system proves no aliasing.
2. Emit `!noalias` and `!alias.scope` metadata on loads/stores (scoped noalias metadata).
3. Emit `!tbaa` (type-based alias analysis) metadata -- AXIOM's explicit types make this
   trivially correct.
4. For `@pure` functions: emit `readonly` or `readnone` function attributes.
5. Use `dereferenceable(N)` and `nonnull` on all non-nullable references.
6. Use `align(N)` from `@align` annotations.

**Key LLVM attributes to emit:**
```llvm
define void @matmul(ptr noalias nonnull dereferenceable(1024) align(64) %a,
                    ptr noalias nonnull dereferenceable(1024) align(64) %b,
                    ptr noalias nonnull dereferenceable(1024) align(64) %result)
    readonly #0 {
  ; ...
}
attributes #0 = { nounwind willreturn nosync }
```

**Expected performance gain:** 10-50% on numerical code, 2-3x on pointer-heavy code.
**Difficulty:** LOW. Just emit the right LLVM IR attributes during codegen. The information
is already in AXIOM's type system and annotations.

**References:**
- LLVM Alias Analysis Infrastructure: https://llvm.org/docs/AliasAnalysis.html
- LLVM Frontend Performance Tips: https://llvm.org/docs/Frontend/PerformanceTips.html
- Restrict-Qualified Pointers in LLVM (Hal Finkel): https://llvm.org/devmtg/2017-02-04/Restrict-Qualified-Pointers-in-LLVM.pdf

---

### 2. Pure Function Optimization (@pure -> readnone/readonly)

**What it is:**
Marking functions as `readnone` (no memory access) or `readonly` (reads but doesn't write)
enables LLVM to: (a) eliminate redundant calls with identical arguments, (b) hoist calls
out of loops, (c) reorder calls freely, (d) eliminate calls whose results are unused.

**Why AXIOM beats C:**
- C compilers must *infer* purity by analyzing the entire function body, and fail for
  any function with external calls, global access, or cross-module boundaries.
- AXIOM's `@pure` annotation is a **guarantee from the source**. The compiler trusts it
  (and can verify it during HIR validation).
- With `@pure` on mathematical functions, LLVM can CSE (Common Subexpression Eliminate)
  across entire function bodies, something impossible in C without LTO and even then
  unreliable.

**How to implement (LLVM):**
1. `@pure` functions that don't read any memory -> `readnone`
2. `@pure` functions that read arguments -> `readonly argmemonly`
3. All `@pure` functions -> `nounwind willreturn nosync nofree`
4. `@const` functions -> `readnone` + mark as `speculatable`
5. Emit `mustprogress` on all functions (AXIOM doesn't have infinite loops without
   side effects)

**Key insight from LLVM docs:**
> "Marking functions as readnone/readonly/argmemonly or noreturn/nounwind is
> important because the optimizer will try to infer these flags, but may not
> always be able to."

**Expected performance gain:** 5-30% depending on code patterns. Huge for code that
calls the same pure function multiple times with the same arguments (common in
mathematical/scientific code).
**Difficulty:** LOW. Map `@pure` -> LLVM attributes in codegen.

**References:**
- LLVM LangRef Function Attributes: https://llvm.org/docs/LangRef.html
- Swift Optimizer Effects Proposal: https://github.com/apple/swift/blob/main/docs/proposals/OptimizerEffects.rst

---

### 3. Compile-Time Evaluation (@const / comptime)

**What it is:**
Evaluating expressions, function calls, and even complex computations at compile time,
embedding the results directly in the binary. Goes far beyond C's `constexpr` because
AXIOM can evaluate arbitrary `@const` functions including loops, recursion, and data
structure manipulation.

**Why AXIOM beats C:**
- C's `constexpr` (C23) is limited: no heap allocation, no I/O, restricted control flow.
- C++ `constexpr` is more powerful but still has restrictions and requires explicit marking.
- Zig's `comptime` demonstrates the power: **40% runtime cost reduction** by shifting
  computation to compile-time. AXIOM's `@const` can do the same.
- Mojo's comptime system (built on MLIR) shows how compile-time evaluation can eliminate
  entire abstraction layers.
- AXIOM's `@const` functions can precompute lookup tables, hash values, configuration
  data, mathematical constants to arbitrary precision, etc.

**How to implement:**
1. Build a compile-time interpreter in the AXIOM compiler (at HIR or MIR level).
2. When a `@const` function is called with all-constant arguments, evaluate at compile
   time and replace the call with the result.
3. For `@const` expressions in strategy blocks, evaluate during optimization surface
   extraction.
4. In LLVM IR: emit the precomputed constant directly (no function call at all).
5. Use LLVM's `ConstantFolding` and `ConstantPropagation` passes for anything the
   frontend doesn't catch.

**Expected performance gain:** Unbounded for specific cases (turning O(n) runtime into
O(1)), typically 5-40% overall depending on how much computation can be shifted.
**Difficulty:** MEDIUM. Requires building an interpreter, but it operates on your own
IR so you control the semantics.

**References:**
- Zig Comptime: https://zig.guide/language-basics/comptime/
- Mojo Compile-Time Metaprogramming: https://docs.modular.com/mojo/manual/parameters/
- C++ constexpr: https://www.learncpp.com/cpp-tutorial/constant-expressions-and-compile-time-optimization/

---

### 4. Aggressive Inlining with @inline + @pure Hints

**What it is:**
Inlining is the single most impactful optimization because it enables all other
optimizations (constant propagation, DCE, vectorization, etc.) to work across function
boundaries. AXIOM's annotations give the compiler far better inlining decisions than C.

**Why AXIOM beats C:**
- C's inliner uses heuristics based on function size, call frequency, and estimated
  benefit. It often makes wrong decisions.
- AXIOM has `@inline(always|never|hint)` for explicit control.
- `@pure` functions are *always* safe to inline (no side effects).
- `@complexity(O(1))` functions should almost always be inlined.
- `@complexity(O(n^3))` functions should almost never be inlined.
- The AI agent can tune `@inline` based on profiling data from the built-in benchmark
  harness.

**How to implement (LLVM):**
1. `@inline(always)` -> `alwaysinline` LLVM attribute
2. `@inline(never)` -> `noinline` LLVM attribute
3. `@inline(hint)` -> `inlinehint` LLVM attribute
4. `@pure` + small function -> auto-add `inlinehint`
5. Use `internal` or `private` linkage for non-exported functions (enables more
   aggressive inlining)
6. Customize the inliner's threshold via LLVM pass parameters

**Expected performance gain:** 5-30%. Cross-module inlining (which LTO enables) is the
single biggest source of performance gain from LTO, at ~2.86% average for full LTO.
**Difficulty:** LOW. Just emit LLVM attributes.

**References:**
- LLVM Frontend Performance Tips: https://llvm.org/docs/Frontend/PerformanceTips.html
- ThinLTO: https://clang.llvm.org/docs/ThinLTO.html

---

### 5. nsw/nuw Flags and Fast-Math from Type System

**What it is:**
LLVM's `nsw` (no signed wrap) and `nuw` (no unsigned wrap) flags on arithmetic tell the
optimizer that overflow is undefined behavior, enabling transformations like:
- `(x + 1) > x` -> `true`
- Loop trip count computation without overflow checks
- Strength reduction of induction variables

Fast-math flags on floating point enable reassociation, reciprocal approximation, and
vectorization of reductions.

**Why AXIOM beats C:**
- C has these semantics for signed integers (UB on overflow) but the compiler must
  *prove* no overflow occurs, which is hard.
- AXIOM can emit `nsw`/`nuw` based on explicit range annotations:
  `@constraint { range: 0..1000000 }` means all arithmetic in that range is nuw/nsw.
- For floating point, AXIOM's `@constraint { precision: "relaxed" }` or
  `@constraint { correctness: "fast-math" }` maps directly to LLVM fast-math flags.
- LLVM docs: "reasoning about overflow is generally hard for an optimizer so providing
  these facts from the frontend can be very impactful."

**How to implement (LLVM):**
1. Emit `nsw` on signed integer arithmetic when overflow is impossible (from range info).
2. Emit `nuw` on unsigned integer arithmetic similarly.
3. Emit `fast` flag group (or individual: `nnan ninf nsz arcp contract afn reassoc`)
   on floating-point ops when `@constraint { precision: "relaxed" }`.
4. Emit `!range` metadata on loads when the value range is known.
5. Use `poison` instead of `undef` everywhere (LLVM recommendation).
6. Emit `noundef` on function parameters.

**Expected performance gain:** 5-20% on arithmetic-heavy code. Fast-math flags can
enable vectorization of reductions that would otherwise be impossible (due to
floating-point non-associativity).
**Difficulty:** LOW. Emit flags during codegen based on annotations.

**References:**
- LLVM Performance Tips: https://llvm.org/docs/Frontend/PerformanceTips.html
- LLVM LangRef (nsw/nuw): https://llvm.org/docs/LangRef.html

---

### 6. Dead Code Elimination (DCE) with @pure

**What it is:**
If a `@pure` function is called but its return value is unused, the call can always be
removed. In C, the compiler can only do this if it can prove the function has no side
effects, which requires whole-program analysis.

**Why AXIOM beats C:**
- C compiler DCE only works on trivially dead code and functions it can fully analyze.
- AXIOM's `@pure` is a blanket guarantee: any pure function with unused results is dead.
- At -O1+, LLVM eliminates >90% of dead blocks, but misses cross-module dead pure calls
  without LTO.
- AXIOM can do this at HIR level before even reaching LLVM.

**How to implement:**
1. HIR pass: if a `@pure` function call's result is unused, remove the call.
2. LLVM: `readnone`/`readonly` functions with unused results are automatically DCE'd.
3. Use `internal`/`private` linkage to enable global DCE.
4. LLVM pass: `-globaldce` removes unused global functions.

**Expected performance gain:** 2-10% (eliminates wasted computation). More impactful in
large programs with library code.
**Difficulty:** LOW.

**References:**
- Dead-code elimination: https://en.wikipedia.org/wiki/Dead-code_elimination
- LLVM GlobalDCE: https://blog.quarkslab.com/global-dead-code-elimination-for-llvm-revisited.html

---

### 7. Profile-Guided Optimization (PGO) with Built-in Benchmark Harness

**What it is:**
Using runtime profiling data to guide optimization decisions: which functions to inline,
which branches are hot/cold, how to lay out code for instruction cache efficiency.

**Why AXIOM beats C:**
- C requires a manual 3-step process: compile with instrumentation, run representative
  workload, recompile with profile data. Most C projects never do this.
- AXIOM has a **built-in benchmark harness** (`axiom-optimize/benchmark.rs`). The
  compiler can automatically: (1) instrument, (2) run benchmarks, (3) recompile with
  profile data -- all in one `axiom compile --pgo` invocation.
- PGO typically gives **5-15% improvement** on real-world code.
- BOLT (post-link optimization) can add another **5-10%** on top of PGO.

**How to implement:**
1. Use LLVM's instrumentation PGO: emit `__llvm_profile_*` calls.
2. Run the built-in benchmark harness to generate `.profdata`.
3. Feed profile back to LLVM via `-fprofile-use` equivalent APIs.
4. For BOLT: post-process the binary using `llvm-bolt`.
5. Expose as `axiom compile --pgo` and `axiom compile --pgo --bolt`.

**LLVM APIs:**
- `InstrProfIncrementInst` for instrumentation
- `llvm::createPGOInstrumentationGenPass()` / `createPGOInstrumentationUsePass()`
- Profile metadata: `!prof` branch weights

**Expected performance gain:** 5-15% (PGO) + 5-10% (BOLT) = potentially 10-25% total.
**Difficulty:** MEDIUM. LLVM has all the infrastructure; you need to wire up the
build pipeline.

**References:**
- PGO Wikipedia: https://en.wikipedia.org/wiki/Profile-guided_optimization
- LLVM PGO: https://clang.llvm.org/docs/UsersManual.html#profile-guided-optimization
- BOLT: https://github.com/llvm/llvm-project/tree/main/bolt

---

## TIER 2: MEDIUM IMPACT, MEDIUM EFFORT
*Implement these after Tier 1 is solid.*

---

### 8. Auto-Vectorization with @vectorizable Annotations

**What it is:**
Automatically converting scalar loop operations into SIMD vector operations (SSE, AVX2,
AVX-512, NEON). LLVM has a loop vectorizer and SLP vectorizer, but they are conservative
and miss many opportunities.

**Why AXIOM beats C:**
- C's vectorizer must prove no aliasing, no dependencies, and profitability -- often
  failing on one of these.
- AXIOM's `@vectorizable(dims)` tells the compiler exactly which loop dimensions are
  safe to vectorize.
- `@pure` on the loop body eliminates the dependency analysis problem.
- `@layout(row_major)` with `@align(64)` guarantees aligned, contiguous memory access.
- The compiler can emit `#pragma clang loop vectorize(enable)` equivalent metadata.
- `@constraint { precision: "relaxed" }` enables floating-point reduction vectorization
  (normally blocked by non-associativity).

**How to implement (LLVM):**
1. Emit `!llvm.loop` metadata with `llvm.loop.vectorize.enable` = true.
2. Emit `llvm.loop.vectorize.width` based on data type and target (e.g., 8 for f32
   on AVX2, 16 on AVX-512).
3. Emit `llvm.loop.interleave.count` for interleaving.
4. Emit `!llvm.access.group` metadata to prove no dependencies.
5. Use `noalias` on all pointers in the loop.
6. For reductions: emit fast-math flags to enable reassociation.
7. For gather/scatter: use `@layout` to provide stride information.

**Expected performance gain:** 2-8x for vectorizable loops (4x typical for f32 on AVX2,
8x on AVX-512). Overall program speedup depends on Amdahl's law.
**Difficulty:** MEDIUM. LLVM does the heavy lifting; AXIOM provides the metadata.

**References:**
- LLVM Vectorizers: https://llvm.org/docs/Vectorizers.html
- Intel Auto-Vectorization Guide: https://www.intel.com/content/dam/develop/external/us/en/documents/31848-compilerautovectorizationguide.pdf

---

### 9. Function Specialization / Monomorphization

**What it is:**
Creating specialized copies of functions for specific constant arguments. For example,
if `sort(data, compare_fn)` is always called with `compare_fn = less_than`, create a
`sort_less_than(data)` version where the comparison is inlined.

**Why AXIOM beats C:**
- C has no generics. `void*`-based polymorphism prevents specialization entirely.
- C++ templates do monomorphization but only for type parameters, not value parameters.
- AXIOM can specialize on **any** constant argument, including function pointers,
  strategy parameters, and enum variants.
- LLVM has a `FunctionSpecialization` pass but it only works on statically-known
  constants. AXIOM's `@const` annotations guarantee constness.
- LLVM benchmarks show **8.24% improvement on SPEC mcf** from function specialization.

**How to implement:**
1. At HIR level: when a function is called with constant arguments, clone it and
   substitute the constants.
2. Emit the specialized function with `internal` linkage.
3. Let LLVM's constant propagation and DCE clean up the specialized version.
4. Use LLVM's `FunctionSpecialization` pass as a fallback.
5. For generic AXIOM functions: always monomorphize (like Rust does).

**Expected performance gain:** 5-20% on code with polymorphic hot paths. 8% on SPEC mcf.
**Difficulty:** MEDIUM. Requires function cloning in the compiler, but straightforward.

**References:**
- LLVM FunctionSpecialization: https://github.com/llvm/llvm-project/blob/main/llvm/lib/Transforms/IPO/FunctionSpecialization.cpp
- RFC on enabling FunctionSpecialization: https://discourse.llvm.org/t/rfc-should-we-enable-function-specialization/61518

---

### 10. Memory Layout Optimization (@layout, SOA/AOS)

**What it is:**
Controlling how data structures are laid out in memory for cache efficiency. Array of
Structures (AOS) vs Structure of Arrays (SOA) can make a **40-60% performance
difference** in data-parallel code.

**Why AXIOM beats C:**
- C fixes struct layout at definition time. Changing AOS to SOA requires rewriting all
  code that touches the struct.
- AXIOM's `@layout` annotation lets the compiler choose the optimal layout:
  `@layout(soa)` converts `array[struct{x,y,z}, N]` into three arrays `x[N], y[N], z[N]`.
- `@layout(row_major)` vs `@layout(col_major)` for matrices is critical for cache
  performance and vectorization.
- Struct field reordering to minimize padding: Rust does this automatically. AXIOM can too.
- `@align(64)` ensures cache-line alignment for SIMD operations.

**How to implement:**
1. At MIR level: transform struct layouts based on `@layout` annotations.
2. SOA transformation: split struct arrays into parallel arrays.
3. Emit aligned allocations using LLVM's `align` attribute.
4. Reorder struct fields to minimize padding (smallest -> largest, with alignment groups).
5. Use `!tbaa` metadata to tell LLVM about the transformed layout.

**Expected performance gain:** 20-60% for data-parallel code with poor cache behavior.
Struct padding reduction: 5-15% memory savings.
**Difficulty:** MEDIUM-HIGH. SOA transformation is a whole-program change that must
update all access patterns. Field reordering is easy.

**References:**
- Intel Memory Layout Transformations: https://www.intel.com/content/www/us/en/developer/articles/technical/memory-layout-transformations.html
- LLVM DLO Slides: https://llvm.org/devmtg/2014-10/Slides/Prashanth-DLO.pdf

---

### 11. Loop Unrolling with Strategy Blocks

**What it is:**
Duplicating loop body iterations to reduce branch overhead, enable SIMD, and expose
instruction-level parallelism. LLVM's default unrolling is conservative (threshold 150
at -O2).

**Why AXIOM beats C:**
- C programmers can use `#pragma unroll` but rarely do, and the compiler uses fixed
  heuristics.
- AXIOM's `@strategy { unroll: ?unroll_factor }` lets the AI agent search for the
  optimal unroll factor using the built-in benchmark harness.
- The AI can profile different unroll factors (1, 2, 4, 8, 16) and pick the best one
  for the specific hardware and loop body.
- LLVM's aggressive unroll threshold at -O3 is still just a heuristic. AXIOM's
  data-driven approach is superior.

**How to implement (LLVM):**
1. Emit `!llvm.loop` metadata with `llvm.loop.unroll.count` set to the strategy value.
2. For full unroll: `llvm.loop.unroll.full` when trip count is known and small.
3. Combine with vectorization: unroll-and-jam (unroll outer loop, vectorize inner).
4. `llvm.loop.unroll.runtime.disable` to prevent runtime unroll overhead when not needed.

**Expected performance gain:** 5-30% on loop-heavy code, especially with small loop
bodies where branch overhead dominates.
**Difficulty:** LOW. Just emit LLVM metadata. The AI agent does the tuning.

**References:**
- LLVM Loop Unroll Pass: https://llvm.org/doxygen/LoopUnrollPass_8cpp_source.html
- LLVM Transform Metadata: https://llvm.org/docs/TransformMetadata.html

---

### 12. Escape Analysis from Ownership Model

**What it is:**
Determining whether allocated objects can be placed on the stack instead of the heap.
Stack allocation is essentially free (just a stack pointer adjustment) while heap
allocation requires malloc/free with lock contention and cache pollution.

**Why AXIOM beats C:**
- C doesn't have garbage collection, so escape analysis is less relevant -- but C
  programmers often heap-allocate when they could stack-allocate, and the compiler
  can't fix this.
- Java's HotSpot JVM uses escape analysis to stack-allocate objects, but it's runtime
  analysis with overhead.
- Go's compiler does static escape analysis but it's limited.
- AXIOM's explicit `@lifetime(scope|static|manual)` annotation tells the compiler
  exactly where data lives. Combined with ownership semantics, the compiler can
  guarantee stack allocation for `@lifetime(scope)` data.
- Scalar replacement: objects that don't escape can be decomposed into individual
  SSA registers.

**How to implement (LLVM):**
1. Use LLVM's `alloca` for `@lifetime(scope)` data (already on stack).
2. Run `SROA` (Scalar Replacement of Aggregates) to decompose stack structs into
   registers.
3. For dynamically-sized `@lifetime(scope)` data: use `alloca` with VLA semantics.
4. Use `lifetime.start`/`lifetime.end` intrinsics to enable stack slot reuse.
5. TinyGo uses the LLVM Attributor to move allocations to the stack -- AXIOM can do
   the same with better precision.

**Expected performance gain:** 10-40% on allocation-heavy code. Less impactful for
numerical/compute code that's already stack-allocated.
**Difficulty:** LOW-MEDIUM. Most of this is just correct IR emission.

**References:**
- Escape Analysis Wikipedia: https://en.wikipedia.org/wiki/Escape_analysis
- Go Escape Analysis: https://goperf.dev/01-common-patterns/stack-alloc/

---

### 13. Custom Pass Pipeline Ordering

**What it is:**
LLVM's default -O2/-O3 pass pipelines are tuned for C/C++. AXIOM can use a custom pass
ordering that better matches its language semantics.

**Why AXIOM beats C:**
- LLVM docs explicitly say: "these pass pipelines make a good starting point ... but
  they have been tuned for C and C++, not your target language."
- AXIOM has more guard conditions (range checks, type checks) that benefit from extra
  LoopUnswitch and LICM passes.
- AXIOM's pure functions enable more aggressive GVN (Global Value Numbering).
- Running the optimized IR back through -O2 as a sanity check can reveal missed
  opportunities.

**How to implement:**
1. Use LLVM's `PassBuilder` with `registerPipelineStartEPCallback` and other extension
   points.
2. Add extra LoopUnswitch + LICM passes for guard condition elimination.
3. Add the IRCE (Inductive Range Check Elimination) pass for range-checked loops.
4. Run aggressive interprocedural constant propagation (IPSCCP) earlier.
5. Run DeadArgElimination more aggressively.
6. Profile the pass pipeline itself to find the optimal ordering.

**Expected performance gain:** 2-10% over default pipeline.
**Difficulty:** MEDIUM. Requires experimentation and benchmarking.

**References:**
- LLVM Pass Pipeline: https://www.npopov.com/2023/04/07/LLVM-middle-end-pipeline.html
- LLVM New Pass Manager: https://llvm.org/docs/NewPassManager.html

---

### 14. Strength Reduction (Annotation-Guided)

**What it is:**
Replacing expensive operations with cheaper equivalents: multiply-by-constant ->
shift+add, divide-by-constant -> multiply-by-reciprocal, modulo-by-power-of-2 -> AND.

**Why AXIOM beats C:**
- LLVM already does most of these at -O2. BUT:
- AXIOM's range annotations enable *additional* strength reductions that LLVM misses.
  For example: if `x` is known to be in `[0, 255]`, then `x / 10` can use a simpler
  fixed-point reciprocal than the general case.
- Integer division is **50x more expensive than addition** and **10x more expensive
  than multiplication** on modern CPUs.
- `@constraint { range: 0..N }` lets the compiler pick optimal reciprocal precision.

**How to implement (LLVM):**
- LLVM's InstCombine and DAGCombine already handle most cases.
- Emit `!range` metadata from AXIOM's constraint annotations.
- LLVM's `ScalarEvolution` handles induction variable strength reduction.
- Custom AXIOM pass: detect `pow(x, 2)` -> `x * x`, `pow(x, 0.5)` -> `sqrt(x)`, etc.

**Expected performance gain:** 2-10% on code with integer division in hot loops.
Already mostly handled by LLVM; AXIOM adds marginal improvements from range info.
**Difficulty:** LOW. Mostly free from LLVM; range metadata is the AXIOM contribution.

**References:**
- Strength Reduction: https://en.wikipedia.org/wiki/Strength_reduction
- LLVM ScalarEvolution: https://llvm.org/docs/Passes.html

---

## TIER 3: HIGH IMPACT, HIGH EFFORT
*These are the heavy hitters but require significant implementation work.*

---

### 15. Polyhedral Optimization for Nested Loops (via Polly)

**What it is:**
Using the polyhedral model to mathematically represent and transform nested loop
structures. Enables: tiling, fusion, fission, skewing, interchange, and parallelization
of affine loop nests.

**Why AXIOM beats C:**
- Polly (LLVM's polyhedral optimizer) works on LLVM IR but struggles to detect valid
  "Static Control Parts" (SCoPs) from C code due to aliasing, complex control flow,
  and pointer arithmetic.
- AXIOM's `@vectorizable`, `@parallel`, and `@strategy { tiling: ... }` annotations
  explicitly mark loop nests as valid SCoPs with known bounds.
- `@pure` loop bodies guarantee no side effects, making all transformations safe.
- Polly documentation notes that annotations can "triple the performance" by enabling
  load hoisting.

**How to implement:**
1. Use Polly as an LLVM pass: `-polly` flag or programmatic pass registration.
2. Emit `!polly.access` metadata to help Polly detect memory access patterns.
3. AXIOM's tensor indexing with known bounds maps directly to polyhedral access
   functions.
4. Strategy blocks provide tiling parameters that override Polly's heuristics.
5. Alternatively: implement tiling, interchange, and fusion as AXIOM HIR/MIR passes
   before LLVM, using the annotation info.

**Expected performance gain:** 2-10x for affine loop nests (stencils, matrix ops,
convolutions). Polly claims 3x from load hoisting alone.
**Difficulty:** HIGH. Polyhedral optimization is mathematically complex, but Polly
exists as a ready-made solution.

**References:**
- Polly: https://polly.llvm.org/
- Polly paper: https://www.researchgate.net/publication/52009049_Polly-polyhedral_optimization_in_LLVM

---

### 16. SIMD Intrinsics Emission for Annotated Loops

**What it is:**
Going beyond auto-vectorization to emit specific SIMD instructions (AVX2, AVX-512)
directly. The Minotaur superoptimizer showed that LLVM misses significant SIMD
optimization opportunities.

**Why AXIOM beats C:**
- C programmers must use platform-specific intrinsics (`_mm256_add_ps`, etc.) which
  are non-portable and error-prone.
- AXIOM's `@vectorizable` + `@target(cpu.simd.avx512)` can emit optimal SIMD code
  automatically.
- Minotaur showed **7.3% mean speedup** on Intel Cascade Lake by finding SIMD
  optimizations LLVM misses, and up to **13% on GMP**.
- The key insight: LLVM's cost model is conservative. AXIOM can override it with
  annotation-guided decisions.

**How to implement:**
1. For hot loops with `@vectorizable`: emit LLVM vector intrinsics directly.
2. Use `llvm.x86.avx2.*` and `llvm.x86.avx512.*` intrinsics for target-specific ops.
3. Integrate Minotaur-style peephole optimization as a post-pass.
4. Use `@align(64)` to guarantee aligned loads/stores (enabling vmovaps instead of
   vmovups).
5. Emit masked operations for loops with non-power-of-2 trip counts (AVX-512 masking).

**Expected performance gain:** 5-15% beyond what auto-vectorization achieves.
**Difficulty:** HIGH. Requires target-specific knowledge and careful testing.

**References:**
- Minotaur: https://arxiv.org/html/2306.00229v3
- Intel Intrinsics Guide: https://www.intel.com/content/www/us/en/docs/intrinsics-guide/index.html

---

### 17. Partial Evaluation / Futamura Projections

**What it is:**
Specializing a general program based on known static inputs, producing a "residual
program" that only handles the dynamic parts. The first Futamura projection says:
specializing an interpreter for specific source code produces an executable.

**Why AXIOM beats C:**
- C has no mechanism for partial evaluation. Template metaprogramming in C++ is a
  limited approximation.
- AXIOM's `@const` parameters and `@strategy` blocks define which inputs are static,
  enabling systematic partial evaluation.
- Example: `matmul(A, B, M=1024, N=1024, K=1024)` with known dimensions can be
  partially evaluated to produce a completely unrolled, tiled, vectorized kernel.
- PyPy and GraalVM use the first Futamura projection for JIT compilation, achieving
  performance competitive with C.

**How to implement:**
1. At HIR/MIR level: when function parameters are marked `@const` and called with
   constants, clone the function and substitute.
2. Run constant propagation and DCE on the specialized version.
3. This subsumes function specialization (item 9) and comptime evaluation (item 3).
4. For strategy blocks: partially evaluate the loop nest with known tile sizes.

**Expected performance gain:** 10-100x for specific cases (interpreter specialization).
Typically 5-30% for general code.
**Difficulty:** HIGH. Full partial evaluation is a research-level problem, but
AXIOM's explicit annotations make it much more tractable.

**References:**
- Partial Evaluation: https://en.wikipedia.org/wiki/Partial_evaluation
- Futamura Projections: https://gist.github.com/fredfeng/d48dee989cc3677090ea25e17d1ca246

---

### 18. Link-Time Optimization (Full Program)

**What it is:**
Optimizing across all translation unit boundaries at link time, enabling cross-module
inlining, dead code elimination, and interprocedural constant propagation.

**Why AXIOM beats C:**
- C's LTO (via -flto) requires all compilation units to be available at link time,
  which is slow and doesn't play well with separate compilation.
- AXIOM is designed for whole-program compilation. Every module is available.
- ThinLTO provides most of LTO's benefits with better compile times.
- Full LTO: **2.86% average runtime improvement** on Clang itself.
- AXIOM can do *better* than C's LTO because `@pure` annotations enable cross-module
  optimizations that LLVM's Attributor would otherwise miss.

**How to implement:**
1. Emit LLVM bitcode for each module.
2. Use LLVM's ThinLTO infrastructure (summary-based import + parallel backends).
3. Use `internal`/`private` linkage wherever possible.
4. Run the Attributor pass with full program visibility.

**Expected performance gain:** 2-5% average, up to 10% on specific workloads.
**Difficulty:** MEDIUM. LLVM provides the infrastructure; AXIOM needs to emit bitcode
and link correctly.

**References:**
- LLVM LTO: https://llvm.org/docs/LinkTimeOptimization.html
- ThinLTO: https://clang.llvm.org/docs/ThinLTO.html
- LTO Performance: https://johnnysswlab.com/link-time-optimizations-new-way-to-do-compiler-optimizations/

---

### 19. Branch Prediction Hints from @complexity and Profiling

**What it is:**
Annotating branches with likely/unlikely hints to guide code layout and branch
prediction. Helps the CPU keep the hot path in the instruction cache.

**Why AXIOM beats C:**
- C has `__builtin_expect` (GCC) and `[[likely]]`/`[[unlikely]]` (C++20), but
  programmers rarely use them correctly.
- AXIOM's `@complexity` annotations imply branch probabilities: an O(1) early-exit
  check is likely to succeed; an O(n^2) fallback is unlikely.
- PGO data (from the built-in harness) provides *measured* branch probabilities.
- Profile metadata: `!prof !{!"branch_weights", i32 1000, i32 1}` for 1000:1 odds.

**Caution:** Research shows mixed results. On modern CPUs with hardware branch
predictors, explicit hints often don't help. **Wrong hints actively hurt performance.**
Profile-guided hints are always better than static guesses.

**How to implement (LLVM):**
1. Emit `!prof` metadata with branch weights from PGO data.
2. For `@complexity(O(1))` guard conditions: weight toward the success path.
3. Use LLVM's block placement pass to layout hot code linearly.
4. Mark error-handling paths as `cold` (function attribute or basic block metadata).

**Expected performance gain:** 0-5%. Significant only in tight loops with predictable
branch patterns. PGO-driven is much better than static hints.
**Difficulty:** LOW for static hints, MEDIUM for PGO integration.

**References:**
- Branch prediction article: https://johnnysswlab.com/how-branches-influence-the-performance-of-your-code-and-what-can-you-do-about-it/
- LWN article on likely/unlikely: https://lwn.net/Articles/420019/

---

### 20. Stencil Optimization and Tiling

**What it is:**
Optimizing stencil computations (iterating over grids where each point depends on its
neighbors) through temporal tiling, spatial tiling, and vectorization. Critical for
scientific computing, image processing, and PDE solvers.

**Why AXIOM beats C:**
- C stencil code requires manual tiling and blocking for performance.
- AXIOM's `@strategy { tiling: { ... } }` and `@vectorizable` annotations let the
  compiler (or AI agent) automatically tile and vectorize stencil loops.
- Combined with polyhedral optimization (Polly), AXIOM can apply diamond tiling,
  trapezoidal tiling, and other advanced space-time transformations.
- Stencil-specific optimizations can yield **2-10x speedups** over naive implementations.

**How to implement:**
1. Detect stencil patterns at HIR level (neighbor access patterns in loop nests).
2. Apply temporal blocking: compute multiple time steps on a tile before moving on.
3. Apply spatial tiling from `@strategy` parameters.
4. Vectorize the innermost loop.
5. Use Polly for automatic tiling when `@strategy` parameters aren't specified.

**Expected performance gain:** 2-10x on stencil code. Highly dependent on memory
bandwidth and cache hierarchy.
**Difficulty:** HIGH. Stencil optimization is a specialized domain.

**References:**
- Automatic Tiling of Iterative Stencil Loops: https://dl.acm.org/doi/10.1145/1034774.1034777
- PLUTO compiler: http://pluto-compiler.sourceforge.net/

---

## TIER 4: SPECIALIZED / EXPERIMENTAL
*Research-grade techniques with high potential but uncertain returns.*

---

### 21. Superoptimization (Souper / Minotaur Integration)

**What it is:**
Using SMT solvers to brute-force search for the shortest/fastest instruction sequence
that implements a given computation. Finds peephole optimizations that human-written
LLVM passes miss.

**Why AXIOM beats C:**
- Superoptimization is language-agnostic, but AXIOM can benefit more because:
  - `@pure` functions have simpler semantics (no side effects) -> faster SMT solving.
  - Known value ranges reduce the search space.
  - AXIOM's whole-program visibility provides more optimization opportunities.
- Souper found optimizations that reduced Clang binary size by **4.4%**.
- Minotaur found **7.3% mean speedup** on SIMD code.

**How to implement:**
1. Use Souper as an LLVM pass (it's an existing LLVM plugin).
2. Use Minotaur for SIMD-specific superoptimization.
3. Cache discovered optimizations (Redis-backed in Minotaur) so compile time is
   amortized.
4. Run superoptimization offline on hot functions identified by PGO.
5. Ship discovered rewrites as permanent compiler rules.

**Expected performance gain:** 1-7% (Souper: 1-2% runtime, 4% code size;
Minotaur: 1.5-7.3% SIMD).
**Difficulty:** HIGH. Requires SMT solver integration, long compile times.
Best used as an offline tool, not in the main compile path.

**References:**
- Souper: https://github.com/google/souper
- Minotaur: https://arxiv.org/html/2306.00229v3
- Hydra Generalization: https://users.cs.utah.edu/~regehr/generalization-oopsla24.pdf

---

## PRIORITIZED IMPLEMENTATION ROADMAP

Below is the consolidated ranking, ordered by (expected impact) / (implementation effort):

| Priority | Technique | Expected Gain | Effort | When |
|----------|-----------|---------------|--------|------|
| **P0** | Alias analysis (noalias everywhere) | 10-50% | LOW | Phase 1 codegen |
| **P0** | Pure function attrs (readnone/readonly) | 5-30% | LOW | Phase 1 codegen |
| **P0** | nsw/nuw + fast-math flags | 5-20% | LOW | Phase 1 codegen |
| **P0** | Dead code elimination (@pure) | 2-10% | LOW | Phase 1 codegen |
| **P0** | Aggressive inlining hints | 5-30% | LOW | Phase 1 codegen |
| **P0** | Strength reduction (range metadata) | 2-10% | LOW | Phase 1 codegen |
| **P1** | Loop unrolling (strategy-driven) | 5-30% | LOW | Phase 2 |
| **P1** | Compile-time evaluation (@const) | 5-40% | MEDIUM | Phase 2 |
| **P1** | Custom LLVM pass pipeline | 2-10% | MEDIUM | Phase 2 |
| **P1** | Profile-guided optimization | 10-25% | MEDIUM | Phase 2 |
| **P1** | Escape analysis / stack alloc | 10-40% | MEDIUM | Phase 2 |
| **P2** | Auto-vectorization hints | 2-8x loops | MEDIUM | Phase 3 |
| **P2** | Function specialization | 5-20% | MEDIUM | Phase 3 |
| **P2** | Memory layout (SOA/AOS) | 20-60% | MED-HIGH | Phase 3 |
| **P2** | LTO (whole program) | 2-10% | MEDIUM | Phase 3 |
| **P3** | Polyhedral optimization (Polly) | 2-10x loops | HIGH | Phase 4 |
| **P3** | SIMD intrinsics emission | 5-15% | HIGH | Phase 4 |
| **P3** | Branch prediction hints | 0-5% | LOW-MED | Phase 4 |
| **P3** | Stencil optimization | 2-10x | HIGH | Phase 4 |
| **P4** | Partial evaluation | 5-100x | HIGH | Phase 5 |
| **P4** | Superoptimization | 1-7% | HIGH | Phase 5 |

---

## KEY INSIGHT: THE COMPOUNDING EFFECT

These optimizations are not additive -- they **compound**. Each optimization enables
others:

1. **noalias** enables vectorization (no alias checks needed)
2. **readnone** enables CSE, which enables constant folding
3. **Constant folding** enables dead code elimination
4. **Loop unrolling** enables vectorization of more patterns
5. **PGO** improves inlining decisions, which enables everything else
6. **LTO** enables cross-module inlining, which enables everything else

A language that provides ALL of these simultaneously (as AXIOM does through its
annotation system) gets a multiplicative benefit that no single optimization provides
alone.

**The realistic target: AXIOM-compiled code should be 1.2-2x faster than clang -O2
on compute-heavy benchmarks**, with specific cases (numerical loops with poor cache
behavior) seeing 5-10x improvements.

The floor is matching clang -O2 (by emitting good LLVM IR with rich annotations).
The ceiling is matching or exceeding hand-optimized C with intrinsics (by combining
auto-vectorization, PGO, polyhedral optimization, and superoptimization).

---

## WHAT AXIOM GETS "FOR FREE" FROM LLVM

Even before any AXIOM-specific optimization, emitting good LLVM IR with the right
annotations gives you these LLVM passes automatically:

- **InstCombine**: algebraic simplifications, strength reduction
- **GVN**: common subexpression elimination, redundant load elimination
- **SROA**: scalar replacement of aggregates (stack structs -> registers)
- **Mem2Reg**: alloca promotion to SSA registers
- **SimplifyCFG**: branch simplification, dead block elimination
- **LICM**: loop invariant code motion
- **LoopVectorize**: automatic vectorization (if aliasing info is provided)
- **SLPVectorize**: straight-line parallelism vectorization
- **JumpThreading**: threading of jumps through conditional blocks
- **CorrelatedValuePropagation**: range-based optimization
- **ADCE**: aggressive dead code elimination
- **TailCallElim**: tail call optimization
- **LoopIdiom**: recognizes memset/memcpy patterns
- **LoopDeletion**: removes loops with no side effects

The key is feeding LLVM the **right metadata**. Without `noalias`, `readnone`,
`nsw/nuw`, `!tbaa`, `!range`, and `!prof` metadata, these passes are crippled.
With them, they approach optimal.
