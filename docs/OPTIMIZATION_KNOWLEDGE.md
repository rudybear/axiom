# AXIOM Optimization Knowledge Base

**This file is read by the LLM optimizer before every optimization pass and updated after discoveries. It is the accumulated wisdom of all optimization sessions.**

Last updated: 2026-03-24

---

## Rule 1: Extract hot loops into @pure functions

**Discovery:** Phase E benchmarks (2026-03-24)
**Impact:** 2x-10x improvement on float-heavy code
**Evidence:**
- JPEG DCT: 1.69x slower → 0.43x (57% faster than C) after extraction
- Finite element: 8.76x → 1.19x after extraction
- Eigenvalue power method: 3.13x → 1.33x after extraction

**Why:** `@pure` functions get LLVM attributes `memory(none)` or `memory(argmem: read/readwrite)` + `fast` flag on ALL float operations. Without `@pure`, float ops use strict IEEE 754 semantics — no FMA, no reassociation, no vectorization of reductions. The `fast` flag enables `vfmadd` (fused multiply-add), loop vectorization with reduction, and algebraic simplification.

**Pattern:**
```axiom
// BAD: hot loop in main() — no fast-math
fn main() -> i32 {
    for i: i32 in range(0, n) {
        for j: i32 in range(0, n) {
            sum = sum + A[i*n+j] * x[j];  // fadd, fmul — no FMA
        }
    }
}

// GOOD: extract into @pure helper — gets fast-math
@pure
fn matvec(A: ptr[f64], x: ptr[f64], out: ptr[f64], n: i32) {
    for i: i32 in range(0, n) {
        let sum: f64 = 0.0;
        for j: i32 in range(0, n) {
            sum = sum + ptr_read_f64(A, i*n+j) * ptr_read_f64(x, j);  // fadd fast, fmul fast → FMA!
        }
        ptr_write_f64(out, i, sum);
    }
}
```

**When to apply:** Always, when a function does float arithmetic and has no observable side effects beyond writing to its output parameters.

---

## Rule 2: Use @constraint { optimize_for: "performance" } for -O3

**Discovery:** Phase C implementation (2026-03-24)
**Impact:** 5-20% on compute-bound code
**Evidence:** Default is -O2. Adding `@constraint { optimize_for: "performance" }` switches to -O3, enabling more aggressive inlining, loop unrolling, and vectorization.

**Pattern:**
```axiom
@module my_program;
@constraint { optimize_for: "performance" };
```

**When to apply:** Always for benchmarks and compute-heavy programs. Use `"memory"` for memory-constrained or `"size"` for code-size-sensitive deployments.

---

## Rule 3: -march=native enables AVX2/FMA automatically

**Discovery:** Phase C implementation (2026-03-24)
**Impact:** 2x-4x on vectorizable loops (AVX2 processes 4 doubles at once)
**Evidence:** Eigenvalue matvec went from scalar `mulsd`/`addsd` to vectorized `vfmadd132pd` (4x throughput)

**Pattern:** Use `--target=native` flag or it's the default now.

**When to apply:** Always for local benchmarking. Use specific targets (`x86-64-v3`, `x86-64-v4`) for portable binaries.

---

## Rule 4: @pure on read-only pointer functions enables noalias + readonly

**Discovery:** MT-1 soundness fix (2026-03-24)
**Impact:** Enables load hoisting, CSE, vectorization across pointer accesses
**Evidence:** The body scanner distinguishes read-only vs read-write `@pure` functions:
- Read-only → `memory(argmem: read) nounwind` — LLVM can reorder loads, eliminate redundant loads
- Read-write → `memory(argmem: readwrite) nounwind` — LLVM can still optimize but more conservatively

**Pattern:**
```axiom
// Gets memory(argmem: read) — maximum optimization
@pure
fn dot_product(a: ptr[f64], b: ptr[f64], n: i32) -> f64 { ... }

// Gets memory(argmem: readwrite) — still good but less aggressive
@pure
fn normalize(v: ptr[f64], n: i32) { ... }  // writes to v
```

**When to apply:** Mark every function that doesn't call print/file/thread builtins as `@pure`.

---

## Rule 5: Avoid large stack arrays in hot paths

**Discovery:** Real-world benchmark analysis (2026-03-24)
**Impact:** Stack arrays >64KB cause performance issues
**Evidence:** Eigenvalue benchmark with `array[f64, 250000]` (2MB stack) was 3x slower. The alloca + memset overhead is significant, and large stack frames cause TLB pressure.

**Pattern:**
```axiom
// BAD: 2MB on stack
let A: array[f64, 250000] = array_zeros[f64, 250000];

// BETTER: heap-allocated
let A: ptr[f64] = heap_alloc(250000, 8);
// ... use ...
heap_free(A);

// BEST: arena-allocated (if lifetime is known)
let arena: ptr[i32] = arena_create(2097152);
let A: ptr[f64] = arena_alloc(arena, 250000, 8);
```

**When to apply:** Use heap or arena for arrays > 64KB. Stack arrays are fine for small, fixed-size data (< 4KB).

---

## Rule 6: Integer division (divl) is expensive — minimize it

**Discovery:** LLM optimization of prime counting (2026-03-24)
**Impact:** 37% speedup on prime counting
**Evidence:** `divl` takes ~25 cycles on x86. The LLM identified this in the assembly and suggested 6k±1 wheel factorization, reducing divisions by 33%.

**Pattern:** When you see heavy use of `%` (modulo) or `/` (division) in a loop, consider:
- Strength reduction: `n % 2 == 0` → `band(n, 1) == 0` (1 cycle vs 25)
- Algorithm change: trial division step=2 → 6k±1 wheel (33% fewer divisions)
- Precomputation: compute divisors once, store in array, iterate

**When to apply:** Any loop with integer division in the hot path. Check the assembly for `divl`/`idivl` instructions.

---

## Rule 7: Arena allocator for batch allocation patterns

**Discovery:** Memory benchmarks (2026-03-22)
**Impact:** 80% faster than malloc on binary trees
**Evidence:** Binary trees (Benchmarks Game classic): AXIOM arena 0.18s vs C malloc 0.92s = 5x faster

**Pattern:**
```axiom
let arena: ptr[i32] = arena_create(8388608);  // 8MB
// Allocate many small objects — each is just a pointer bump (~2ns)
for i: i32 in range(0, 100000) {
    let node: ptr[i32] = arena_alloc(arena, 3, 4);  // 3 ints per node
}
// Free ALL at once — O(1)
arena_reset(arena);
```

**When to apply:** Tree construction, graph building, particle systems, per-frame scratch data — anywhere you allocate many objects and free them all together.

---

## Rule 8: readonly_ptr/writeonly_ptr for pointer access direction

**Discovery:** MT-4 ownership slices (2026-03-24)
**Impact:** Enables LLVM readonly/writeonly parameter attributes → better alias analysis
**Evidence:** `readonly_ptr[T]` gets `ptr noalias readonly` which tells LLVM the function won't modify through this pointer, enabling more aggressive load reordering.

**Pattern:**
```axiom
@pure
fn sum(data: readonly_ptr[f64], n: i32) -> f64 { ... }

fn fill(output: writeonly_ptr[f64], n: i32) { ... }
```

**When to apply:** Always annotate pointer params with the correct access direction. It costs nothing and helps the optimizer.

---

## Rule 9: Beware of codegen patterns that prevent vectorization

**Discovery:** Benchmark analysis (2026-03-24)
**Impact:** 2x-8x difference between vectorized and scalar loops
**Evidence:** median_filter_3x3 is 8x slower — the sorting network pattern doesn't vectorize. C's equivalent code with `-ffast-math` doesn't vectorize either, but C's scalar codegen is more efficient.

**Known non-vectorizing patterns in AXIOM:**
- Sorting networks (compare-and-swap chains) — data-dependent branches prevent SIMD
- While loops with non-trivial exit conditions — only for loops with range() vectorize reliably
- Array index computations with complex expressions — GEP chains may confuse the vectorizer
- Mixed i32/i64 operations — type conversions break SIMD lanes

**When to apply:** If a loop is slow, check the assembly. If you see scalar instructions (`mulsd`, `addsd`) where you expect vector (`vmulpd`, `vaddpd`), the loop isn't vectorizing. Consider restructuring.

---

## Rule 10: The fence pattern for parallel regions

**Discovery:** MT-1 soundness fix (2026-03-24)
**Impact:** Correctness, not performance
**Evidence:** Without fences, worker thread stores may not be visible to the main thread.

**Pattern:**
```
fence release          ← before dispatching work (flush main thread writes)
job_dispatch(...)
job_wait()
fence acquire          ← after waiting (ensure worker writes visible)
```

**When to apply:** Always around parallel regions. The AXIOM runtime handles this automatically for `job_dispatch`/`job_wait`, but if using raw threads + atomics, add explicit fences.

---

## Anti-Patterns (Things That Make AXIOM Slower)

### Anti-Pattern 1: Float arithmetic in non-@pure functions
Float ops in `main()` or non-@pure functions get strict IEEE 754 semantics. No FMA, no vectorization. Always extract into @pure.

### Anti-Pattern 2: Large stack arrays (>64KB)
Causes expensive memset on every function call. Use heap or arena instead.

### Anti-Pattern 3: Mixing i32 and i64 in tight loops
Type conversions (sext/trunc) break SIMD lanes. Keep loop variables and array indices the same width.

### Anti-Pattern 4: Simulating bitwise ops with arithmetic
Before bitwise builtins existed, some benchmarks used `x % 256` for `x & 0xFF`. Now use native `band(x, 255)` — 25x faster for modulo-power-of-2.

### Anti-Pattern 5: Not using @pure on obviously pure functions
If a function only reads params and returns a value, mark it `@pure`. There's no reason not to.

---

## How This File Is Used

1. **Before optimization:** The LLM optimizer reads this file as part of the prompt context
2. **During analysis:** The LLM checks if any known rules/anti-patterns apply to the target code
3. **After discovery:** New rules are appended when the LLM finds novel optimization patterns
4. **Format:** Each rule has Discovery (when), Impact (how much), Evidence (data), Pattern (code), When to apply (guidance)
