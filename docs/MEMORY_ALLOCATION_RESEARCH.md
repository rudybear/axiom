# AXIOM Memory Allocation Research: Beating C's malloc/free

## Executive Summary

C programs use `malloc`/`free` for heap allocation -- a general-purpose system that must
handle arbitrary allocation patterns, sizes, and lifetimes with zero compile-time
information. AXIOM has a decisive advantage: **the compiler knows allocation sizes (from
the type system), lifetimes (from `@lifetime` annotations), purity (from `@pure`), memory
layout (from `@layout`), and access patterns (from `@strategy`)**. This information
enables allocation strategies that are provably impossible for a C compiler to apply
automatically.

**Current state:** AXIOM has only stack-allocated arrays (`array[T, N]` via `alloca`).
It needs heap allocation for dynamic sizes, long-lived data, and data that escapes
function scope.

**Goal:** Implement heap allocation that is measurably faster than C's malloc/free for
every workload where AXIOM has compile-time information advantages.

**Key insight:** There is no single "best" allocator. The optimal strategy depends on the
allocation pattern. AXIOM's annotations let the compiler **select the optimal allocator
per call site** -- something C programmers must do manually and rarely bother with.

---

## PRIORITY RANKING (by impact-to-effort ratio)

| Rank | Technique | Expected Speedup | Effort | Section |
|------|-----------|-----------------|--------|---------|
| 1 | Bump/arena allocation | 50-200x vs malloc for batch patterns | Medium | S1 |
| 2 | Escape analysis + stack promotion | 2-10x for non-escaping allocs | Medium | S2 |
| 3 | Link against mimalloc | 1.5-2x vs system malloc, free | Easy | S3 |
| 4 | Compile-time size specialization | 1.3-2x for known-size allocs | Easy | S4 |
| 5 | Pool allocation (fixed-size) | 3-7x for uniform-size patterns | Medium | S5 |
| 6 | Stack alloc with heap fallback | 2-5x for small dynamic arrays | Easy | S6 |
| 7 | Region-based management | 5-20x for scoped patterns | Hard | S7 |
| 8 | Zero-copy view semantics | Infinite (eliminates copies) | Easy | S8 |
| 9 | LLVM stacksave/stackrestore | 2-5x for loop-scoped allocs | Easy | S9 |
| 10 | Slab allocation | 2-5x for object-heavy code | Medium | S10 |
| 11 | Huge pages | 1.1-1.3x for large allocations | Easy | S11 |
| 12 | Custom page management (mmap/VA) | 1.5-3x for large sparse data | Medium | S12 |
| 13 | Recycling allocators | 2-7x for alloc/free cycles | Medium | S13 |
| 14 | SIMD memset/memcpy | 2-3x for array initialization | Easy | S14 |
| 15 | Non-temporal stores | 1.2-2x for streaming writes | Easy | S15 |
| 16 | Prefetching | 1.1-1.5x (up to 50% peak) | Easy | S16 |
| 17 | Lifetime-driven pools | 3-10x for known-lifetime allocs | Hard | S17 |
| 18 | LLVM allocator attributes | 1.2-2x (enables LLVM opts) | Easy | S18 |
| 19 | Memory-mapped lazy alloc | Near-zero cost until touched | Medium | S19 |
| 20 | Write-combining buffers | 1.2-1.5x for sequential init | Easy | S20 |

---

## S1: BUMP / ARENA ALLOCATION

### What it is

A bump allocator maintains a pointer into a pre-allocated buffer. Each allocation simply
increments the pointer by the requested size. Deallocation is not supported individually
-- the entire arena is freed at once by resetting the pointer. This is the fastest
possible general allocator: allocation is a single pointer addition.

### Performance data

- **200x+ faster than GNU malloc** for allocation-heavy workloads (Hacker News benchmark,
  2025: C++ arena allocator vs glibc malloc).
- **Allocation: ~2 ns** (pointer bump) vs **~26 ns** (malloc on RPi 4B, measured by
  Feilbach 2025). Deallocation: **single integer write** vs **~31 ns** (free).
- **LLVM's own BumpPtrAllocator** uses this strategy internally for all compiler IR
  allocation. Default slab size: 4096 bytes, grows by allocating new slabs.
- **Game engines** use per-frame arenas: allocate millions of objects, free all at frame
  end. 16ms frame budget is easily met (Arena allocator blog, Fleury 2023).
- **In aggregate benchmarks**: malloc/free heap took 26,882ms while obstack arena took
  272ms -- roughly **100x faster** for 1-thread allocation-heavy workloads (obstack
  benchmark, cleeus/obstack).

### How AXIOM's annotations enable it better than C

AXIOM's `@lifetime(scope)` annotation tells the compiler that all allocations in a scope
die together. AXIOM's `@pure` annotation proves no external references escape. Together,
they prove arena-safety at compile time. C programmers must manually decide to use arenas
and restructure their code accordingly.

Proposed AXIOM syntax:
```axiom
@lifetime(scope)
fn process_frame(data: slice[f32]) -> f32 {
    // All allocations here use a bump allocator
    // Arena is freed when function returns
    let temp: array[f32, ?n] = allocate(n);  // bump alloc, not malloc
    ...
}
```

Or explicit arena annotation:
```axiom
@arena(frame_arena)
{
    let particles: slice[Particle] = arena_alloc(1000);
    let physics: slice[Vec3] = arena_alloc(1000);
    // both freed when block exits
}
```

### LLVM implementation approach

**Option A -- Inline bump pointer (fastest, for small arenas):**
```llvm
; Arena state: base pointer + current offset
%arena.base = call ptr @llvm.stacksave()  ; or mmap'd region
%arena.offset = alloca i64
store i64 0, ptr %arena.offset

; Bump allocation (replaces malloc call):
%off = load i64, ptr %arena.offset
%ptr = getelementptr i8, ptr %arena.base, i64 %off
%new_off = add i64 %off, 64  ; allocation size
store i64 %new_off, ptr %arena.offset

; Deallocation (replaces all free calls):
call void @llvm.stackrestore(ptr %arena.base)  ; or munmap
```

**Option B -- Runtime arena library (for large/dynamic arenas):**
Emit calls to a small runtime: `axiom_arena_create(size_hint)`,
`axiom_arena_alloc(arena, size, align)`, `axiom_arena_destroy(arena)`.
The runtime uses mmap/VirtualAlloc for backing memory with slab growth.

**Option C -- LLVM BumpPtrAllocator pattern:**
Pre-allocate slabs (default 4KB, growing). When a slab fills, allocate a new one.
Track slabs in a vector. On arena destroy, free all slabs.

### Implementation difficulty: MEDIUM

Requires: new annotation (`@arena` or `@lifetime(scope)`), arena codegen path in
`axiom-codegen/src/llvm.rs`, small runtime library for slab management.

---

## S2: ESCAPE ANALYSIS + STACK PROMOTION

### What it is

If a heap allocation does not "escape" its function (i.e., the pointer is never stored
to a global, returned, or passed to an opaque function), the allocation can be replaced
with a stack allocation (`alloca`). This eliminates malloc/free entirely.

### Performance data

- **Go 1.25/1.26**: Escape analysis eliminates heap allocations for slices under 32 bytes.
  64KB boundary: allocations at 65535 bytes are stack-allocated; at 65536 they go to heap
  -- producing **10,000x speedup** on repeated allocation benchmarks (Go blog 2025).
- **V8 JavaScript**: Disabling escape analysis causes **31.5% slowdown** on RayTrace
  benchmark. Average impact: 3-6% across Octane suite (kipply's blog).
- **LuaJIT**: Allocation sinking (escape-analysis-based) makes LuaJIT **700x faster than
  Lua** and on par with C++ for struct arithmetic (kipply's blog).
- **LLVM HeapToStack pass**: Achieves approximately **2x speedup** by promoting malloc to
  alloca, with secondary benefits from exposing further optimization opportunities
  (CMU CS 15-745 report; LLVM D1745 review).
- **Java HotSpot**: Escape analysis enables scalar replacement of objects, avoiding heap
  allocation for short-lived objects (Wikipedia).

### How AXIOM's annotations enable it better than C

**This is AXIOM's killer advantage.** In C, LLVM must perform expensive interprocedural
escape analysis with conservative assumptions. In AXIOM:

- `@pure` functions **cannot** capture pointers (no side effects by definition). Any
  allocation inside a `@pure` function that is not returned is trivially non-escaping.
- `@lifetime(scope)` explicitly declares that data dies at scope exit.
- `@const` functions are evaluated at compile time -- no runtime allocation needed at all.
- Explicit types (`array[T, N]`) with compile-time-known N can always be stack-allocated.

The compiler can promote to stack with **zero analysis** when annotations are present:
```axiom
@pure
fn compute(n: i32) -> f64 {
    let buffer: array[f64, 256] = array_zeros[f64, 256];  // always stack
    // @pure guarantees buffer cannot escape
    return buffer[0];
}
```

### LLVM implementation approach

1. During HIR-to-LLVM lowering, check if function is `@pure` or `@lifetime(scope)`.
2. For any `allocate()` call inside such a function where the result is not returned:
   - Replace `call ptr @malloc(i64 %size)` with `%buf = alloca i8, i64 %size`
   - Remove corresponding `call void @free(ptr %buf)`
3. For dynamic sizes that might overflow the stack, emit a size check:
   ```llvm
   %fits = icmp ult i64 %size, 65536  ; 64KB threshold
   br i1 %fits, label %stack_path, label %heap_path
   stack_path:
     %buf = alloca i8, i64 %size
     br label %use
   heap_path:
     %buf2 = call ptr @axiom_alloc(i64 %size)
     br label %use
   ```
4. Additionally, mark AXIOM's allocation functions with LLVM's `allockind` and
   `alloc-family` attributes so LLVM's own heap-to-stack passes can work too.

### Implementation difficulty: MEDIUM

Requires: escape analysis logic during codegen (simplified by `@pure`), size threshold
constant, conditional codegen for stack/heap split.

---

## S3: LINK AGAINST mimalloc AS DEFAULT ALLOCATOR

### What it is

mimalloc is Microsoft Research's general-purpose allocator that consistently outperforms
jemalloc, tcmalloc, and system malloc across diverse workloads. It uses free-list sharding
with per-page free lists for excellent cache locality and low contention.

### Performance data

- **mimalloc vs glibc malloc**: 1.67x faster on mimalloc-bench aggregate (Exgen-Malloc
  paper, 2025).
- **mimalloc vs jemalloc**: 1.41x faster on mimalloc-bench aggregate.
- **mimalloc vs tcmalloc**: 1.15x faster on mimalloc-bench aggregate.
- **Redis**: 7% faster than tcmalloc, 14% faster than jemalloc (mimalloc paper).
- **Lean theorem prover**: 13% overall speedup vs tcmalloc -- implying mimalloc is
  **1.6x faster** in allocator-bound portions (mimalloc tech report).
- **Memory usage**: mimalloc uses less memory than jemalloc (25.2% less) and tcmalloc
  (34.7% less) on mimalloc-bench (Exgen-Malloc paper).
- **SPEC CPU2017**: 1.05-1.09x faster than glibc across integer benchmarks.

### How AXIOM's annotations enable it better than C

AXIOM can go beyond just linking mimalloc:
- For `@pure` functions with known allocation patterns, bypass mimalloc entirely and use
  arenas (S1) or stack promotion (S2). mimalloc becomes the **fallback** allocator.
- AXIOM's compile-time-known sizes let it use mimalloc's size-class-specific APIs
  (`mi_malloc_small`, `mi_zalloc_small`) directly, skipping size-class lookup.
- AXIOM can batch-allocate from mimalloc for arena backing slabs.

### LLVM implementation approach

This is the simplest possible improvement:
1. Build mimalloc as a static library (it's ~8K lines of C, MIT license).
2. Link AXIOM programs against mimalloc instead of system libc malloc.
3. Either:
   - **Static override**: Link mimalloc.o first so linker resolves malloc/free to mimalloc.
   - **Direct API**: Emit `call ptr @mi_malloc(i64 %size)` instead of `@malloc`.
   - **mimalloc-override.h**: Redefine malloc to mi_malloc at compile time.

For AXIOM, the cleanest approach is emitting direct `mi_malloc`/`mi_free` calls and
statically linking the mimalloc library.

### Implementation difficulty: EASY

Requires: Add mimalloc as a build dependency. Change malloc/free symbol names in codegen.
Approximately 1 hour of work for an immediate 1.5-2x improvement on allocation-heavy code.

---

## S4: COMPILE-TIME SIZE SPECIALIZATION

### What it is

When the compiler knows the exact allocation size at compile time (which AXIOM's type
system guarantees for `array[T, N]`), it can:
1. Pre-compute the exact byte size (no runtime multiplication).
2. Select the optimal size class directly (skip lookup tables).
3. Use fixed-size pool allocators for common sizes.
4. Potentially allocate from a pre-sized slab with zero overhead.

### Performance data

- **LLVM constant propagation** eliminates size computation entirely when N is known.
- **Size-class lookup elimination**: mimalloc's fast path for small objects is ~15
  instructions; with known size class, it's ~5 instructions.
- **No realloc needed**: C programs often `malloc` + `realloc` as arrays grow. AXIOM
  arrays have fixed size in the type -- allocate once, exactly right.
- **Scalar Replacement of Aggregates (SROA)**: LLVM's SROA pass can break up known-size
  allocas into individual SSA values, eliminating memory access entirely.

### How AXIOM's annotations enable it better than C

In C, `malloc(n * sizeof(int))` -- the compiler sees a runtime multiplication.
In AXIOM, `array[i32, 1024]` -- the compiler sees a constant 4096 bytes at parse time.

```axiom
fn process() -> i32 {
    // Compiler knows this is exactly 4096 bytes
    let data: array[i32, 1024] = array_zeros[i32, 1024];
    // Can use: fixed-size pool, slab, or pre-computed alloca
    ...
}
```

AXIOM's `@align(64)` further constrains the allocation to enable aligned SIMD access.

### LLVM implementation approach

1. During codegen, when emitting allocation for `array[T, N]`:
   - Compute `size = N * sizeof(T)` at compile time.
   - If size <= stack threshold: emit `alloca [N x T], align A`.
   - If size > threshold: emit `call ptr @mi_malloc(i64 <constant>)`.
   - Emit constant size as immediate, not computed value.
2. Use LLVM's `allocsize` attribute on allocation functions so LLVM knows the returned
   buffer size for bounds-check elimination.
3. For repeated allocations of the same size, route to a per-size pool (see S5).

### Implementation difficulty: EASY

This is largely already done for stack arrays. Extending to heap arrays requires
computing the constant size during HIR lowering and passing it through to codegen.

---

## S5: POOL ALLOCATION (Fixed-Size Object Pools)

### What it is

A pool allocator pre-allocates a block of memory divided into fixed-size slots. Allocation
pops a slot from a free list (O(1)). Deallocation pushes it back (O(1)). Eliminates
fragmentation entirely for uniform-size objects.

### Performance data

- **O(1) allocation and deallocation** vs O(N) for general malloc with fragmentation
  (mtrebi/memory-allocators benchmark).
- **Free list allocator is ~3x faster than malloc** for general-purpose patterns, and
  pool allocators are faster still for uniform sizes (mtrebi README).
- **7x faster than Windows global heap** for recycling allocator patterns
  (endurodave/Allocator benchmark).
- **Lattner's automatic pool allocation (PLDI 2005)**: 10-25% speedup in most heap-
  intensive programs, **2x** in two cases, **10x** in two benchmarks. Won Best Paper.
  Key insight: segregating data structures into separate pools improves cache locality
  by eliminating interleaving of unrelated objects.
- **Linux kernel SLUB**: Per-CPU object caches eliminate spinlock acquisition for common
  allocation patterns. Allocation is lock-free on the fast path.
- **Database systems**: Transaction Processing Facility and similar systems use object
  pools for deterministic allocation latency (Wikipedia: Memory pool).

### How AXIOM's annotations enable it better than C

AXIOM knows struct sizes at compile time. The compiler can automatically create per-type
pools:
```axiom
struct Particle {
    pos: (f32, f32, f32)
    vel: (f32, f32, f32)
    life: f32
}

// Compiler knows sizeof(Particle) = 28 bytes, can create a pool for this size
fn spawn_particles(n: i32) -> slice[Particle] {
    let particles: slice[Particle] = allocate(n);  // routed to 32-byte pool
    ...
}
```

AXIOM's `@lifetime(scope)` tells the compiler the pool can be freed in bulk.
AXIOM's `@constraint { max_memory_mb: 256 }` tells the compiler the maximum pool size.

### LLVM implementation approach

1. Build a small runtime library with typed pools:
   ```c
   typedef struct { void* free_list; void* slab; size_t slot_size; } Pool;
   void* axiom_pool_alloc(Pool* p);
   void axiom_pool_free(Pool* p, void* ptr);
   void axiom_pool_destroy(Pool* p);
   ```
2. During codegen, for allocations of struct types or fixed-size arrays:
   - Create a global pool per unique size class.
   - Replace `malloc(size)` with `axiom_pool_alloc(&pool_<size>)`.
   - Replace `free(ptr)` with `axiom_pool_free(&pool_<size>, ptr)`.
3. Initialize pools lazily on first allocation or eagerly at program start.

### Implementation difficulty: MEDIUM

Requires: pool runtime library (~200 lines of C), codegen changes to route allocations
by size class, pool initialization logic.

---

## S6: STACK ALLOCATION WITH HEAP FALLBACK

### What it is

Try to allocate on the stack first (via `alloca`). If the requested size exceeds a
threshold, fall back to heap allocation. This is the strategy behind Rust's `SmallVec`
and similar "small buffer optimization" patterns.

### Performance data

- **SmallVec** reduces heap allocations significantly when most collections fit inline.
  Improves cache locality for small collections (Rust perf book).
- **Allocation rate reduction**: In real workloads, 80-95% of allocations are small enough
  to fit on stack, eliminating the vast majority of malloc calls.
- **Go 1.25**: Stack-allocated slices under 32 bytes eliminate allocation entirely.
- **Trade-off**: Slightly slower than plain Vec for operations that must check inline vs
  heap, but much faster overall when most allocations stay inline.

### How AXIOM's annotations enable it better than C

AXIOM's type system often knows array sizes at compile time, making the stack/heap
decision a compile-time choice rather than a runtime branch:

```axiom
fn sort_small(data: slice[i32]) -> slice[i32] {
    // Compiler inserts: if data.len <= 1024, use alloca; else use malloc
    let temp: array[i32, ?n] = allocate(data.len);
    ...
}
```

For `@pure` functions, the compiler knows temp cannot escape, so it can always try stack
first without worrying about dangling pointers.

### LLVM implementation approach

```llvm
define ptr @axiom_smart_alloc(i64 %size) {
  %fits = icmp ult i64 %size, 65536    ; 64KB threshold
  br i1 %fits, label %stack, label %heap
stack:
  %s = call ptr @llvm.stacksave()
  %buf = alloca i8, i64 %size, align 16
  ret ptr %buf
heap:
  %buf2 = call ptr @mi_malloc(i64 %size)
  ret ptr %buf2
}
```

Note: The stack path requires careful lifetime management -- the `stacksave`/`stackrestore`
pair must bracket the usage scope.

### Implementation difficulty: EASY

Requires: Size threshold constant (configurable via `@strategy`), conditional branch in
codegen, `stacksave`/`stackrestore` for loop contexts.

---

## S7: REGION-BASED MEMORY MANAGEMENT

### What it is

All allocations within a lexical region share a common lifetime. When the region ends,
all its memory is freed at once -- no individual free calls needed. This is a
generalization of arena allocation with support for nested regions and region parameters.

### Performance data

- **Cyclone** (PLDI 2002): Region-based memory management for C with static type checking.
  Eliminates dangling pointers and memory leaks with modest annotation burden.
- **MLKit**: Automatic region inference for ML programs. Region annotations are inferred
  by the compiler. Programs run with no garbage collector.
- **Tofte & Talpin**: Foundational work showing that region inference can achieve
  performance competitive with or better than garbage collection for functional programs.

### How AXIOM's annotations enable it better than C

AXIOM can implement Cyclone-style regions with explicit annotations:
```axiom
@lifetime(scope)
fn process(data: slice[f32]) -> f32 {
    @arena(temp) {
        let a: slice[f32] = arena_alloc(temp, 1000);
        let b: slice[f32] = arena_alloc(temp, 1000);
        // a and b freed together at block exit
    }
    // temp arena is destroyed here
    ...
}
```

The key advantage over Cyclone: AXIOM's `@pure` annotation eliminates the need for
complex alias analysis. A `@pure` function cannot store pointers into globals, so
region safety is trivially verified.

### LLVM implementation approach

1. Region = arena with a type-system-enforced lifetime.
2. Compiler checks that no pointer allocated in a region escapes that region.
3. At region entry: create arena (bump allocator or mmap'd buffer).
4. At region exit: destroy arena (free all memory at once).
5. Nested regions use `stacksave`/`stackrestore` for stack-backed regions.

### Implementation difficulty: HARD

Requires: Lifetime checking pass in the compiler, region type system, escape analysis
for region pointers. This is essentially building a simplified Rust borrow checker.

---

## S8: ZERO-COPY VIEW SEMANTICS

### What it is

Slices and sub-arrays are represented as fat pointers (base pointer + length) that
reference existing memory without copying it. This eliminates allocation entirely for
many common operations (slicing, windowing, subarray extraction).

### Performance data

- **Zero-cost**: Converting a struct reference to a byte slice requires only pointer
  arithmetic (alignment check + bounds check). No allocation, no copying.
- **Rust's zerocopy crate**: Enables direct memory mapping of files into typed data
  structures. The performance is bounded by I/O, not allocation.
- **Zig slices**: Fat pointers (ptr + len) are the standard way to pass array views.
  No allocation overhead.

### How AXIOM's annotations enable it better than C

AXIOM already has `slice[T]` as a first-class type (fat pointer = ptr + len). Combined
with `@layout` annotations, the compiler knows the exact memory layout and can verify
that slice operations are safe:

```axiom
fn window(data: slice[f32] @layout(row_major), start: i32, len: i32) -> slice[f32] {
    // Returns a view into data -- no allocation, no copy
    return data[start..start + len];  // fat pointer arithmetic only
}
```

AXIOM's `@align` annotation ensures slices are properly aligned for SIMD operations.
C has no equivalent -- raw pointer arithmetic with no layout guarantees.

### LLVM implementation approach

Slices are already `{ ptr, i64 }` structs in LLVM IR. Sub-slicing is:
```llvm
%new_ptr = getelementptr float, ptr %base, i64 %start
%result = insertvalue { ptr, i64 } undef, ptr %new_ptr, 0
%result2 = insertvalue { ptr, i64 } %result, i64 %len, 1
```

No allocation, no memcpy, no free. Pure pointer arithmetic.

### Implementation difficulty: EASY

AXIOM already has slices. Just need to implement slice operations as pointer arithmetic
rather than allocation + copy.

---

## S9: LLVM stacksave/stackrestore FOR LOOP-SCOPED ALLOCATION

### What it is

LLVM's `@llvm.stacksave()` and `@llvm.stackrestore()` intrinsics record and restore
the stack pointer, enabling dynamic stack allocation within loops without stack overflow.
This is how Clang implements C99 variable-length arrays.

### Performance data

- **Stack allocation is essentially free**: Adjusting the stack pointer is a single
  instruction. No system calls, no lock contention, no fragmentation.
- **VLA in loops**: Without stacksave/restore, a VLA in a loop would grow the stack
  unboundedly. With it, each iteration reuses the same stack space.
- **Limitation**: LLVM's alias analysis sometimes fails to recognize that stackrestore
  invalidates prior allocas, leading to missed optimizations.

### How AXIOM's annotations enable it better than C

AXIOM's `@lifetime(scope)` on a block inside a loop tells the compiler to emit
stacksave/restore:

```axiom
for i in 0..n {
    @lifetime(scope) {
        let temp: array[f32, ?k] = allocate(k);
        // temp is stack-allocated, freed at end of each iteration
    }
}
```

C programmers must manually use `alloca` and hope the compiler does the right thing.
AXIOM makes this explicit and verifiable.

### LLVM implementation approach

```llvm
for.body:
  %save = call ptr @llvm.stacksave()
  %temp = alloca float, i64 %k, align 16
  ; ... use temp ...
  call void @llvm.stackrestore(ptr %save)
  br label %for.cond
```

### Implementation difficulty: EASY

Just emit stacksave/restore around alloca blocks when inside loops.

---

## S10: SLAB ALLOCATION

### What it is

Pre-allocate "slabs" of memory divided into fixed-size slots for common object sizes.
Each slab contains objects of exactly one size. Allocation is O(1): pop from per-CPU
free list. Deallocation is O(1): push back. No fragmentation within a slab.

### Performance data

- **Linux kernel SLUB allocator**: Default for all kernel object allocation. Per-CPU
  caches eliminate spinlock contention. Used billions of times per second globally.
- **Bonwick (1994)**: Original slab allocator paper showed significant reduction in
  initialization cost for frequently created/destroyed objects.
- **Cache-friendly**: Objects of the same type are packed contiguously, improving spatial
  locality and L1/L2 cache utilization.

### How AXIOM's annotations enable it better than C

AXIOM knows struct sizes at compile time. The compiler can automatically create slab
caches for commonly allocated types:

```axiom
struct Node {
    value: i64
    left: ptr[Node]
    right: ptr[Node]
}
// sizeof(Node) = 24 bytes -> compiler creates a 32-byte slab cache
// All Node allocations go through this cache
```

### LLVM implementation approach

Build a small slab allocator runtime. At program start (or lazily), initialize slab
caches for each struct size used in the program. Route allocations by size to the
appropriate cache.

### Implementation difficulty: MEDIUM

---

## S11: HUGE PAGES

### What it is

For allocations larger than 2MB, use 2MB "huge pages" instead of the standard 4KB pages.
This reduces TLB (Translation Lookaside Buffer) misses by up to 512x because each TLB
entry covers 512x more memory.

### Performance data

- **TLB miss cost**: 10-20 cycles for a page walk vs 1-2 cycles for a TLB hit -- a
  **10x penalty** per miss.
- **Meta/Facebook**: Close to **20% of execution cycles** spent handling TLB misses on
  64GB servers.
- **STREAM benchmark**: **11.6% to 16.6% improvement** with huge pages.
- **Redis**: **31% improvement** (117,096 vs 89,286 req/s) with THP enabled.
- **SpecCPU 2006**: ~13% improvement for integer benchmarks, ~7% for floating-point.

### How AXIOM's annotations enable it better than C

AXIOM's type system knows allocation sizes. When `array[T, N]` with `N * sizeof(T) > 2MB`,
the compiler automatically uses huge pages:

```axiom
// 4 million f32s = 16MB -> compiler uses huge pages
let matrix: array[f32, 4000000] = array_zeros[f32, 4000000];
```

AXIOM's `@strategy { huge_pages: true }` could also be explicit.

### LLVM implementation approach

On Linux: `mmap(NULL, size, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS|MAP_HUGETLB, -1, 0)`
On Windows: `VirtualAlloc(NULL, size, MEM_COMMIT|MEM_RESERVE|MEM_LARGE_PAGES, PAGE_READWRITE)`

Emit the appropriate OS call when allocation size exceeds 2MB threshold.

### Implementation difficulty: EASY

---

## S12: CUSTOM PAGE MANAGEMENT (mmap / VirtualAlloc)

### What it is

Instead of using malloc (which sits atop the OS page allocator), allocate pages directly
from the OS. This avoids malloc's bookkeeping overhead for large allocations. Both
VirtualAlloc (Windows) and mmap (Linux) guarantee page-aligned memory with lazy physical
allocation.

### Performance data

- **glibc already does this**: Large blocks (>128KB by default) are allocated via mmap
  internally. The advantage of managing this directly is eliminating the threshold
  decision overhead and customizing page management.
- **rpmalloc**: Lock-free allocator built directly on VirtualAlloc/mmap. Fixed page
  alignment by construction (masking out low bits gives page header). Very fast for
  multi-threaded workloads.
- **Lazy allocation**: Memory is not physically allocated until first access (page fault).
  This makes sparse arrays essentially free until elements are touched.

### How AXIOM's annotations enable it better than C

AXIOM's `@constraint { max_memory_mb: N }` tells the compiler the upper bound on memory
usage. The compiler can `mmap` the entire address space at program start and bump-allocate
within it. This gives arena-like speed with virtual-memory-backed safety.

### LLVM implementation approach

Emit platform-specific system calls for large allocations:
```llvm
; Linux
%ptr = call ptr @mmap(ptr null, i64 %size, i32 3, i32 34, i32 -1, i64 0)
; Windows
%ptr = call ptr @VirtualAlloc(ptr null, i64 %size, i32 12288, i32 4)
```

Use `#ifdef`-style platform detection during codegen or link against a thin abstraction.

### Implementation difficulty: MEDIUM

---

## S13: RECYCLING ALLOCATORS (Free-List Per Size Class)

### What it is

Keep a per-size-class free list. When an object is freed, push it onto the free list
instead of returning it to the system allocator. When a new object of the same size is
needed, pop from the free list. The recycled memory is "cache-hot" -- it was recently
accessed, so it's likely still in L1/L2 cache.

### Performance data

- **7x faster than Windows global heap** for allocation/deallocation cycles
  (endurodave/Allocator benchmark).
- **Feilbach (2025) real workload**: Recycling pool reduced malloc calls by **99%**
  (from 2 billion to 21 million), improving throughput by **15.4%** (2.79M to 3.22M
  nodes/sec). Loads executed reduced by **43-73%** due to improved cache locality.
- **mimalloc**: Uses free-list sharding internally -- per-page free lists give similar
  benefits with better scalability.

### How AXIOM's annotations enable it better than C

AXIOM's type system knows the size of every allocation. The compiler can maintain typed
free lists that never mix sizes:

```axiom
struct TreeNode { ... }  // sizeof = 32 bytes

@lifetime(manual)
fn build_tree() -> ptr[TreeNode] {
    let node: ptr[TreeNode] = allocate(1);  // checks 32-byte free list first
    ...
}

fn free_node(node: ptr[TreeNode]) {
    deallocate(node);  // pushes to 32-byte free list, does not call free()
}
```

### LLVM implementation approach

Maintain a global array of free list heads, indexed by size class. Allocation:
```llvm
%head = load ptr, ptr @freelist_32  ; size class 32
%is_null = icmp eq ptr %head, null
br i1 %is_null, label %slow, label %fast
fast:
  %next = load ptr, ptr %head
  store ptr %next, ptr @freelist_32
  ret ptr %head
slow:
  %new = call ptr @mi_malloc(i64 32)
  ret ptr %new
```

### Implementation difficulty: MEDIUM

---

## S14: SIMD-OPTIMIZED memset/memcpy

### What it is

Use SIMD instructions (AVX-256: 32 bytes/instruction, AVX-512: 64 bytes/instruction)
for array initialization and copying instead of byte-by-byte or even word-by-word loops.

### Performance data

- **AVX-512 memset**: glibc optimizations boost memset performance by up to **7.5%** on
  Intel Skylake/Ice Lake via AVX-512.
- **memset for large blocks**: Often **2-3x faster** than loops on modern CPUs due to
  vectorized implementations.
- **calloc vs malloc+memset**: calloc can use OS-level page zeroing (pages from mmap are
  pre-zeroed by the kernel), avoiding the memset entirely.

### How AXIOM's annotations enable it better than C

AXIOM's `@target { cpu.simd }` and `@align(64)` annotations guarantee that:
1. The target CPU has SIMD support.
2. Arrays are aligned for SIMD access.
3. Array sizes are known at compile time, enabling loop unrolling of memset.

```axiom
@target { cpu.simd }
fn init_data(data: array[f32, 1024] @align(64)) {
    // Compiler emits 64 AVX-512 stores (64 bytes each) = 4096 bytes
    // Instead of 1024 scalar stores
}
```

### LLVM implementation approach

LLVM already optimizes `@llvm.memset` and `@llvm.memcpy` to SIMD when available.
AXIOM's job is to:
1. Emit `@llvm.memset` / `@llvm.memcpy` with correct alignment attributes.
2. Ensure the alignment annotation (`@align(64)`) propagates to the LLVM IR.
3. Use `calloc`-equivalent (mmap'd zeroed pages) for zero-initialized arrays.

```llvm
call void @llvm.memset.p0.i64(ptr align 64 %data, i8 0, i64 4096, i1 false)
```

### Implementation difficulty: EASY

AXIOM already emits memset for `array_zeros`. Just need to ensure alignment is propagated.

---

## S15: NON-TEMPORAL STORES (Write-Combining)

### What it is

For sequential writes to newly allocated memory (initialization), use non-temporal store
instructions that bypass the CPU cache and write directly to memory. This avoids polluting
the cache with data that won't be read again soon.

### Performance data

- **Benefit is workload-dependent**: True benefit seen with memory-bound code not
  dominated by loads -- e.g., STREAM COPY benchmark, relaxation methods,
  lattice-Boltzmann (Georg Hager's blog).
- **Saves Read-For-Ownership (RFO)**: Normal stores must first read the cache line into
  cache (RFO), then modify it. Non-temporal stores skip the RFO, saving memory bandwidth.
- **Caveat**: On Sandy Bridge Xeon, non-temporal stores actually *slowed down* STREAM
  kernels because the L2 hardware prefetcher is very effective (Hager's blog). Profile
  before using.

### How AXIOM's annotations enable it better than C

AXIOM's `@strategy { streaming: true }` or `@layout` annotations can trigger
non-temporal stores for initialization of large arrays:

```axiom
@strategy { streaming: true }
fn init_large_array(data: slice[f64] @align(64)) {
    for i in 0..data.len {
        data[i] = 0.0;  // compiler emits MOVNTPD instead of MOVAPD
    }
}
```

### LLVM implementation approach

```llvm
; Non-temporal store (AVX-512):
call void @llvm.x86.avx512.storent.pd.512(ptr %addr, <8 x double> zeroinitializer)
; Or generic:
store <4 x double> zeroinitializer, ptr %addr, align 32, !nontemporal !0
!0 = !{i32 1}
```

LLVM supports the `!nontemporal` metadata on store instructions.

### Implementation difficulty: EASY

Just add `!nontemporal` metadata to stores when `@strategy { streaming: true }` is present.

---

## S16: PREFETCHING

### What it is

Insert `prefetch` instructions to bring data into cache before it's needed, hiding memory
latency. Most effective for indirect memory accesses and large array traversals.

### Performance data

- **SPEC2006**: Up to **50% improvement** per benchmark, **11% average** improvement
  on Shenwei 1621 processor with software prefetching (ResearchGate paper).
- **Array loops**: Prefetching upcoming array elements in a loop is the simplest and most
  effective pattern.
- **Indirect accesses**: Prefetching through pointer chains (linked lists, trees) provides
  large gains but requires more sophisticated analysis.

### How AXIOM's annotations enable it better than C

AXIOM's `@strategy { prefetch: ?distance }` provides an explicit optimization surface
for prefetch distance tuning:

```axiom
@strategy { prefetch: ?prefetch_distance }
fn sum(data: slice[f64]) -> f64 {
    let mut total: f64 = 0.0;
    for i in 0..data.len {
        total = total + data[i];
    }
    return total;
}
```

The AI agent can then propose `prefetch_distance: 8` (or 16, 32, etc.) and the compiler
emits `llvm.prefetch` intrinsics at the specified distance.

### LLVM implementation approach

```llvm
; Prefetch data 8 iterations ahead in a loop:
%future_idx = add i64 %i, 8
%future_ptr = getelementptr double, ptr %data, i64 %future_idx
call void @llvm.prefetch.p0(ptr %future_ptr, i32 0, i32 3, i32 1)
; args: ptr, rw (0=read), locality (3=keep in all caches), cache_type (1=data)
```

### Implementation difficulty: EASY

The `@strategy { prefetch: ?distance }` infrastructure already exists in AXIOM's
optimization protocol. Just need to emit the LLVM intrinsic.

---

## S17: MEMORY POOLS WITH COMPILE-TIME-KNOWN LIFETIMES

### What it is

Combine pool allocation (S5) with lifetime annotations (S7). When the compiler knows
exactly when a pool's objects die (via `@lifetime(scope|static|manual)`), it can:
- Skip reference counting.
- Skip garbage collection.
- Free the entire pool in one operation.
- Reuse pool memory for the next scope.

### Performance data

- Combines benefits of pool allocation (3-7x) with arena deallocation (100x+ for
  bulk free). Net effect: **3-10x** for programs with clear lifetime boundaries.
- **Cyclone regions**: Zero runtime overhead for region entry/exit. The type system
  prevents dangling pointers statically.
- **Rust ownership**: Zero-cost deallocation through deterministic drop. No GC pauses,
  no reference counting overhead.

### How AXIOM's annotations enable it better than C

```axiom
@lifetime(scope)
fn game_frame(entities: slice[Entity]) {
    // Pool is created at frame start, destroyed at frame end
    let particle_pool: Pool[Particle] = pool_create(10000);

    for i in 0..entities.len {
        let p: ptr[Particle] = pool_alloc(particle_pool);
        // ... physics simulation ...
    }
    // pool_destroy(particle_pool) is implicit -- @lifetime(scope) guarantees it
}
```

`@lifetime(static)`: Pool lives for entire program (e.g., caches, lookup tables).
`@lifetime(manual)`: Programmer controls deallocation explicitly (unsafe, for C interop).

### LLVM implementation approach

Pool + arena hybrid: pool for per-object alloc/free within the scope, arena-style bulk
deallocation at scope exit. The compiler inserts destructor calls at scope exits based
on lifetime annotations.

### Implementation difficulty: HARD

Requires: Full lifetime analysis pass, pool runtime, integration with codegen.

---

## S18: LLVM ALLOCATOR ATTRIBUTES

### What it is

Mark AXIOM's allocation functions with LLVM's allocator attributes (`allockind`,
`alloc-family`, `allocsize`, `allocalign`) so LLVM's optimization passes can reason
about and optimize AXIOM allocations.

### Performance data

- Enables LLVM to perform **dead allocation elimination** (remove unused allocs).
- Enables **heap-to-stack promotion** by LLVM's own passes (in addition to AXIOM's).
- Enables **allocation merging** and **allocation hoisting** out of loops.
- The Rust compiler uses these attributes to allow LLVM to optimize Rust allocations.

### How AXIOM's annotations enable it better than C

AXIOM can provide richer attributes than C because it knows more about its allocations:

```llvm
; AXIOM's allocator function with full attributes:
declare noalias ptr @axiom_alloc(i64 %size, i64 %align)
  allockind("alloc,unzeroed") allocsize(0) allocalign(1)
  alloc-family("axiom")

declare void @axiom_free(ptr allocptr %p)
  allockind("free") alloc-family("axiom")
```

### LLVM implementation approach

Add attributes to all allocation function declarations in the generated LLVM IR.
This is a ~10-line change in the codegen.

### Implementation difficulty: EASY

---

## S19: MEMORY-MAPPED LAZY ALLOCATION

### What it is

Reserve a large virtual address range (e.g., 1GB) using mmap/VirtualAlloc with no
physical memory commitment. Physical pages are allocated on-demand when first accessed
(page fault). This enables "sparse arrays" where only touched elements consume memory.

### Performance data

- **Zero cost until touched**: A 1GB mmap reservation costs essentially nothing until
  pages are accessed.
- **Sparse arrays**: An array of 1 billion elements where only 1% are used consumes
  only ~10MB of physical memory.
- **OS overcommit**: Linux allows allocating virtual memory far exceeding physical RAM.
  Only used pages consume resources.

### How AXIOM's annotations enable it better than C

```axiom
@strategy { sparse: true }
fn sparse_matrix(rows: i32, cols: i32) -> slice[f64] {
    // Compiler uses mmap with lazy allocation
    // Physical memory only for touched elements
    let data: slice[f64] = lazy_allocate(rows * cols);
    ...
}
```

### LLVM implementation approach

```llvm
; Reserve virtual address space (no physical memory):
; Linux: MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE
%ptr = call ptr @mmap(ptr null, i64 %size, i32 3, i32 16418, i32 -1, i64 0)
```

### Implementation difficulty: MEDIUM

---

## S20: WRITE-COMBINING BUFFERS

### What it is

For sequential writes to new allocations, use write-combining memory type to coalesce
multiple stores into cache-line-sized writes. The CPU's write-combining buffers
accumulate stores to the same cache line before flushing to memory.

### Performance data

- Effective for initialization patterns where data is written sequentially and not
  read back immediately.
- Most beneficial when combined with non-temporal stores (S15).
- **Workload-dependent**: Benefits vary by CPU microarchitecture.

### How AXIOM's annotations enable it better than C

Same as S15 -- `@strategy { streaming: true }` triggers this behavior.

### LLVM implementation approach

Same as S15 -- non-temporal stores automatically use write-combining buffers on x86.

### Implementation difficulty: EASY

---

## RECOMMENDED IMPLEMENTATION ORDER

### Phase 1: Immediate Wins (Week 1-2)
1. **S3: Link mimalloc** -- 1 hour of work, immediate 1.5-2x improvement.
2. **S18: LLVM allocator attributes** -- 10 lines of code, enables LLVM optimizations.
3. **S8: Zero-copy slices** -- Implement slice operations as pointer arithmetic.
4. **S14: SIMD memset** -- Ensure alignment propagates to LLVM memset calls.
5. **S4: Compile-time size specialization** -- Emit constant sizes, not computed.

### Phase 2: Core Allocation Infrastructure (Week 3-6)
6. **S1: Arena/bump allocation** -- Build arena runtime + `@arena` annotation.
7. **S2: Escape analysis** -- Leverage `@pure` for trivial stack promotion.
8. **S6: Stack with heap fallback** -- Conditional alloca/malloc codegen.
9. **S9: stacksave/stackrestore** -- Loop-scoped stack allocation.
10. **S16: Prefetching** -- Emit `llvm.prefetch` from `@strategy { prefetch }`.

### Phase 3: Advanced Allocators (Week 7-12)
11. **S5: Pool allocation** -- Per-type-size pools with free lists.
12. **S13: Recycling allocators** -- Free list per size class.
13. **S10: Slab allocation** -- Pre-allocated slabs for common sizes.
14. **S11: Huge pages** -- Automatic huge page use for large allocations.
15. **S12: Custom page management** -- Direct mmap/VirtualAlloc.

### Phase 4: Lifetime System (Week 13+)
16. **S7: Region-based management** -- Full region type system.
17. **S17: Lifetime-driven pools** -- Pools with compile-time-known lifetimes.
18. **S15/S20: Non-temporal stores** -- Profile-guided streaming store insertion.
19. **S19: Lazy allocation** -- mmap-based sparse arrays.

---

## NEW ANNOTATIONS NEEDED

| Annotation | Target | Purpose |
|-----------|--------|---------|
| `@lifetime(scope\|static\|manual)` | Function, Block | Declares allocation lifetime |
| `@arena(name)` | Block | Creates a named arena for bump allocation |
| `@pool(type)` | Block, Function | Routes allocations to a typed pool |

These build on AXIOM's existing custom annotation infrastructure (`@<name>` is already
parsed and preserved in AST/HIR).

---

## KEY RESEARCH PAPERS AND RESOURCES

1. **Lattner & Adve, "Automatic Pool Allocation" (PLDI 2005)**: Best Paper. 10-25%
   speedup, up to 10x on some benchmarks. Fully automatic, no annotations needed.
   https://llvm.org/pubs/2005-05-21-PLDI-PoolAlloc.html

2. **Grossman et al., "Region-Based Memory Management in Cyclone" (PLDI 2002)**:
   Foundation for annotation-driven region management.
   https://homes.cs.washington.edu/~djg/papers/old/cyclone_regions-abstract.html

3. **Leijen et al., "mimalloc: Free List Sharding in Action" (MSR 2019)**:
   Modern allocator design, outperforms all competitors.
   https://github.com/microsoft/mimalloc

4. **Lattner, "Macroscopic Data Structure Analysis and Optimization" (PhD thesis, 2005)**:
   Complete theory of pointer analysis and pool allocation in LLVM.
   https://llvm.org/pubs/2005-05-04-LattnerPHDThesis.html

5. **CMU CS 15-745, "Promoting Heap Allocations to the Stack in LLVM"**:
   Practical heap-to-stack promotion with ~2x speedup.
   http://www.cs.cmu.edu/afs/cs.cmu.edu/user/jatina/www/CS_15_745_Final_Report.pdf

6. **kipply, "Escape Analysis in PyPy, LuaJIT, V8, C++, Go and More"**:
   Cross-language comparison of escape analysis effectiveness.
   https://kipp.ly/escape-analysis/

7. **Fleury, "Untangling Lifetimes: The Arena Allocator"**:
   Practical guide to arena allocation for game engines.
   https://www.rfleury.com/p/untangling-lifetimes-the-arena-allocator

8. **Bonwick, "The Slab Allocator" (USENIX 1994)**:
   Foundational paper on object-caching kernel memory allocator.
   https://people.eecs.berkeley.edu/~kubitron/courses/cs194-24-S14/hand-outs/bonwick_slab.pdf

9. **LLVM RFC: Attributes for Allocator Functions in IR**:
   How to mark custom allocators for LLVM optimization.
   https://discourse.llvm.org/t/rfc-attributes-for-allocator-functions-in-llvm-ir/61464

10. **Feilbach, "Custom Memory Allocator: Implementation and Performance Measurements"**:
    Real benchmark data: malloc ~26ns, recycling pool 99% fewer malloc calls, 15% speedup.
    https://chrisfeilbach.com/2025/06/22/custom-memory-allocator-implementation-and-performance-measurements/

11. **Exgen-Malloc (arXiv 2025)**: Comprehensive allocator comparison including mimalloc,
    jemalloc, tcmalloc on SPEC CPU2017, Redis, and mimalloc-bench.
    https://arxiv.org/html/2510.10219v1

12. **Nikitin, "Transparent Hugepages: Measuring the Performance Impact"**:
    Detailed benchmarks of THP including STREAM (11-16%), Redis (31%), SpecCPU (7-13%).
    https://alexandrnikitin.github.io/blog/transparent-hugepages-measuring-the-performance-impact/

---

## BOTTOM LINE

AXIOM's annotation system (`@pure`, `@lifetime`, `@layout`, `@align`, `@strategy`,
`@arena`) provides the compiler with information that C compilers can never have. This
enables:

1. **Automatic allocator selection**: The compiler picks arena, pool, stack, or heap per
   call site based on annotations.
2. **Guaranteed escape analysis**: `@pure` makes escape analysis trivial.
3. **Zero-overhead deallocation**: `@lifetime(scope)` enables bulk arena deallocation.
4. **Size-optimized allocation**: Type system provides exact sizes at compile time.
5. **SIMD-aligned allocation**: `@align` ensures vectorizable memory access.

**Combined expected improvement over C malloc/free**: 2-10x for typical programs, up to
100x for allocation-heavy workloads with clear lifetime patterns. The key is that these
are not alternative allocators the programmer chooses -- the compiler selects the optimal
strategy automatically based on annotations that are already present for other reasons
(correctness, optimization, documentation).
