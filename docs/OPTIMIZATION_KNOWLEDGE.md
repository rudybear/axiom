# AXIOM Optimization Knowledge Base

**This file is read by the LLM optimizer before every optimization pass and updated after discoveries. It is the accumulated wisdom of all optimization sessions.**

Last updated: 2026-03-28

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

## Rule 5: Use heap_alloc_zeroed (calloc) for large arrays — NOT stack arrays

**Discovery:** Real-world benchmark analysis (2026-03-24)
**Impact:** 8-15x improvement on programs with large arrays
**Evidence:**
- median_filter: 40.4ms → 4.7ms (8.6x faster!) after switching from stack to calloc
- run_length_encode: 74.7ms → 5.1ms (14.7x faster!) after switching
- Both now BEAT C turbo (-O3 -march=native -ffast-math) by 6-8%

**Why:** `array_zeros[T, N]` generates `alloca + memset` which zeroes memory byte-by-byte. For 2MB arrays this takes ~1ms per array. `heap_alloc_zeroed` maps to `calloc` which on modern OSes gets already-zeroed pages from the kernel via `mmap`/`VirtualAlloc` — effectively FREE zeroing for large allocations (>4KB). This is the same mechanism that makes C's `static` global arrays zero-cost.

**Pattern:**
```axiom
// BAD: 2MB stack array — expensive memset at function entry
let data: array[i32, 500000] = array_zeros[i32, 500000];

// GOOD: calloc — OS provides pre-zeroed pages for free
let data: ptr[i32] = heap_alloc_zeroed(500000, 4);
// ... use ptr_read_i32/ptr_write_i32 instead of data[i] ...
heap_free(data);

// ALSO GOOD: arena for batch patterns
let arena: ptr[i32] = arena_create(2097152);
let data: ptr[i32] = arena_alloc(arena, 500000, 4);
```

**When to apply:** ANY array larger than 4KB (4096 bytes). Stack arrays are fine for small fixed data (sort window, lookup table). For large data, always use `heap_alloc_zeroed`.

**Combine with @inline(always):** Small helper functions (LCG, min, max) should be `@inline(always)` to avoid function call overhead in tight loops.

---

## Rule 5b: @inline(always) on small hot helpers

**Discovery:** median_filter + RLE optimization (2026-03-24)
**Impact:** Eliminates function call overhead, enables cross-function optimization
**Evidence:** LCG function was called 262144 times per frame — inlining removed call/ret overhead AND enabled LLVM to optimize the LCG state into registers.

**Pattern:**
```axiom
@pure @inline(always)
fn lcg_next(seed: i64) -> i64 {
    return (1103515245 * seed + 12345) % 2147483648;
}
```

**When to apply:** Any @pure helper < 10 lines that's called in a hot loop.

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

### Anti-Pattern 6: Trusting agent reports without verification
The LLM optimization agent may report "fixed N files" but only actually modify M < N. In self-opt pass 1, the agent claimed to convert stack arrays to heap in 9 benchmarks — but 4 benchmarks (edge_detection, lz77, fft, finite_element) were NOT actually changed.

**Always run `verify-optimization.sh` after optimization passes.** The script checks:
1. Large stack arrays that should be heap (Rule 5)
2. Missing @pure on helper functions (Rule 1)
3. Missing @constraint annotations (Rule 2)
4. LLVM IR attributes match expectations

**Pipeline integration:** The QA agent now runs this script automatically and rejects if issues are found.

---

## Rule 15: Always verify optimization changes persisted

**Discovery:** Self-improvement pass 3 (2026-03-25)
**Impact:** Prevents false "optimization complete" status
**Evidence:** 4 of 9 benchmarks in self-opt pass 1 were not actually modified despite agent reporting success.

**Verification command:**
```bash
bash .pipeline/scripts/verify-optimization.sh benchmarks/real_world/
```

**When to apply:** After EVERY optimization pass. Before accepting any commit that claims performance improvements.

---

## Rule 11: AOS with vec3 fields beats SOA when accessing all fields per entity

**Pattern:** `vec3(ptr_read_f64(data, i+0), ptr_read_f64(data, i+1), ptr_read_f64(data, i+2))` —
three sequential reads reconstructing a vector from flat memory.

**When to apply:** When code processes one entity's full state at a time (raytracers, physics engines,
particle systems). AOS with vec3 fields eliminates reconstruction overhead.

**When SOA wins instead:** When iterating one property across ALL entities (e.g., updating all positions,
then all velocities). In this case, SOA enables SIMD lane packing across entities.

**AXIOM-specific:** Use vec3 struct fields for AOS: `struct Sphere { center: vec3, radius: f64 }`.
The vec3 type is SIMD-aligned and loads as a single `<4 x double>` instruction.

---

## How This File Is Used

1. **Before optimization:** The LLM optimizer reads this file as part of the prompt context
2. **During analysis:** The LLM checks if any known rules/anti-patterns apply to the target code
3. **After discovery:** New rules are appended when the LLM finds novel optimization patterns
4. **Format:** Each rule has Discovery (when), Impact (how much), Evidence (data), Pattern (code), When to apply (guidance)

## Rule 12: AXIOM codegen generates verbose IR — use @inline(always) aggressively

**Pattern:** AXIOM's codegen puts every variable in an alloca and every ptr_read/write
is an explicit GEP+load/store. This generates ~5x more IR instructions than equivalent C.
LLVM's SROA/mem2reg at -O2 eliminates most allocas, but function boundaries block
this optimization.

**When to apply:** Always add `@inline(always)` to functions in the hot path that:
- Are called in tight loops (compression inner loops, crypto rounds, hash processing)
- Take ptr parameters that hold constant addresses (lookup tables)
- Are small-to-medium size (<100 lines)

**Why it helps:** Inlining eliminates parameter allocas and lets LLVM see through the
entire computation chain. This enables constant propagation of pointer addresses,
dead store elimination, and register promotion that can't happen across function
boundaries.

**Measured impact:** LZAV compression: 129ms→115ms (-11%) just from adding @inline(always)
to compress/decompress. AES-128: closing 3.7x gap to 1.04x required inlining + const
propagation.

**Anti-pattern:** Don't inline very large functions (>200 lines) or recursive functions —
this causes code bloat and icache pressure.

## Rule 13: Use u8 storage for byte-level algorithms, not i32

**Pattern:** Crypto/compression algorithms operate on bytes. Using i32 (4 bytes) per
byte value wastes 75% of cache capacity. AES S-box: 256 bytes (u8) vs 1024 bytes (i32).

**When to apply:** Any algorithm that processes byte streams — AES, compression, hashing.
Use `array_const_u8(...)` for lookup tables, `ptr_read_u8`/`ptr_write_u8` for byte access,
`heap_alloc(n, 1)` for byte buffers.

**Measured impact:** AES-128: 312ms→280ms (-10%) just from i32→u8 storage.

## Rule 14: Wrapping arithmetic (+%, *%) is mandatory for hash/crypto

**Pattern:** Hash functions and encryption rely on integer overflow wrapping. AXIOM's
default `+` uses `nsw` (no signed wrap) which is UB on overflow. Use `+%` (wrapping add)
and `*%` (wrapping multiply) for ALL arithmetic in hash/crypto code.

**When to apply:** Any code computing hashes (SipHash, xxHash, CRC), encryption (AES key
schedule), or checksums.

**Anti-pattern:** Never use `+` or `*` in hash functions — the nsw flag allows LLVM to
assume overflow doesn't happen, which can silently produce wrong results.

## Design Decision: No >> or << infix operators

AXIOM deliberately does NOT support `>>` and `<<` as infix operators.
All bitwise operations use explicit function calls: `shr()`, `lshr()`, `shl()`.

**Why:** AXIOM is AI-first. AI agents don't benefit from C syntax sugar.
Explicit function calls are:
- **Unambiguous**: `shr(x, 4)` is always arithmetic shift right. `lshr(x, 4)` is always logical.
  In C, `x >> 4` depends on whether `x` is signed or unsigned — a source of bugs.
- **Searchable**: An AI can grep for `shr(` or `lshr(` precisely.
- **Consistent**: All bitwise ops are functions: `band`, `bor`, `bxor`, `shl`, `shr`, `lshr`, `rotl`, `rotr`.
  Adding `>>` as an operator would break this consistency.

The `shr()` vs `lshr()` distinction is AXIOM's answer to C's type-dependent `>>` behavior.
For unsigned types (u32), use `lshr()`. For signed types (i32), use `shr()`.
