# Self-Optimization Pass 1 — Learnings

**Date:** 2026-03-24
**Result:** 0.97x → 0.94x (AXIOM 6% faster than C turbo overall)

## What Worked

### Biggest wins:
1. **MD5: C-winning → AXIOM 14% faster** — `@inline(always)` on 5 round functions eliminated 640,000 function calls (64 rounds × 10,000 blocks). LLVM inlined everything into a single tight loop.

2. **conjugate_gradient: 1.16x → 1.03x** — Extracting `cg_iterate` and 5 helpers as `@pure` enabled `fast` flags on ALL float math in the CG solver. The inner dot product now uses FMA.

3. **fluid_sim: 1.12x → 1.04x** — Same pattern: extracting collision/streaming into `@pure` functions. The D2Q9 collision kernel has heavy float math that benefits massively from FMA.

4. **sparse_matrix: 1.10x → 0.98x (tie!)** — SpMV inner loop got `fmul fast + fadd fast` after extraction. Now vectorizes with `vfmadd132pd`.

5. **lru_cache: 1.10x → 0.95x (tie!)** — heap_alloc_zeroed eliminated 200KB of memset.

6. **huffman: 1.15x → 1.05x (tie!)** — heap for data array + `@inline(always)` on lcg.

### What DIDN'T fully close the gap:

1. **eigenvalue (still 1.41x)** — Despite @pure + heap + ptr params, AXIOM generates slightly different loop structure. The `power_iteration` function call may prevent loop fusion. C inlines everything into main.

2. **ray_tracer (still 1.18x)** — C gets 5x more vectorized instructions. The per-pixel ray tracing doesn't vectorize well in AXIOM because each pixel has independent branching (ray-sphere intersection tests). C's `-ffast-math` + aggressive unrolling enables more SIMD.

## New Knowledge Base Rules Discovered

### Rule 11: Array-by-value params cause hidden copies
When a `@pure fn f(arr: array[f64, N])` is called, the entire array is COPIED onto the callee's stack. For large arrays this is catastrophic. Always use `ptr[f64]` params for arrays > 1KB.

### Rule 12: @inline(always) on round functions in crypto
MD5/SHA-256 round functions are called 64 times per block. At 10K+ blocks, function call overhead dominates. `@inline(always)` eliminates this entirely and enables LLVM to optimize across round boundaries.

### Rule 13: Ray tracing doesn't vectorize well across pixels
Per-pixel ray tracing has data-dependent branches (early exit on intersection, shadow rays). This prevents SIMD across pixels. C's advantage here is from aggressive scalar optimization under `-ffast-math`, not vectorization. Consider: batch rays into coherent groups (ray packets) for future SIMD optimization.

## Remaining Opportunity

- eigenvalue: try `@inline(always)` on `matvec_multiply` to enable loop fusion
- ray_tracer: approximate sqrt with fast inverse sqrt, or batch coherent rays
- Both: investigate if AXIOM's `fastcc` calling convention causes different register allocation
