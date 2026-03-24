# AXIOM Multithreading & Parallel Job System: Correctness Analysis

**Status:** CRITICAL -- current implementation has undefined behavior
**Date:** 2026-03-24
**Scope:** Complete analysis of LLVM memory model, data dependency analysis, safe parallelism patterns, and proposed correct designs for AXIOM

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [What Is Wrong with AXIOM's Current Approach](#2-what-is-wrong-with-axioms-current-approach)
3. [LLVM's Memory Model and Atomic Support](#3-llvms-memory-model-and-atomic-support)
4. [Data Dependency Analysis for Parallelism](#4-data-dependency-analysis-for-parallelism)
5. [Safe Parallelism Patterns in Language Design](#5-safe-parallelism-patterns-in-language-design)
6. [LLVM Parallel IR Patterns](#6-llvm-parallel-ir-patterns)
7. [Reduction Patterns](#7-reduction-patterns)
8. [Proposed Correct Designs for AXIOM](#8-proposed-correct-designs-for-axiom)
9. [Implementation Roadmap](#9-implementation-roadmap)
10. [References](#10-references)

---

## 1. Executive Summary

AXIOM's current job system (`job_dispatch`, `job_wait`) is **dangerously naive**. It dispatches work to threads with:

- **No data dependency checking** -- the compiler cannot prove that parallel invocations do not alias
- **No synchronization guarantees** -- no memory fences between producer and consumer
- **No memory model** -- LLVM IR requires explicit atomic/fence annotations for cross-thread visibility
- **A lying `@pure` annotation** -- `compute_chunk` in `jobs_test.axm` is annotated `@pure` but writes through a raw pointer, which is the exact opposite of pure
- **No reduction support** -- summing across threads requires atomics or thread-local accumulators
- **No dependency graph** -- Job B cannot wait on Job A's specific output

The runtime C implementation (`axiom_rt.c`) is actually well-structured -- it has a proper thread pool with condition variables. The problem is entirely at the **language and compiler level**: AXIOM provides no mechanism to verify that the user's parallel code is correct, and emits LLVM IR that can exhibit undefined behavior under concurrent execution.

This document provides exhaustive research on how to fix this correctly.

---

## 2. What Is Wrong with AXIOM's Current Approach

### 2.1 The Current API

```axiom
// From tests/samples/jobs_test.axm
@pure
fn compute_chunk(data: ptr[i32], start: i32, end: i32) {
    for i: i32 in range(start, end) {
        ptr_write_i32(data, i, i * i);
    }
}

fn main() -> i32 {
    let n: i32 = 10000;
    let data: ptr[i32] = heap_alloc(n, 4);
    jobs_init(4);
    job_dispatch(compute_chunk, data, n);
    job_wait();
    // ... read results ...
}
```

### 2.2 Specific Bugs

#### Bug 1: `@pure` is a lie

The current codegen (`llvm.rs:863-893`) translates `@pure` to:
```llvm
; For @pure with pointer args:
attributes #N = { memory(argmem: read) nounwind willreturn nosync }
```

But `compute_chunk` **writes** through `data` via `ptr_write_i32`. Marking it `memory(argmem: read)` tells LLVM the function only reads argument memory, which is false. LLVM is then free to:
- Reorder or eliminate the stores
- Assume the memory pointed to by `data` is unchanged after the call
- Delete the entire `job_dispatch` call as dead code (since the function "has no side effects")

This is **immediate undefined behavior** in LLVM IR.

#### Bug 2: `noalias` on all pointer parameters is wrong for shared data

From `llvm.rs:931-933`:
```rust
if llvm_type == "ptr" {
    // AXIOM has no pointer aliasing -- emit noalias on all ptr params.
    parts.push(format!("ptr noalias %{}", param.name));
}
```

When multiple threads call `compute_chunk` with the **same** `data` pointer but different `start/end` ranges, `noalias` tells LLVM that no other pointer in the function aliases this one. But in the parallel context, other threads ARE accessing the same memory through the same base pointer. If any LLVM optimization pass reasons across the `noalias` guarantee in a cross-thread context, the result is undefined behavior.

For the specific case where threads access **disjoint** ranges, this happens to be safe on current hardware. But LLVM provides no guarantee that it will remain safe, and the compiler has no mechanism to verify the disjointness.

#### Bug 3: No memory fence between job_dispatch and job_wait

The C runtime does have synchronization (condition variable wait/signal), which provides an implicit acquire/release fence on POSIX and Windows. However, the LLVM IR emitted by AXIOM has **no indication** that memory is synchronized between the `call void @axiom_job_dispatch(...)` and the subsequent reads of `data`. LLVM is free to:
- Hoist reads of `data` above the `job_wait` call
- Cache values from before `job_dispatch` and reuse them after `job_wait`
- Reorder the loads with respect to the function calls

The runtime C functions act as **opaque calls**, which LLVM treats conservatively (assuming they may read/write all memory). This is actually the saving grace -- because the functions are not marked `readnone` or `readonly`, LLVM cannot reorder loads/stores past them. But this is **accidental correctness**, not by design.

#### Bug 4: No bounds enforcement

`compute_chunk` receives `(data: ptr[i32], start: i32, end: i32)`. Nothing prevents a worker from:
- Writing outside its assigned range (`data[end + 100] = ...`)
- Writing to index 0 regardless of its assigned range
- Passing a negative start or end beyond the array size
- Having overlapping ranges between workers

The language has no mechanism to enforce that each worker only accesses its assigned slice.

#### Bug 5: No reduction support

If the user wants to sum values in parallel:
```axiom
// WRONG -- data race on sum
@pure
fn sum_chunk(data: ptr[i32], start: i32, end: i32) {
    for i: i32 in range(start, end) {
        sum = sum + ptr_read_i32(data, i);  // WHERE does sum live? Race condition!
    }
}
```

There is no mechanism for:
- Thread-local accumulators
- Atomic accumulation
- Combining partial results after parallel execution

#### Bug 6: No dependency graph

```axiom
// WRONG -- both jobs operate on same data, no ordering guarantee
job_dispatch(compute_forces, bodies, n);    // writes forces
job_dispatch(integrate_positions, bodies, n); // reads forces, writes positions
job_wait(); // waits for BOTH -- but integrate may start before compute finishes!
```

The current `job_dispatch` queues work immediately. If both dispatches happen before any worker picks up work, a worker could grab the `integrate_positions` job before `compute_forces` has completed for that data.

---

## 3. LLVM's Memory Model and Atomic Support

### 3.1 LLVM's Memory Model Overview

LLVM implements the C++20 memory model (corrected from C++11/C14). The key principles:

1. **Data races on non-atomic variables are undefined behavior.** If two threads access the same memory location, at least one is a write, and there is no happens-before relationship between them, the behavior is undefined.

2. **Atomic operations provide ordering guarantees.** LLVM provides atomic loads, stores, read-modify-write operations, and fences with explicit ordering constraints.

3. **The compiler may reorder non-atomic operations freely** within the constraints of the program's sequential semantics. Cross-thread visibility requires explicit synchronization.

### 3.2 Memory Ordering Levels

From weakest to strongest:

| Ordering | LLVM IR keyword | C++ equivalent | Guarantees |
|----------|----------------|----------------|------------|
| Not Atomic | (default) | N/A | Races produce `undef`. No atomicity. |
| Unordered | `unordered` | N/A (LLVM-specific) | No tearing. Races produce consistent values (not `undef`). No ordering. |
| Monotonic | `monotonic` | `memory_order_relaxed` | Consistent per-address total order. No cross-address ordering. |
| Acquire | `acquire` | `memory_order_acquire` | All prior writes by the releasing thread are visible. Read barrier. |
| Release | `release` | `memory_order_release` | All prior writes become visible to acquiring threads. Write barrier. |
| Acquire-Release | `acq_rel` | `memory_order_acq_rel` | Both acquire and release. For read-modify-write ops. |
| Sequentially Consistent | `seq_cst` | `memory_order_seq_cst` | Acquire + Release + total global order of all seq_cst operations. |

### 3.3 Atomic Instructions in LLVM IR

#### Atomic Load
```llvm
%val = load atomic i32, ptr %addr seq_cst, align 4
%val = load atomic i32, ptr %addr acquire, align 4
%val = load atomic i32, ptr %addr monotonic, align 4
```

#### Atomic Store
```llvm
store atomic i32 %val, ptr %addr seq_cst, align 4
store atomic i32 %val, ptr %addr release, align 4
store atomic i32 %val, ptr %addr monotonic, align 4
```

#### Atomic Read-Modify-Write (`atomicrmw`)
```llvm
%old = atomicrmw add ptr %addr, i32 1 seq_cst
%old = atomicrmw sub ptr %addr, i32 1 acquire
%old = atomicrmw xchg ptr %addr, i32 %newval acq_rel
%old = atomicrmw and ptr %addr, i32 %mask release
%old = atomicrmw or ptr %addr, i32 %bits monotonic
%old = atomicrmw xor ptr %addr, i32 %bits seq_cst
%old = atomicrmw max ptr %addr, i32 %val seq_cst
%old = atomicrmw min ptr %addr, i32 %val seq_cst
%old = atomicrmw umax ptr %addr, i32 %val seq_cst
%old = atomicrmw umin ptr %addr, i32 %val seq_cst
```

#### Compare-and-Swap (`cmpxchg`)
```llvm
%result = cmpxchg ptr %addr, i32 %expected, i32 %desired seq_cst seq_cst
; %result is { i32, i1 } -- the old value and a success flag
%old = extractvalue { i32, i1 } %result, 0
%ok  = extractvalue { i32, i1 } %result, 1
```

The two orderings are for success and failure cases respectively. The failure ordering must be no stronger than the success ordering.

#### Fence
```llvm
fence acquire          ; prevent reordering of loads past this point
fence release          ; ensure all prior stores are visible
fence acq_rel          ; both acquire and release
fence seq_cst          ; full sequential consistency barrier
```

A `monotonic` load followed by an `acquire` fence is roughly equivalent to an `acquire` load. A `release` fence followed by a `monotonic` store is roughly equivalent to a `release` store.

### 3.4 Hardware Mapping

**x86 (strong memory model):**
- All loads are implicitly acquire (except from WC memory)
- All stores are implicitly release
- `seq_cst` stores emit `XCHG` (which has implicit `LOCK` prefix)
- `seq_cst` fences emit `MFENCE`
- `atomicrmw add` emits `LOCK XADD`
- `cmpxchg` emits `LOCK CMPXCHG`
- Most atomics are free on x86 -- the hardware already provides strong ordering

**ARM/AArch64 (weak memory model):**
- Acquire loads need `LDAR` (ARMv8) or load + `DMB` barrier
- Release stores need `STLR` (ARMv8) or `DMB` + store
- RMW operations use load-link/store-conditional (`LDXR`/`STXR`) loops
- Every atomic operation has real cost on ARM

### 3.5 What LLVM Requires for Correct Multithreaded IR

1. **Any memory location accessed by multiple threads where at least one access is a write MUST use atomic operations** or be protected by synchronization primitives (which themselves use atomics internally).

2. **Non-atomic loads from locations written by other threads return `undef`** -- not the written value, not garbage, but LLVM's poison/undef value which can propagate unpredictably.

3. **Function calls to opaque (external) functions act as compiler barriers** -- LLVM assumes they may read/write any non-local memory. This is why AXIOM's current approach "accidentally works" -- `axiom_job_dispatch` and `axiom_job_wait` are opaque calls that prevent reordering.

4. **The `nosync` attribute on a function tells LLVM it does not synchronize with other threads.** AXIOM currently emits `nosync` on `@pure` functions. If a `@pure` function is called from a parallel context, this is dangerous -- it tells LLVM that no synchronization is happening, enabling aggressive reordering.

### 3.6 Thread-Local Storage in LLVM IR

```llvm
; General-dynamic TLS (most portable)
@counter = thread_local global i32 0, align 4

; Initial-exec TLS (faster, for statically linked)
@counter = thread_local(initialexec) global i32 0, align 4

; Local-exec TLS (fastest, for executable-only)
@counter = thread_local(localexec) global i32 0, align 4
```

Thread-local variables are useful for per-thread accumulators in reduction patterns.

### 3.7 The `noalias` Attribute

From LLVM LangRef: `noalias` on a function parameter means that memory locations accessed via pointer values based on this argument are not also accessed (during the execution of the function) via pointer values not based on this argument. This is similar to C99's `restrict`.

**Critical for parallelism:** When AXIOM emits `noalias` on all pointer parameters, it is saying "this pointer does not alias any other pointer visible to this function." In a parallel context where multiple threads share the same base pointer:

- If threads access **disjoint** ranges: `noalias` is technically correct (no single function invocation sees aliasing)
- If threads access **overlapping** ranges: `noalias` is a lie and produces UB
- **The compiler cannot verify disjointness** without range analysis

The correct approach: only emit `noalias` when the compiler can **prove** non-aliasing, or when the user explicitly annotates exclusive access.

---

## 4. Data Dependency Analysis for Parallelism

### 4.1 Types of Data Dependencies

For a loop to be safely parallelizable, the compiler must prove the absence of **loop-carried dependencies**:

| Dependency Type | Definition | Example | Parallelizable? |
|----------------|-----------|---------|-----------------|
| Flow (RAW) | Read-After-Write: iteration j reads what iteration i wrote | `a[i] = ...; ... = a[i-1]` | NO |
| Anti (WAR) | Write-After-Read: iteration j writes what iteration i read | `... = a[i]; a[i-1] = ...` | With privatization |
| Output (WAW) | Write-After-Write: both iterations write same location | `a[0] = i; a[0] = j` | NO |
| Input (RAR) | Read-After-Read: both iterations read same location | `... = a[0]; ... = a[0]` | YES (always safe) |

### 4.2 The GCD Test

The simplest dependency test for array subscripts. Given:
```
Statement S1: ... = A[a*i + b]
Statement S2: A[c*i + d] = ...
```

If `GCD(a, c)` does not divide `(d - b)`, then there is **no** dependency. If it does divide, there **may** be a dependency (the test is necessary but not sufficient).

Example:
```
A[2*i] = ...     ; writes even indices
... = A[2*i + 1] ; reads odd indices
```
GCD(2, 2) = 2, and 2 does not divide (1 - 0) = 1. Therefore: **no dependency**, safe to parallelize.

### 4.3 The Banerjee Test and Omega Test

More precise tests that consider the iteration space bounds:

- **Banerjee test**: Computes bounds on the difference between subscript expressions. If the bounds do not include zero, no dependency exists.
- **Omega test**: Uses integer linear programming to exactly characterize dependencies. Most precise but most expensive.

### 4.4 Polyhedral Analysis

The most powerful framework for dependency analysis:

1. **Represent each loop iteration as a point in an integer lattice** (the iteration space)
2. **Represent array accesses as affine functions** of loop indices
3. **Compute dependency polyhedra** -- the set of iteration pairs that access the same memory
4. **If the dependency polyhedron is empty**, the loop is parallelizable

The polyhedral model can also find **partial parallelism** -- tiling and reordering to maximize parallelizable sub-regions.

### 4.5 Alias Analysis for Pointer-Based Languages

For languages with pointers (like AXIOM), array subscript analysis is not enough. The compiler must also determine whether different pointer arguments refer to the same memory:

**Types of alias analysis:**
- **Type-based (TBAA)**: Different types cannot alias (C strict aliasing)
- **Flow-sensitive**: Tracks pointer values through assignments
- **Field-sensitive**: Distinguishes fields within structs
- **Context-sensitive**: Different call sites may have different aliasing

**AXIOM's problem:** Raw `ptr[T]` provides no aliasing information. The compiler cannot distinguish:
```axiom
fn f(a: ptr[i32], b: ptr[i32]) { ... }
// Called as: f(data, data)       -- a and b alias!
// Called as: f(data, other_data) -- a and b don't alias
```

### 4.6 What AXIOM Would Need for Automatic Parallelization

To automatically parallelize loops, AXIOM would need:

1. **Array bounds information** in the type system (not just `ptr[T]` but `slice[T, start..end]` or similar)
2. **Alias analysis annotations** (`exclusive`, `readonly`, `noalias`)
3. **Loop analysis passes** that compute dependencies
4. **Scalar evolution** to determine that loop induction variables produce non-overlapping array indices
5. **Inter-procedural analysis** to determine what called functions actually access

This is the work of decades of compiler research. A more practical approach is to **require the programmer to annotate** parallelism intent and have the compiler **verify** the annotation is safe, rather than discovering parallelism automatically.

---

## 5. Safe Parallelism Patterns in Language Design

### 5.1 Rust: Ownership-Based Safety

**Core mechanism:** The borrow checker statically ensures that:
- Mutable references (`&mut T`) are exclusive -- only one exists at a time
- Shared references (`&T`) are immutable -- any number can coexist
- A value cannot be both mutably and immutably borrowed simultaneously

**Thread safety traits:**
- `Send`: Type can be transferred to another thread (ownership moves)
- `Sync`: Type can be shared between threads via `&T` references
- Both are auto-traits -- the compiler derives them if all fields satisfy them
- `!Send`/`!Sync` types (e.g., `Rc<T>`, raw pointers) cannot cross thread boundaries

**Rayon parallel iterators:**
```rust
// Safe: par_iter_mut gives each thread exclusive &mut to disjoint elements
data.par_iter_mut().for_each(|x| *x = compute(*x));

// Safe: par_iter gives each thread shared & references
let sum: i32 = data.par_iter().map(|x| x * x).sum();

// Won't compile: closure captures &mut to shared state
let mut total = 0;
data.par_iter().for_each(|x| total += x); // ERROR: FnMut, not Fn+Sync
```

**Key insight:** Rayon requires closures to be `Fn + Send + Sync`, not `FnMut`. This means:
- The closure cannot mutate captured state (only `Fn`, not `FnMut`)
- The closure can be sent to other threads (`Send`)
- The closure can be shared between threads (`Sync`)

**How `split_at_mut` enables safe parallelism:**
```rust
let (left, right) = slice.split_at_mut(mid);
// left: &mut [T] for [0..mid]
// right: &mut [T] for [mid..len]
// Compiler proves these don't alias -- safe to use from different threads
```

**Correctness guarantees:** Compile-time, zero runtime cost. If the code compiles, there are no data races.

**Implementation complexity for AXIOM:** HIGH -- requires a full ownership/borrowing type system, lifetime analysis, and trait system.

### 5.2 Go: Runtime Detection, Not Prevention

**Model:** Goroutines with shared memory. No compile-time data race prevention.

**Philosophy:** "Do not communicate by sharing memory; instead, share memory by communicating" (use channels).

**Race detector:** Runtime instrumentation (ThreadSanitizer) that detects concurrent unsynchronized access. Enabled with `-race` flag. 10x CPU/memory overhead.

**Data race definition (Go spec):** A data race occurs when two goroutines access the same variable concurrently and at least one of the accesses is a write, without synchronization.

**Synchronization primitives:**
- Channels (preferred)
- `sync.Mutex`, `sync.RWMutex`
- `sync/atomic` package
- `sync.WaitGroup` for joining

**Lesson for AXIOM:** Runtime detection is useful for testing but cannot guarantee correctness. Go's approach is deliberately pragmatic -- it accepts that data races are possible and provides tools to find them rather than prevent them.

### 5.3 Chapel: Forall with Data Intents

**Model:** First-class parallel loops with explicit data-sharing semantics.

**Forall loop syntax:**
```chapel
forall i in 1..n with (in localCopy, ref sharedRef, + reduce sum) {
    localCopy += 1;        // per-task private copy
    sharedRef[i] = i;      // shared mutable reference (user's responsibility)
    sum += data[i];         // reduction -- safe, compiler handles accumulation
}
```

**Data intent types:**

| Intent | Semantics | Thread Safety |
|--------|-----------|---------------|
| `in` / `const in` | Per-task copy of outer variable | SAFE -- no sharing |
| `ref` / `const ref` | Alias to outer variable | UNSAFE -- user must ensure no races |
| `reduce` (with operator) | Per-task accumulator, combined after loop | SAFE -- compiler-managed |
| (default for scalars) | `const in` (value copy) | SAFE |
| (default for arrays) | `ref` | UNSAFE |

**Shadow variables:** Each task gets its own set of shadow variables. References within the task body refer to shadow variables, not the original. This is how reduction works: each task has its own accumulator shadow variable, initialized to the reduction identity (0 for `+`, 1 for `*`, etc.), and the results are combined after the loop.

**Correctness guarantees:** Reductions are safe. `in`/`const in` are safe. `ref` intents are the user's responsibility -- the compiler does not verify safety for `ref`.

**Implementation complexity for AXIOM:** MODERATE -- requires shadow variable codegen and reduction combining, but no full ownership system.

### 5.4 Swift: Actor Isolation and Sendable

**Model:** Structured concurrency with actor isolation.

**Actors:** Each actor owns a bag of mutable state. Access to an actor's state is serialized -- only one task can access it at a time. Cross-actor access requires `await`.

**Sendable protocol:** Types that can safely cross concurrency domains. Value types (structs, enums) are automatically Sendable if all fields are. Reference types must explicitly declare Sendable conformance, and the compiler verifies they are either:
- Immutable (all `let` properties)
- Actor-isolated
- Use internal locking

**Structured concurrency:** Tasks form a tree. Parent tasks cannot return until all child tasks complete. Cancellation propagates down the tree.

**Lesson for AXIOM:** The actor model is excellent for message-passing concurrency but less applicable to data-parallel workloads (parallel for loops). The Sendable concept is relevant -- AXIOM could require that data passed to parallel regions implements a "sendable" constraint.

### 5.5 Java: Virtual Threads and Structured Concurrency

**Virtual threads (Project Loom):** Lightweight, JVM-managed threads. Millions can coexist. Mapped to platform threads by the JVM scheduler.

**Structured concurrency (JEP 505, Java 25):** Tasks form hierarchical scopes. A `StructuredTaskScope` ensures:
- All forked subtasks complete before the scope returns
- If any subtask fails, siblings can be cancelled
- Resources are cleaned up deterministically

**Scoped values:** Immutable, inheritable context that replaces `ThreadLocal`. Cannot be mutated, only rebound in a child scope.

**Lesson for AXIOM:** Structured concurrency ensures that parallel regions have well-defined lifetimes and resource cleanup. The "scoped value" pattern is useful -- immutable shared context is always safe.

### 5.6 Cilk: Spawn/Sync with Serial Semantics

**Model:** Fork-join parallelism with provable efficiency.

**Keywords:**
```c
cilk_spawn f(x);  // f may execute in parallel with continuation
cilk_sync;        // wait for all spawned tasks in this function
cilk_for (int i = 0; i < n; i++) { ... } // parallel loop
```

**Serial elision property:** Removing all Cilk keywords produces a valid sequential C program with the same semantics. This means:
- The parallel version is correct if and only if the sequential version is correct
- Debugging can be done on the sequential version
- The programmer thinks sequentially, the runtime parallelizes

**Work-stealing scheduler:** Achieves provably optimal parallel execution time for "fully strict" (well-structured) programs:
- Time: `O(T1/P + T_inf)` where T1 is serial time, P is processors, T_inf is critical path
- Space: `O(P * S1)` where S1 is serial space
- Communication: `O(P * T_inf)` steals

**Data race freedom:** Cilk does NOT guarantee data race freedom. It provides a race detector (Cilksan) as a debugging tool. The programmer must ensure that spawned tasks do not have conflicting accesses.

**Lesson for AXIOM:** The serial elision property is extremely valuable for debugging and reasoning. AXIOM could adopt this: `@parallel for` should produce the same result as the sequential `for`, with a race detector available for testing.

### 5.7 Naughty Dog: Fiber-Based Job System with Counters

**Architecture (GDC 2015, Christian Gyrling):**

1. **All engine code is structured as jobs** running in fibers on a thread pool (one thread per CPU core)
2. **Jobs are defined as function pointer + data pointer**
3. **Dependencies use atomic counters:**
   - `run_jobs(func, data, count)` returns an `AtomicCounter*`
   - Counter starts at `count`, decremented when each job completes
   - `wait_for_counter(counter, target_value)` blocks until counter reaches target
4. **Fibers enable non-blocking waits:**
   - When a job calls `wait_for_counter`, the **fiber** is suspended (not the thread)
   - The thread picks up another ready fiber from the pool
   - When the counter reaches the target, the suspended fiber is placed back in the ready queue
   - **No thread context switching** -- only register save/restore

**Counter handle system:**
- Counters are stored in a global array
- Handles encode `(index, version)` to detect use-after-free
- `WaitForCounterAndFree` combines waiting and deallocation

**Data flow between jobs:**
- Jobs communicate through shared memory (pointer passing)
- The counter/dependency system ensures ordering but NOT data isolation
- It is the programmer's responsibility to ensure no data races within a dependency level

**Lesson for AXIOM:** The counter-based dependency model is simple and efficient. AXIOM could adopt this for the "aggressive" design tier. However, Naughty Dog accepts that data races are possible -- their programmers are experts who reason about data flow manually. AXIOM should layer safety annotations on top.

---

## 6. LLVM Parallel IR Patterns

### 6.1 How Clang Lowers OpenMP Parallel For

When Clang encounters `#pragma omp parallel for`, it:

1. **Outlines** the loop body into a separate function (the "microtask")
2. **Replaces** the loop with a call to `__kmpc_fork_call` (the OpenMP runtime)
3. **Distributes** loop iterations via `__kmpc_for_static_init_4`

#### The LLVM IR Pattern

**Original C code:**
```c
#pragma omp parallel for
for (int i = 0; i < 100; i++) {
    A[i] = B[i] * C[i];
}
```

**Generated LLVM IR (simplified):**
```llvm
; Main function -- replaced parallel region with runtime call
define void @main() {
entry:
  ; Capture shared variables into a struct
  %agg.captured = alloca %struct.anon, align 8
  %0 = getelementptr inbounds %struct.anon, ptr %agg.captured, i32 0, i32 0
  store ptr @A, ptr %0
  %1 = getelementptr inbounds %struct.anon, ptr %agg.captured, i32 0, i32 1
  store ptr @B, ptr %1
  %2 = getelementptr inbounds %struct.anon, ptr %agg.captured, i32 0, i32 2
  store ptr @C, ptr %2

  ; Fork threads -- each runs the outlined function
  call void (%ident_t*, i32, void (i32*, i32*, ...)*, ...) @__kmpc_fork_call(
    %ident_t* @.loc, i32 1,
    void (i32*, i32*, ...)* bitcast (
      void (i32*, i32*, %struct.anon*)* @.omp_outlined. to void (i32*, i32*, ...)*
    ),
    ptr %agg.captured
  )
  ret void
}

; Outlined function -- the actual parallel work
define internal void @.omp_outlined.(i32* noalias %.global_tid.,
                                      i32* noalias %.bound_tid.,
                                      %struct.anon* %__context) {
entry:
  ; Local iteration variables
  %.omp.lb = alloca i32, align 4      ; lower bound for this thread
  %.omp.ub = alloca i32, align 4      ; upper bound for this thread
  %.omp.stride = alloca i32, align 4
  %.omp.is_last = alloca i32, align 4
  %.omp.iv = alloca i32, align 4      ; induction variable

  ; Initialize bounds
  store i32 0, ptr %.omp.lb
  store i32 99, ptr %.omp.ub
  store i32 1, ptr %.omp.stride

  ; Get this thread's ID
  %tid = load i32, ptr %.global_tid.

  ; Compute this thread's iteration range
  call void @__kmpc_for_static_init_4(
    %ident_t* @.loc, i32 %tid,
    i32 34,                          ; schedule type (static)
    ptr %.omp.is_last,
    ptr %.omp.lb, ptr %.omp.ub,
    ptr %.omp.stride,
    i32 1, i32 1                     ; incr, chunk
  )

  ; Clamp upper bound
  %ub = load i32, ptr %.omp.ub
  %cmp = icmp sgt i32 %ub, 99
  %ub.clamped = select i1 %cmp, i32 99, i32 %ub
  store i32 %ub.clamped, ptr %.omp.ub

  ; Load shared data pointers from captured struct
  %A.ptr = getelementptr %struct.anon, ptr %__context, i32 0, i32 0
  %A = load ptr, ptr %A.ptr
  %B.ptr = getelementptr %struct.anon, ptr %__context, i32 0, i32 1
  %B = load ptr, ptr %B.ptr
  %C.ptr = getelementptr %struct.anon, ptr %__context, i32 0, i32 2
  %C = load ptr, ptr %C.ptr

  ; The actual loop
  %lb = load i32, ptr %.omp.lb
  store i32 %lb, ptr %.omp.iv
  br label %omp.inner.for.cond

omp.inner.for.cond:
  %iv = load i32, ptr %.omp.iv
  %ub2 = load i32, ptr %.omp.ub
  %cond = icmp sle i32 %iv, %ub2
  br i1 %cond, label %omp.inner.for.body, label %omp.inner.for.end

omp.inner.for.body:
  %i = load i32, ptr %.omp.iv
  ; B[i] * C[i]
  %b.addr = getelementptr i32, ptr %B, i32 %i
  %b.val = load i32, ptr %b.addr
  %c.addr = getelementptr i32, ptr %C, i32 %i
  %c.val = load i32, ptr %c.addr
  %mul = mul i32 %b.val, %c.val
  ; A[i] = result
  %a.addr = getelementptr i32, ptr %A, i32 %i
  store i32 %mul, ptr %a.addr
  br label %omp.inner.for.inc

omp.inner.for.inc:
  %iv2 = load i32, ptr %.omp.iv
  %next = add i32 %iv2, 1
  store i32 %next, ptr %.omp.iv
  br label %omp.inner.for.cond

omp.inner.for.end:
  ; Clean up worksharing
  call void @__kmpc_for_static_fini(%ident_t* @.loc, i32 %tid)
  ; Barrier -- ensure all threads are done
  call void @__kmpc_barrier(%ident_t* @.loc, i32 %tid)
  ret void
}
```

### 6.2 Key Observations for AXIOM

1. **Shared variables are captured in a struct** and passed by pointer to the outlined function. This is exactly what AXIOM's `job_dispatch(func, data, total_items)` does, except AXIOM passes a single flat pointer instead of a structured capture.

2. **The runtime computes per-thread bounds** (`__kmpc_for_static_init_4`). AXIOM does this in the C runtime (`chunk = (total_items + num_workers - 1) / num_workers`).

3. **A barrier synchronizes after the loop** (`__kmpc_barrier`). AXIOM uses `job_wait()`.

4. **Private variables are allocated locally** in the outlined function. AXIOM has no concept of private vs. shared.

5. **`noalias` is applied to the thread ID parameters**, not to the data pointers. AXIOM applies `noalias` to ALL pointer parameters, which is overly aggressive.

### 6.3 Parallel Loop Metadata

For indicating that a loop has no loop-carried dependencies (enabling vectorization and other optimizations):

```llvm
for.body:
  %val = load i32, ptr %arrayidx, !llvm.access.group !1
  %mul = mul i32 %val, %val
  store i32 %mul, ptr %arrayidx2, !llvm.access.group !1
  br i1 %exitcond, label %for.end, label %for.body, !llvm.loop !0

!0 = distinct !{!0, !{!"llvm.loop.parallel_accesses", !1}}
!1 = distinct !{}
```

**Semantics:** The `!llvm.access.group !1` metadata on each memory instruction says "this access belongs to access group 1." The `!llvm.loop.parallel_accesses !1` metadata on the loop backedge says "all accesses in group 1 have no loop-carried dependencies."

This metadata is a **promise from the frontend** -- LLVM does not verify it. If the metadata is wrong, the result is undefined behavior.

**Nested loops:**
```llvm
; Inner loop accesses belong to group !3
; Outer loop accesses belong to group !4
; Inner loop: parallel w.r.t. !3
; Outer loop: parallel w.r.t. both !3 and !4
!1 = distinct !{!1, !{!"llvm.loop.parallel_accesses", !3}}
!2 = distinct !{!2, !{!"llvm.loop.parallel_accesses", !3, !4}}
!3 = distinct !{}
!4 = distinct !{}
```

### 6.4 Thread-Local Storage for Per-Thread State

```llvm
; Per-thread accumulator for reduction
@tls_sum = thread_local global i64 0, align 8

define void @worker_reduce(ptr %data, i32 %start, i32 %end) {
entry:
  ; Zero the thread-local accumulator
  store i64 0, ptr @tls_sum
  br label %loop

loop:
  %i = phi i32 [ %start, %entry ], [ %next, %loop ]
  %addr = getelementptr i32, ptr %data, i32 %i
  %val = load i32, ptr %addr
  %val64 = sext i32 %val to i64
  %old = load i64, ptr @tls_sum
  %new = add i64 %old, %val64
  store i64 %new, ptr @tls_sum
  %next = add i32 %i, 1
  %done = icmp sge i32 %next, %end
  br i1 %done, label %exit, label %loop

exit:
  ; After all threads finish, main thread combines tls_sum values
  ret void
}
```

---

## 7. Reduction Patterns

### 7.1 The Problem

Reductions (e.g., sum, max, min, product) require accumulating a value across parallel iterations. Naive shared accumulation is a data race:

```
// RACE CONDITION -- multiple threads read-modify-write sum simultaneously
sum = sum + array[i];
```

### 7.2 Approach 1: Atomic Accumulation

Each thread atomically adds to a shared accumulator:

```llvm
; atomicrmw add is atomic -- no race
%old = atomicrmw add ptr %sum_ptr, i64 %partial_val seq_cst
```

**Pros:** Simple, correct, minimal code
**Cons:** Extremely slow for high-contention reductions. On x86, `LOCK XADD` causes cache line bouncing between cores. For N iterations on P cores, creates P * (N/P) = N atomic operations.

**LLVM IR:**
```llvm
define void @reduce_atomic(ptr %data, ptr %sum, i32 %start, i32 %end) {
entry:
  br label %loop
loop:
  %i = phi i32 [ %start, %entry ], [ %next, %loop ]
  %addr = getelementptr i32, ptr %data, i32 %i
  %val = load i32, ptr %addr
  %val64 = sext i32 %val to i64
  %old = atomicrmw add ptr %sum, i64 %val64 seq_cst
  %next = add i32 %i, 1
  %done = icmp sge i32 %next, %end
  br i1 %done, label %exit, label %loop
exit:
  ret void
}
```

### 7.3 Approach 2: Thread-Local Accumulation + Final Combine

Each thread accumulates into a private variable; after all threads finish, the main thread combines results.

```
Per-thread:
  local_sum = 0
  for i in my_range:
    local_sum += data[i]
  thread_results[my_thread_id] = local_sum

Main thread (after barrier):
  total = 0
  for t in 0..num_threads:
    total += thread_results[t]
```

**Pros:** No contention during the parallel phase. Only P stores after the parallel phase (one per thread).
**Cons:** Requires per-thread storage and a final sequential combine step.

**LLVM IR (using TLS):**
```llvm
@tls_partial = thread_local global i64 0, align 8

define void @reduce_tls(ptr %data, i32 %start, i32 %end) {
entry:
  store i64 0, ptr @tls_partial    ; reset per-thread accumulator
  br label %loop
loop:
  %i = phi i32 [ %start, %entry ], [ %next, %loop ]
  %addr = getelementptr i32, ptr %data, i32 %i
  %val = load i32, ptr %addr
  %val64 = sext i32 %val to i64
  %old = load i64, ptr @tls_partial
  %new = add i64 %old, %val64
  store i64 %new, ptr @tls_partial
  %next = add i32 %i, 1
  %done = icmp sge i32 %next, %end
  br i1 %done, label %exit, label %loop
exit:
  ret void
}
```

**This is what OpenMP does.** The `__kmpc_reduce_nowait` call determines whether to use the primary-thread-combines path or the atomic path.

### 7.4 Approach 3: Tree Reduction

Combine partial results in a tree pattern: P threads produce P partial results, then log2(P) rounds of pairwise combining:

```
Thread 0: partial[0]    Thread 1: partial[1]    Thread 2: partial[2]    Thread 3: partial[3]
         \              /                                \              /
       partial[0] + partial[1]                        partial[2] + partial[3]
                    \                                    /
                  partial[0] + partial[1] + partial[2] + partial[3]
```

**Pros:** Optimal for GPU and SIMD. Parallel combining phase.
**Cons:** Complex to implement. Requires synchronization between rounds. Overkill for CPU with 4-16 cores.

### 7.5 Recommended Approach for AXIOM

**Thread-local accumulation with final combine** (Approach 2) is the right choice for CPU parallelism:

1. During codegen, when a `@parallel_for` has a `reduction(+: sum)` clause:
   - Emit a stack-allocated `local_sum` in the outlined worker function
   - Initialize `local_sum` to the identity value for the operator (0 for +, 1 for *, INT_MAX for min, etc.)
   - Replace references to `sum` in the loop body with `local_sum`
   - After the loop, atomically add `local_sum` to the shared `sum`

2. This means only ONE atomic operation per thread per reduction variable, not one per iteration.

**LLVM IR pattern for AXIOM:**
```llvm
; Worker function for parallel_for with reduction(+: total)
define void @worker(ptr %data, i32 %start, i32 %end, ptr %total) {
entry:
  %local_total = alloca i64, align 8
  store i64 0, ptr %local_total              ; identity for +
  br label %loop

loop:
  %i = phi i32 [ %start, %entry ], [ %next, %loop ]
  %addr = getelementptr i32, ptr %data, i32 %i
  %val = load i32, ptr %addr
  %val64 = sext i32 %val to i64
  %old = load i64, ptr %local_total
  %new = add i64 %old, %val64
  store i64 %new, ptr %local_total
  %next = add i32 %i, 1
  %done = icmp sge i32 %next, %end
  br i1 %done, label %combine, label %loop

combine:
  ; ONE atomic add per thread -- minimal contention
  %partial = load i64, ptr %local_total
  %old_total = atomicrmw add ptr %total, i64 %partial seq_cst
  ret void
}
```

### 7.6 Reduction Identity Values

| Operator | Identity | Notes |
|----------|----------|-------|
| `+` | 0 | Additive identity |
| `*` | 1 | Multiplicative identity |
| `min` | TYPE_MAX | Maximum representable value |
| `max` | TYPE_MIN | Minimum representable value |
| `and` (bitwise) | ~0 (all ones) | Bitwise AND identity |
| `or` (bitwise) | 0 | Bitwise OR identity |
| `xor` (bitwise) | 0 | Bitwise XOR identity |
| `&&` (logical) | true | Logical AND identity |
| `\|\|` (logical) | false | Logical OR identity |

---

## 8. Proposed Correct Designs for AXIOM

### 8.1 Approach A: Conservative -- OpenMP-Style Annotations

#### Design

```axiom
@parallel_for(
    private: [i, temp],
    shared_read: [positions],
    shared_write: [velocities],
    reduction(+: total_energy)
)
for i: i32 in range(0, n) {
    let temp: f64 = compute_energy(positions, i);
    velocities[i] = velocities[i] + temp;
    total_energy = total_energy + temp;
}
```

#### Annotation Semantics

| Clause | Meaning | Compiler Action |
|--------|---------|-----------------|
| `private: [vars]` | Each thread gets its own copy, uninitialized | Emit as stack alloca in outlined function |
| `firstprivate: [vars]` | Per-thread copy, initialized from original | Emit alloca + copy in outlined function entry |
| `shared_read: [vars]` | All threads can read, none can write | Emit with `readonly` attribute; error if any write detected |
| `shared_write: [vars]` | Threads write to non-overlapping ranges | Emit normally; compiler verifies non-overlapping via index analysis |
| `reduction(op: var)` | Thread-local accumulation with final combine | Emit local accumulator + atomic combine |

#### LLVM IR Mapping

The compiler would:

1. **Outline** the loop body into a separate function
2. **Capture** shared variables in a context struct
3. **Emit** per-thread private variables as local allocas
4. **Emit** reduction variables as local allocas with identity initialization
5. **Call** `axiom_job_dispatch` with the outlined function
6. **After `axiom_job_wait`**, no further combining needed (reductions already atomically accumulated)

```llvm
; Context struct for captured shared variables
%struct.par_ctx = type { ptr, ptr, ptr, i32 }
; Fields: positions, velocities, total_energy_ptr, n

; Outlined worker function
define internal void @parallel_worker(ptr %ctx, i32 %start, i32 %end) {
entry:
  ; Load shared data from context
  %positions = load ptr, ptr (getelementptr %struct.par_ctx, ptr %ctx, i32 0, i32 0)
  %velocities = load ptr, ptr (getelementptr %struct.par_ctx, ptr %ctx, i32 0, i32 1)
  %total_ptr = load ptr, ptr (getelementptr %struct.par_ctx, ptr %ctx, i32 0, i32 2)

  ; Private variable (per-thread)
  %temp = alloca double, align 8

  ; Reduction accumulator (per-thread)
  %local_energy = alloca double, align 8
  store double 0.0, ptr %local_energy    ; identity for +

  br label %loop
  ; ... loop body ...

combine:
  ; Atomic combine for reduction
  %partial = load double, ptr %local_energy
  ; For floating-point: use cmpxchg loop (atomicrmw fadd not universally available)
  call void @atomic_fadd(ptr %total_ptr, double %partial)
  ret void
}
```

#### Compiler Verification

For `shared_write` variables, the compiler MUST verify that each thread only writes to indices in its `[start, end)` range. This requires:

1. **Index analysis:** All writes to `shared_write` arrays must use the loop variable `i` (or a linear function of it) as the index
2. **No cross-range writes:** No write to `shared_write[j]` where `j` could be outside `[start, end)`
3. **Reject if unverifiable:** If the compiler cannot prove non-overlapping access, emit a compile error

```
ERROR: Cannot verify non-overlapping writes to 'velocities' in @parallel_for.
       Write at line 5: velocities[i] -- OK, i is loop variable.
       Write at line 7: velocities[i+1] -- ERROR: may access index outside [start, end).
```

#### Correctness Guarantees
- **Reductions:** Correct by construction (local accumulation + atomic combine)
- **Private variables:** No sharing, no races
- **shared_read:** Compiler-enforced read-only access
- **shared_write:** Compiler-verified non-overlapping access OR compile error
- **Memory model:** Atomic combine provides release semantics; `job_wait` provides acquire

#### Performance Implications
- Negligible overhead for simple parallel loops
- One atomic operation per reduction per thread per dispatch
- Outlining overhead is one function call per thread per dispatch
- No runtime dependency tracking needed

#### Implementation Complexity: MODERATE
- Extend parser to handle `@parallel_for` with clause syntax
- Extend HIR to represent data-sharing clauses
- Extend codegen to outline loop body, emit captures, emit reductions
- Implement index analysis for `shared_write` verification
- Modify `@pure` semantics to NOT apply to parallel worker functions

### 8.2 Approach B: Moderate -- Ownership-Based (Rust-Inspired)

#### Design

```axiom
// Slice types with access modes
@parallel
fn update(pos: slice[f64, readonly], vel: slice[f64, exclusive], dt: f64) {
    for i: i32 in range(0, len(vel)) {
        vel[i] = vel[i] + pos[i] * dt;
    }
}

// Usage: compiler proves slices don't overlap
fn main() -> i32 {
    let pos: slice[f64] = ...;
    let vel: slice[f64] = ...;
    parallel_for(update, pos.as_readonly(), vel.as_exclusive(), dt, len(vel));
}
```

#### Type System Additions

```
slice[T, readonly]    -- immutable view, can be shared across threads
slice[T, exclusive]   -- mutable view, only one thread can hold it
slice[T, shared]      -- (default) aliasing possible, not safe for parallelism
```

The compiler enforces:
- `readonly` slices cannot be written to
- `exclusive` slices cannot be aliased -- if you pass a slice as `exclusive`, you cannot also access it from another path
- `@parallel` functions can only accept `readonly` or `exclusive` slice parameters (not `shared`)

#### LLVM IR Mapping

```llvm
; readonly slices get: ptr noalias readonly
; exclusive slices get: ptr noalias
; The noalias is now CORRECT because the type system proves non-aliasing

define internal void @update(ptr noalias readonly %pos, i64 %pos_len,
                              ptr noalias %vel, i64 %vel_len,
                              double %dt,
                              i32 %start, i32 %end) {
  ; Only accesses vel[start..end] and pos[start..end]
  ; Compiler can verify this from the loop structure
  ...
}
```

#### How Split Works

When dispatching parallel work, the runtime splits `exclusive` slices:
```
Original: vel[0..1000], exclusive
Thread 0: vel[0..250],   exclusive  -- disjoint sub-slice
Thread 1: vel[250..500],  exclusive  -- disjoint sub-slice
Thread 2: vel[500..750],  exclusive  -- disjoint sub-slice
Thread 3: vel[750..1000], exclusive  -- disjoint sub-slice
```

Each sub-slice is `exclusive` because it does not overlap with any other sub-slice. The compiler/runtime guarantees this by construction (splitting at computed boundaries).

`readonly` slices are NOT split -- each thread gets a view of the full array, which is safe because no writes occur.

#### Correctness Guarantees
- **Exclusive access:** Type-system enforced. Cannot write to `readonly`; cannot alias `exclusive`.
- **Non-overlapping writes:** By construction -- `exclusive` slices are split into disjoint regions.
- **Memory model:** Splitting and joining provide implicit acquire/release semantics.
- **No `@pure` lie:** Functions marked `@parallel` have different semantics from `@pure` -- they are allowed to write to `exclusive` parameters.

#### Performance Implications
- Zero runtime overhead for safety checks (all compile-time)
- Slice splitting is computed once per dispatch (O(num_threads))
- Fat pointers (ptr + length) have slightly higher register pressure than raw pointers
- `readonly` enables LLVM to optimize more aggressively (hoist loads, eliminate stores)

#### Implementation Complexity: HIGH
- Requires slice types with access modes in the type system
- Requires access mode checking in the type checker
- Requires split/join operations in codegen
- Does NOT require full ownership/lifetime system (simpler than Rust)
- Does require proving that `exclusive` slices are not aliased at the call site

### 8.3 Approach C: Aggressive -- Dependency Graph (Naughty Dog-Inspired)

#### Design

```axiom
// Job handles enable dependency chaining
let j1: JobHandle = job_dispatch(compute_forces, bodies, n);
let j2: JobHandle = job_dispatch(integrate, bodies, n, depends_on: [j1]);
let j3: JobHandle = job_dispatch(render_prep, render_data, m, depends_on: [j1]);
// j2 and j3 can run in parallel after j1 completes
job_wait(j2);
job_wait(j3);
```

#### JobHandle Semantics

```axiom
struct JobHandle {
    counter: ptr[atomic_i32],    // atomic counter, decremented when jobs finish
    generation: u32,              // use-after-free detection
}
```

- `job_dispatch` returns a `JobHandle` whose counter starts at `num_chunks`
- `depends_on: [handles]` means "do not start until all listed handles reach zero"
- `job_wait(handle)` blocks (or yields fiber) until the counter reaches zero
- Multiple jobs can depend on the same handle (fan-out)
- A job can depend on multiple handles (fan-in / barrier)

#### Dependency Graph Example

```
Frame N:
  j_physics = job_dispatch(physics_step, ...)
  j_ai      = job_dispatch(ai_update, ..., depends_on: [])        // independent
  j_anim    = job_dispatch(animation, ..., depends_on: [j_physics]) // needs physics
  j_render  = job_dispatch(render, ..., depends_on: [j_anim, j_ai]) // needs both
  job_wait(j_render)
```

This forms a DAG:
```
j_physics -----> j_anim -----> j_render
                                  ^
j_ai     -------------------------/
```

#### LLVM IR Mapping

```llvm
; job_dispatch returns a handle (i64 encoding counter_index + generation)
%j1 = call i64 @axiom_job_dispatch_dep(ptr @compute_forces, ptr %bodies, i32 %n,
                                         i32 0, ptr null)
; 0 dependencies, null dependency array

; j2 depends on j1
%dep_array = alloca [1 x i64]
%dep0 = getelementptr [1 x i64], ptr %dep_array, i32 0, i32 0
store i64 %j1, ptr %dep0
%j2 = call i64 @axiom_job_dispatch_dep(ptr @integrate, ptr %bodies, i32 %n,
                                         i32 1, ptr %dep_array)

; Wait for j2
call void @axiom_job_wait_handle(i64 %j2)
```

#### Runtime Implementation Changes

The `axiom_rt.c` job system needs to be extended:

```c
typedef struct {
    AxiomJobFunc func;
    void        *data;
    int          start;
    int          end;
    volatile int *counter;     // atomic counter for this dispatch group
    int          num_deps;     // number of dependencies
    volatile int **dep_counters; // array of counters to wait on
} AxiomJobDep;

// Worker thread: before executing a job, spin/yield until all dep_counters are 0
static void axiom_worker_func_dep(AxiomJobDep *job) {
    // Wait for dependencies
    for (int d = 0; d < job->num_deps; d++) {
        while (__atomic_load_n(job->dep_counters[d], __ATOMIC_ACQUIRE) > 0) {
            // Yield to other fibers/jobs (or spin briefly)
            yield_fiber();
        }
    }
    // Execute
    job->func(job->data, job->start, job->end);
    // Signal completion
    __atomic_sub_fetch(job->counter, 1, __ATOMIC_RELEASE);
}
```

#### Combining with Safety Annotations

The dependency graph alone does NOT prevent data races within a single dispatch. It only orders dispatches. For full safety, combine with Approach A or B:

```axiom
// Safe: dependency ensures j1 completes before j2 starts
// Plus: annotations specify data access patterns
let j1: JobHandle = @parallel_for(shared_write: [forces])
    job_dispatch(compute_forces, physics_ctx, n);

let j2: JobHandle = @parallel_for(shared_read: [forces], shared_write: [positions])
    job_dispatch(integrate, physics_ctx, n, depends_on: [j1]);
```

#### Correctness Guarantees
- **Inter-job ordering:** Dependencies provide happens-before relationships
- **Memory visibility:** Atomic counter decrement (release) + dependency wait (acquire) = full synchronization
- **Intra-job safety:** NOT guaranteed by dependencies alone -- needs Approach A or B annotations
- **Deadlock freedom:** Only if dependency graph is a DAG (no cycles). Compiler could verify this for static graphs.

#### Performance Implications
- Dependency checking adds overhead per job launch (scanning dep_counters)
- Fiber-based waiting avoids thread blocking (high CPU utilization)
- Enables overlapping independent work (e.g., physics and AI in parallel)
- Counter-based tracking is O(1) per dependency check

#### Implementation Complexity: HIGH
- Extend runtime with dependency-aware job queue
- Implement fiber system (or use OS-level fibers: Windows fibers, ucontext on POSIX)
- Handle counter allocation, generation tracking, use-after-free detection
- Dependency cycle detection (optional but recommended)
- Integrate with Approach A or B for intra-job safety

---

## 8.4 Comparison of Approaches

| Aspect | A: OpenMP-style | B: Ownership-based | C: Dependency graph |
|--------|-----------------|---------------------|---------------------|
| **Safety level** | High (with verification) | Highest (type-system) | Medium (ordering only) |
| **Data race prevention** | Annotation + analysis | Type system enforced | Not inherent |
| **Reduction support** | Yes (built-in) | Via explicit combine | Manual |
| **Job dependencies** | No | No | Yes |
| **Learning curve** | Low (familiar to C/C++ devs) | Medium (new concepts) | Medium |
| **Implementation effort** | Moderate | High | High |
| **Runtime overhead** | Minimal | Minimal | Per-dependency check |
| **Composability** | Low (flat loops only) | Medium (function-level) | High (arbitrary DAGs) |
| **LLVM IR complexity** | Moderate (outlining) | Low (attribute-based) | High (counter management) |

### 8.5 Recommended Phased Implementation

**Phase 1 (Immediate): Fix critical bugs**
- Remove `@pure` from functions that write through pointers
- Stop emitting `nosync` on functions called from parallel contexts
- Only emit `noalias` on pointer params when provably correct
- Add a `@job` annotation that is distinct from `@pure`
- Emit a compiler warning when `@pure` is used on functions with pointer writes

**Phase 2 (Short-term): Implement Approach A (OpenMP-style)**
- Add `@parallel_for` annotation with data-sharing clauses
- Implement loop body outlining in codegen
- Implement reduction pattern (local accumulate + atomic combine)
- Implement index analysis for `shared_write` verification
- Add `@parallel_for` lowering to use existing `axiom_job_dispatch` runtime

**Phase 3 (Medium-term): Implement Approach B (Ownership-based slices)**
- Add `slice[T, readonly]` and `slice[T, exclusive]` types
- Implement access mode checking in type checker
- Implement automatic slice splitting for parallel dispatch
- Allow `@parallel` functions with typed slice parameters

**Phase 4 (Long-term): Implement Approach C (Dependency graph)**
- Extend runtime with `JobHandle` and dependency-aware scheduling
- Implement fiber-based non-blocking waits
- Add dependency cycle detection
- Integrate with Phase 2/3 safety annotations

---

## 9. Implementation Roadmap

### 9.1 Immediate Actions (Fix Bugs)

#### Fix 1: Stop lying about `@pure`

In `crates/axiom-codegen/src/llvm.rs`, the `build_func_attr_suffix` function:

**Current (WRONG):**
```rust
} else if annots.is_pure {
    if annots.reads_arg_memory {
        attrs.push("memory(argmem: read)");  // WRONG if function writes through ptr!
    }
    attrs.push("nosync");                     // WRONG for parallel functions!
}
```

**Needed:** Either:
1. Make `@pure` actually mean pure (no writes at all), and introduce `@job` for parallel work functions
2. Or check whether the function body contains any pointer writes and emit `memory(argmem: readwrite)` instead

Introduce a new annotation:
```
@job  -- function can be dispatched as parallel work. May read/write through pointer args.
         NOT pure. NOT nosync. Emits: memory(argmem: readwrite) nounwind
```

#### Fix 2: Conditional `noalias`

In `build_params_str`:

**Current (WRONG):**
```rust
if llvm_type == "ptr" {
    parts.push(format!("ptr noalias %{}", param.name));
}
```

**Needed:** Only emit `noalias` when the function is NOT called from a parallel context with shared data, OR when the type system proves the pointer is exclusive:

```rust
if llvm_type == "ptr" {
    if func_annots.is_const {
        // @const functions truly don't alias (no memory access)
        parts.push(format!("ptr noalias %{}", param.name));
    } else if func_annots.is_pure && !func_annots.writes_through_ptr {
        // @pure functions that only read can have noalias
        parts.push(format!("ptr noalias readonly %{}", param.name));
    } else {
        // Default: no noalias claim
        parts.push(format!("ptr %{}", param.name));
    }
}
```

#### Fix 3: Add memory fence awareness

After `job_wait()`, the compiler should not reorder reads above it. Since `axiom_job_wait` is an opaque external call, LLVM already treats it conservatively. But if LLVM ever gets cross-module optimization or LTO, this could break. Add a compiler fence:

```llvm
call void @axiom_job_wait()
fence acquire                    ; ensure all worker writes are visible
```

### 9.2 Short-Term: @parallel_for Implementation

#### Parser Changes

Add to the annotation parser:
```
@parallel_for(
    [private: [ident_list],]
    [firstprivate: [ident_list],]
    [shared_read: [ident_list],]
    [shared_write: [ident_list],]
    [reduction(op: ident_list),]
)
```

#### HIR Changes

Add a new annotation kind:
```rust
ParallelFor {
    private: Vec<String>,
    firstprivate: Vec<String>,
    shared_read: Vec<String>,
    shared_write: Vec<String>,
    reductions: Vec<(ReductionOp, Vec<String>)>,
}
```

#### Codegen Changes

When encountering a `for` loop with `@parallel_for` annotation:

1. **Outline** the loop body into a new function with signature `(ptr %ctx, i32 %start, i32 %end)`
2. **Build capture struct** containing all shared variables
3. **Emit local allocas** for private and firstprivate variables
4. **Emit reduction locals** initialized to identity values
5. **After the loop**, emit atomic combine for each reduction variable
6. **Replace the original loop** with:
   ```llvm
   call void @axiom_jobs_init(i32 %num_cores)
   call void @axiom_job_dispatch(ptr @outlined_worker, ptr %ctx, i32 %n)
   call void @axiom_job_wait()
   fence acquire
   ```

### 9.3 Required Runtime Changes

The current `axiom_rt.c` runtime is adequate for Phase 1-2 with minor additions:

1. **Add `axiom_job_dispatch_dep`** for dependency-aware dispatch (Phase 4)
2. **Add `axiom_job_wait_handle`** for waiting on specific handles (Phase 4)
3. **Add `axiom_atomic_add_f64`** for floating-point atomic accumulation (Phase 2):
   ```c
   double axiom_atomic_add_f64(volatile double *ptr, double val) {
       // CAS loop for atomic float add (no hardware support on most CPUs)
       union { double d; uint64_t u; } old_val, new_val;
       do {
           old_val.u = __atomic_load_n((volatile uint64_t*)ptr, __ATOMIC_RELAXED);
           new_val.d = old_val.d + val;
       } while (!__atomic_compare_exchange_n(
           (volatile uint64_t*)ptr, &old_val.u, new_val.u,
           0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST));
       return old_val.d;
   }
   ```

---

## 10. References

### LLVM Documentation
- LLVM Language Reference Manual: Memory Model for Concurrent Operations -- https://llvm.org/docs/LangRef.html
- LLVM Atomic Instructions and Concurrency Guide -- https://llvm.org/docs/Atomics.html
- LLVM Code Transformation Metadata -- https://llvm.org/docs/TransformMetadata.html
- LLVM Alias Analysis Infrastructure -- https://llvm.org/docs/AliasAnalysis.html
- D52116: Introduce llvm.loop.parallel_accesses and llvm.access.group metadata -- https://reviews.llvm.org/D52116
- Representing Parallelism Within LLVM (Hal Finkel) -- https://llvm.org/devmtg/2018-04/slides/Finkel-Representing%20Parallelism%20Within%20LLVM.pdf
- Restrict-qualified pointers in LLVM (Hal Finkel) -- https://llvm.org/devmtg/2017-02-04/Restrict-Qualified-Pointers-in-LLVM.pdf

### OpenMP and LLVM
- OpenMP Support in Clang/LLVM -- https://clang.llvm.org/docs/OpenMPSupport.html
- LLVM/OpenMP Runtimes documentation -- https://openmp.llvm.org/design/Runtimes.html
- Analyzing Clang OpenMP work-sharing for loop AST and LLVM IR -- https://yunmingzhang.wordpress.com/2015/11/06/analyzing-clang-openmp-work-sharing-for-loop-ast-and-llvm-ir/
- OpenMP specification: Data-Sharing Attribute Clauses -- https://www.openmp.org/spec-html/5.0/openmpsu106.html
- OpenMP specification: Reduction Clause -- https://www.openmp.org/spec-html/5.2/openmpsu52.html
- LLNL HPC Tutorials: OpenMP Reduction Clause -- https://hpc-tutorials.llnl.gov/openmp/reduction_clause/

### Language Design for Safe Parallelism
- How Rust makes Rayon's data parallelism magical (Red Hat) -- https://developers.redhat.com/blog/2021/04/30/how-rust-makes-rayons-data-parallelism-magical
- Rayon: data parallelism in Rust (Niko Matsakis) -- https://smallcultfollowing.com/babysteps/blog/2015/12/18/rayon-data-parallelism-in-rust/
- Fearless Concurrency -- The Rust Programming Language -- https://doc.rust-lang.org/book/ch16-00-concurrency.html
- Chapel Forall Loops documentation -- https://chapel-lang.org/docs/primers/forallLoops.html
- Chapel Data Parallelism specification -- https://chapel-lang.org/docs/language/spec/data-parallelism.html
- Swift Actors proposal (SE-0306) -- https://github.com/apple/swift-evolution/blob/main/proposals/0306-actors.md
- Eliminate data races using Swift Concurrency (WWDC22) -- https://developer.apple.com/videos/play/wwdc2022/110351/
- Java Structured Concurrency and Scoped Values -- https://softwaremill.com/structured-concurrency-and-scoped-values-in-java/
- Beyond Loom: Weaving new concurrency patterns (Red Hat) -- https://developers.redhat.com/articles/2023/10/03/beyond-loom-weaving-new-concurrency-patterns
- Cilk (Wikipedia) -- https://en.wikipedia.org/wiki/Cilk
- Zig's new async I/O -- https://kristoff.it/blog/zig-new-async-io/

### Game Engine Job Systems
- Parallelizing the Naughty Dog Engine Using Fibers (GDC 2015, Christian Gyrling) -- https://gdcvault.com/play/1022186/Parallelizing-the-Naughty-Dog-Engine
- Fiber-based Job System (Dangling Pointers blog) -- https://danglingpointers.com/post/job-system/
- FiberTaskingLib (RichieSams) -- https://github.com/RichieSams/FiberTaskingLib

### Dependency Analysis and Parallelization Theory
- Loop dependence analysis (Wikipedia) -- https://en.wikipedia.org/wiki/Loop_dependence_analysis
- Automatic parallelization (Wikipedia) -- https://en.wikipedia.org/wiki/Automatic_parallelization
- GCD test (Wikipedia) -- https://en.wikipedia.org/wiki/GCD_test
- The Polyhedral Model Is More Widely Applicable Than You Think -- https://link.springer.com/chapter/10.1007/978-3-642-11970-5_16
- Ownership Types for Safe Programming: Preventing Data Races -- https://web.eecs.umich.edu/~bchandra/publications/oopsla02.pdf
- Uniqueness and Reference Immutability for Safe Parallelism -- https://www.semanticscholar.org/paper/Uniqueness-and-reference-immutability-for-safe-Gordon-Parkinson/7144951bb3fa665b12b1477438a4650e69be0e4b
- A Type and Effect System for Deterministic Parallel Java -- https://dl.acm.org/doi/10.1145/1639949.1640097
- Parallel Programming Must Be Deterministic by Default (Bocchino et al.) -- https://www.usenix.org/legacy/event/hotpar09/tech/full_papers/bocchino/bocchino_html/index.html

### Parallel Reduction
- Optimizing Parallel Reduction in CUDA (Mark Harris, NVIDIA) -- https://developer.download.nvidia.com/assets/cuda/files/reduction.pdf
- Faster Parallel Reductions on Kepler (NVIDIA) -- https://developer.nvidia.com/blog/faster-parallel-reductions-kepler/

### Data Race Detection
- Go Data Race Detector -- https://go.dev/doc/articles/race_detector
- Introducing the Go Race Detector (Go blog) -- https://go.dev/blog/race-detector
- ThreadSanitizer (LLVM project) -- used by Go's race detector

---

## Appendix A: Summary of AXIOM Source Files Analyzed

| File | Relevance |
|------|-----------|
| `crates/axiom-codegen/src/llvm.rs` | LLVM IR generation; `@pure`/`noalias` emission; job system builtin codegen |
| `crates/axiom-driver/runtime/axiom_rt.c` | C runtime: thread pool, job queue, atomics, mutex implementations |
| `crates/axiom-hir/src/hir.rs` | HIR node definitions; annotation kinds including `@pure`, `@parallel` |
| `tests/samples/jobs_test.axm` | Test file demonstrating current (broken) job system usage |
| `.pipeline/milestones/M7.4-job-system.json` | Milestone definition for job system |

## Appendix B: LLVM IR Quick Reference for Atomic Operations

```llvm
; === Atomic Load ===
%v = load atomic i32, ptr %p monotonic, align 4
%v = load atomic i32, ptr %p acquire, align 4
%v = load atomic i32, ptr %p seq_cst, align 4

; === Atomic Store ===
store atomic i32 %v, ptr %p monotonic, align 4
store atomic i32 %v, ptr %p release, align 4
store atomic i32 %v, ptr %p seq_cst, align 4

; === Atomic Read-Modify-Write ===
%old = atomicrmw add  ptr %p, i32 %v seq_cst    ; fetch-and-add
%old = atomicrmw sub  ptr %p, i32 %v acq_rel     ; fetch-and-sub
%old = atomicrmw xchg ptr %p, i32 %v acquire      ; exchange
%old = atomicrmw and  ptr %p, i32 %v release      ; fetch-and-and
%old = atomicrmw or   ptr %p, i32 %v monotonic    ; fetch-and-or
%old = atomicrmw max  ptr %p, i32 %v seq_cst      ; fetch-and-max (signed)
%old = atomicrmw umax ptr %p, i32 %v seq_cst      ; fetch-and-max (unsigned)

; === Compare-and-Swap ===
%res = cmpxchg ptr %p, i32 %expected, i32 %desired acq_rel monotonic
; Returns { i32, i1 }: old value + success flag
%old = extractvalue { i32, i1 } %res, 0
%ok  = extractvalue { i32, i1 } %res, 1

; === Fence ===
fence acquire
fence release
fence acq_rel
fence seq_cst

; === Thread-Local Storage ===
@tls_var = thread_local global i32 0, align 4
@tls_var = thread_local(initialexec) global i32 0, align 4

; === Parallel Loop Metadata ===
; On memory instructions:
  %v = load i32, ptr %addr, !llvm.access.group !1
  store i32 %v, ptr %addr2, !llvm.access.group !1
; On loop backedge:
  br i1 %cond, label %loop, label %exit, !llvm.loop !0
; Metadata definitions:
  !0 = distinct !{!0, !{!"llvm.loop.parallel_accesses", !1}}
  !1 = distinct !{}

; === noalias on function parameters ===
define void @f(ptr noalias %a, ptr noalias readonly %b) { ... }
```
