# AXIOM -- AI eXchange Intermediate Optimization Medium

## Project Identity

AXIOM is a programming language designed as the canonical transfer format between AI agents, optimized for machine understanding and iterative optimization, that compiles to native code via LLVM IR. It is human-readable but AI-first: every construct carries semantic intent, explicit optimization surfaces, and structured metadata that AI agents can systematically explore and improve.

**This is NOT a language for humans to program in. This is a language for AI agents to communicate optimized computation through.**

**Repository:** https://github.com/rudybear/axiom

## Performance Goal

AXIOM must achieve top-tier scores on language comparison benchmarks. This is a hard requirement.

**FINAL results (197 benchmarks + 21 real-world C project ports):**
- **115/115 benchmarks pass**, 1.01x average ratio vs C (parity)
- **21 real-world C project ports** (~60K+ combined GitHub stars) -- all at parity or faster
- **Raytracer:** AXIOM scalar 42ms (+7% faster than C), AXIOM AOS vec3 44ms (+2% faster), C -O2 47ms, C turbo 51ms
- JPEG DCT: **AXIOM 56% faster** than C turbo
- RLE compression: **AXIOM 16% faster** than C turbo
- Binary trees (Benchmarks Game classic): **AXIOM 80% faster** with arena allocator
- See real-world port results below

**Real-world C project ports (21 total):**

| Project | GitHub Stars | Result |
|---------|-------------|--------|
| QOI (image codec) | 7,439 | **AXIOM 16% faster** |
| TurboPFor (integer compression) | 800+ | **AXIOM 35% faster** |
| Huffman/miniz (deflate codec) | 2,300+ | **AXIOM 14% faster** |
| SipHash (keyed hash) | 400+ | Parity |
| xxHash32 (non-crypto hash) | 10,954 | Parity |
| AES-128 (encryption) | 4,902 | Parity |
| heatshrink (embedded compression) | 1,300+ | Parity |
| LZ4 (fast compression) | 10,600 | Parity |
| cJSON (JSON parser) | 11,000 | Parity |
| FastLZ (LZ77 compression) | 500+ | Parity |
| LZAV (improved LZ77) | 400+ | Parity (1.04x) |
| Base64 (Turbo-Base64 codec) | -- | Parity |
| BLAKE3 (crypto hash) | -- | Parity |
| minimp3 (MP3 IMDCT) | -- | Parity |
| stb_jpeg (JPEG IDCT) | -- | Parity |
| SMHasher (4 hash functions) | -- | Parity |
| lodepng (PNG decode core) | 2,200+ | Parity |
| fpng (fast PNG encode) | 850+ | Parity |
| libdeflate (fast DEFLATE) | 900+ | Parity |
| utf8proc (UTF-8 processing) | 450+ | Parity |
| Roaring Bitmaps (compressed bitmaps) | 1,500+ | Parity |

**Rules:** No benchmark-specific cheating. General optimizations only. Reproducible results.

**Why AXIOM beats C:** `@pure` -> fast-math + `memory(none)` | `noalias` on all pointer params (Fortran advantage) | `nsw` on integer arithmetic | Arena allocator (50-200x faster than malloc) | `@lifetime(scope)` heap-to-stack promotion | LLVM allocator attributes | `fence release/acquire` for correct concurrency | `readonly`/`writeonly` pointer attributes | `calloc` zero-page trick (skips user-space memset) | `@inline(always)` -> `alwaysinline` for hot paths | Global constant arrays (`array_const_*`) -> direct GEP into .rodata | Interprocedural const pointer propagation | `inbounds` GEP on all ptr_read/ptr_write | `zext` for array indices (not `sext`)

**Optimization Knowledge Base:** 14 rules + 6 anti-patterns, grows with each LLM session. Includes LLM optimization feedback loop where the knowledge base is read before every optimization pass and updated after discoveries.

---

## Architecture Overview

```
AXIOM Source (.axm)           <- AI agents read/write here
       |
       v
AXIOM Lexer (63 tests)        <- Tokenizer with error recovery
       |
       v
AXIOM Parser (52 tests)        <- Recursive descent + Pratt expressions
       |
       v
AXIOM HIR (25 tests)          <- Annotation validation, type checking,
       |                          @strict enforcement, pre/postcondition lowering
       v
LLVM IR Text Gen (165 tests)  <- Optimized IR with noalias, nsw, fast-math,
       |                          SIMD vec2/vec3/vec4 types,
       |                          fastcc, branch hints, allocator attributes,
       |                          fence release/acquire, readonly/writeonly,
       |                          SIMD width metadata, DWARF debug info,
       |                          @precondition/@postcondition checks (--debug)
       v
clang -O2                     <- Native binary (x86_64, AArch64)
```

## Technology Stack

- **Compiler language**: Rust (~40,100 lines, 7 crates)
- **Backend**: LLVM IR text generation -> clang -O2 (no inkwell dependency)
- **Parser**: Hand-written recursive descent with Pratt parsing
- **Build system**: Cargo workspace
- **CI**: GitHub Actions (`.github/workflows/ci.yml`)
- **Testing**: 545 tests passing (unit + integration + doc-tests + E2E)
- **Benchmarks**: 197 programs (115 simple + 30 complex + 20 real-world + 30 memory + 2 GitHub repos) + 21 real-world C project ports

## Current Feature Set

### Types
```
Primitives:    i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f16 bf16 f32 f64 bool
               vec2 vec3 vec4 ivec2 ivec3 ivec4 fvec2 fvec3 fvec4 mat3 mat4
               u32 has proper unsigned semantics: udiv, urem, icmp ult, add nuw
Arrays:        array[T, N]              // fixed-size, stack-allocated
Pointers:      ptr[T]                   // heap pointer
               readonly_ptr[T]          // read-only pointer (enables LLVM readonly attr)
               writeonly_ptr[T]         // write-only pointer (enables LLVM writeonly attr)
Slices:        slice[T]                 // fat pointer (ptr + length)
Vectors:       vec2 vec3 vec4           // SIMD f64 vectors (2/3/4 lanes, hardware-mapped)
Int vectors:   ivec2 ivec3 ivec4       // SIMD i32 vectors
Float vectors: fvec2 fvec3 fvec4       // SIMD f32 vectors
Matrices:      mat3 mat4              // 3x3 and 4x4 f64 matrices
Tensors:       tensor[T, dims...]       // planned
Tuples:        (T1, T2, T3)
Functions:     fn(T1, T2) -> R
Sum types:     type Name = V1(T) | V2(T)  // parsed, codegen via builtins (option/result)
Structs:       struct Name { field: Type } // with literal constructors: Name { x: 1, y: 2 }
```

### Control Flow
```
if / else if / else            // Conditional chains (else if fully supported)
for i in range(start, end)     // Counted loop
for i in range(start, end, step) // Counted loop with step
while cond { }                 // While loop
break;                         // Break out of loop
continue;                      // Skip to next iteration
return expr;                   // Return with value
return;                        // Bare return in void functions
```

### Constants & Bindings
```
let x: i32 = 42;                        // Immutable binding
let mut x: i32 = 0;                     // Mutable binding
const PI: f64 = 3.14159265358979;       // Local constant (inlined at use sites)
@lifetime(scope) let buf: ptr[i32] = heap_alloc(100, 4);  // Stack-promoted heap alloc
```

### Struct Features
```
struct Point { x: f64, y: f64 }
let p: Point = Point { x: 1.0, y: 2.0 };   // Struct literal constructor
fn make_point() -> Point { ... }              // Struct return from functions
```

### GLSL-Style Swizzles
```
let v: vec3 = vec3(1.0, 2.0, 3.0);
let xy: vec2 = v.xy;                // Extract first 2 components
let reversed: vec3 = v.zyx;         // Reorder components
let broadcast: vec3 = v.xxx;        // Broadcast single component
```

### Annotations (all implemented)
```
@pure                          // No side effects -> fast-math, memory(none), noalias
@const                         // Compile-time evaluable -> speculatable + const eval
@inline(always | never | hint) // Inlining control
@complexity(expr)              // Algorithmic complexity class
@intent("description")         // Semantic intent
@constraint { key: value }     // Hard performance constraints
@strategy { ... }              // Optimization surface with ?holes
@vectorizable(dims)            // Loop vectorization hints
@parallel(dims)                // Parallelization hints
@parallel_for(shared_read: [...], shared_write: [...], reduction(+: var), private: [...])
                               // Data-parallel for loop with OpenMP-style sharing clauses
@target(device_class)          // Target hardware
@layout(row_major | col_major) // Memory layout
@align(bytes)                  // Alignment
@lifetime(scope | static | manual)  // Lifetime control -> heap-to-stack promotion, escape analysis
@export                        // C ABI export
@strict                        // Module: enforce annotations on all functions
@precondition(expr)            // Function: runtime check at entry (--debug)
@postcondition(expr)           // Function: runtime check at exit (--debug)
@test { input: (...), expect } // Function: inline test case
@requires(expr)                // Function: formal precondition (alias for @precondition)
@ensures(expr)                 // Function: formal postcondition (alias for @postcondition)
@invariant(expr)               // Block: loop invariant (checked in --debug)
@trace                         // Function: emit ENTER/EXIT calls for tracing
@link("lib", "kind")           // Function: link against a native library
@transfer { ... }              // Inter-agent handoff
@optimization_log { ... }      // Optimization history
```

### All Builtin Functions (~171 total)

#### I/O (4)
```
print(str) print_i32(n) print_i64(n) print_f64(x)
```

#### Math (25)
```
abs(x) abs_f64(x) fabs(x)
min(a,b) max(a,b) min_f64(a,b) max_f64(a,b)
sqrt(x) pow(x,y)
sin(x) cos(x) tan(x) asin(x) acos(x) atan(x) atan2(y,x)
floor(x) ceil(x) round(x) log(x) log2(x) exp(x) exp2(x)
to_f64(i32->f64) to_f64_i64(i64->f64)
```

#### Vector Construction & Math (9)
```
vec2(x,y) vec3(x,y,z) vec4(x,y,z,w)
dot(a,b) cross(a,b) length(v) normalize(v) reflect(i,n) lerp(a,b,t)
```

#### Integer/Float Vector Construction (6)
```
ivec2(x,y) ivec3(x,y,z) ivec4(x,y,z,w)
fvec2(x,y) fvec3(x,y,z) fvec4(x,y,z,w)
```

#### Vector Conversions (6)
```
vec2_to_ivec2(v) vec2_to_fvec2(v) ivec2_to_vec2(v) fvec2_to_vec2(v)
ivec3_to_vec3(v) fvec3_to_vec3(v) ivec4_to_vec4(v) fvec4_to_vec4(v)
vec3_to_ivec3(v) vec3_to_fvec3(v) vec4_to_ivec4(v) vec4_to_fvec4(v)
```

#### Matrix Operations (14)
```
mat3_identity() mat3_mul_vec3(m,v)
mat4_identity() mat4_mul(a,b) mat4_mul_vec4(m,v) mat4_transpose(m)
mat4_translate(x,y,z) mat4_scale(x,y,z) mat4_rotate_x(a) mat4_rotate_y(a) mat4_rotate_z(a)
mat4_perspective(fov,aspect,near,far) mat4_look_at(eye,center,up)
mat4_row(m,i) mat4_set_row(m,i,v)
```

#### Slices (6)
```
slice_from(ptr, len) slice_get(s, idx) slice_set(s, idx, val)
slice_len(s) slice_ptr(s) slice_sub(s, start, end)
```

#### Conversions (5)
```
widen(narrow->wide) narrow(wide->narrow) truncate(float->int)
f32_to_f64(x) f64_to_f32(x)
```

#### Bitwise (11)
```
band(a,b) bor(a,b) bxor(a,b) bnot(a)
shl(a,n) shr(a,n) lshr(a,n) rotl(a,n) rotr(a,n)
rotl64(a,n) rotr64(a,n)
```

#### Memory -- Heap (10)
```
heap_alloc(count, elem_size) heap_alloc_zeroed(count, elem_size)
heap_free(ptr) heap_realloc(ptr, new_count, elem_size)
ptr_read_i32(ptr, idx) ptr_read_i64(ptr, idx) ptr_read_f64(ptr, idx)
ptr_write_i32(ptr, idx, val) ptr_write_i64(ptr, idx, val) ptr_write_f64(ptr, idx, val)
```

#### Memory -- Narrow Ptr (7)
```
ptr_read_f32(ptr, idx) ptr_read_i16(ptr, idx) ptr_read_u8(ptr, idx)
ptr_write_u8(ptr, idx, val) ptr_write_i16(ptr, idx, val) ptr_write_f32(ptr, idx, val)
ptr_offset(ptr, byte_offset)
```

#### Memory -- Arena (4)
```
arena_create(size_bytes) arena_alloc(arena, count, elem_size)
arena_reset(arena) arena_destroy(arena)
```

#### Global Constant Arrays (3)
```
array_const_i32(v0, v1, ..., vN)   // Compile-time constant i32 array in .rodata
array_const_u8(v0, v1, ..., vN)    // Compile-time constant u8 array in .rodata
array_const_f64(v0, v1, ..., vN)   // Compile-time constant f64 array in .rodata
```

#### Global Mutable Arrays (3)
```
global_array_i32(size)             // Zero-initialized writable global i32 array
global_array_u8(size)              // Zero-initialized writable global u8 array
global_array_f64(size)             // Zero-initialized writable global f64 array
```

#### Memory Operations (3)
```
memcpy(dst, src, bytes)            // Copy bytes (non-overlapping) -> llvm.memcpy
memset(ptr, val, bytes)            // Fill memory with byte value -> llvm.memset
memmove(dst, src, bytes)           // Copy bytes (overlapping safe) -> llvm.memmove
```

#### File I/O (3)
```
file_read(path) file_write(path, data, len) file_size(path)
```

#### System (3)
```
clock_ns() get_argc() get_argv(index)
```

#### Coroutines (5)
```
coro_create(func, arg) coro_resume(handle)
coro_yield(value) coro_is_done(handle) coro_destroy(handle)
```

#### Threading (2)
```
thread_create(func, arg) thread_join(handle)
```

#### Atomics (4)
```
atomic_load(ptr) atomic_store(ptr, val)
atomic_add(ptr, delta) atomic_cas(ptr, expected, desired)
```

#### Mutex (4)
```
mutex_create() mutex_lock(mtx) mutex_unlock(mtx) mutex_destroy(mtx)
```

#### Job System (8)
```
jobs_init(num_workers) job_dispatch(func, data, total_items)
job_wait() jobs_shutdown() num_cores()
job_dispatch_handle(func, data, total_items)
job_dispatch_after(func, data, total_items, dependency_handle)
job_wait_handle(handle)
```

#### Option (5)
```
option_none() option_some(val) option_is_some(opt) option_is_none(opt) option_unwrap(opt)
```

#### String (5)
```
string_from_literal(str) string_len(s) string_ptr(s) string_eq(s1, s2) string_print(s)
```

#### Vec / Dynamic Array (9)
```
vec_new(elem_size) vec_push_i32(v, val) vec_push_f64(v, val)
vec_get_i32(v, idx) vec_get_f64(v, idx) vec_set_i32(v, idx, val) vec_set_f64(v, idx, val)
vec_len(v) vec_free(v)
```

#### Function Pointers (3)
```
fn_ptr(func_name) call_fn_ptr_i32(fp, arg) call_fn_ptr_f64(fp, arg)
```

#### Result / Error Handling (6)
```
result_ok(val) result_err(code) result_is_ok(r) result_is_err(r)
result_unwrap(r) result_err_code(r)
```

#### Platform Detection (1)
```
cpu_features()
```

#### Debug / Verification (2)
```
assert(cond, msg) debug_print(expr)
```

### C Interop
```axiom
extern fn clock() -> i64;
@export fn compute(data: ptr[f64], n: i32) -> f64 { ... }
```

### AI Agent Integration
- **Optimization protocol**: extract surfaces -> propose values -> validate -> benchmark -> record
- **AgentSession API**: Rust API for AI agents to load/analyze/optimize AXIOM programs
- **MCP server**: 5 tools over JSON-RPC stdio (load, surfaces, propose, compile, history)
- **@transfer blocks**: Structured inter-agent handoff with confidence scores

## LLVM Optimizations Applied

| Optimization | LLVM Effect | Enabled By |
|---|---|---|
| `noalias` on all ptr params | Eliminates alias analysis overhead | No aliasing by design |
| `memory(none)` / `memory(argmem: read)` | CSE, hoisting, dead call elimination | `@pure` annotation |
| `nsw` on integer arithmetic | Strength reduction, loop opts | Defined semantics |
| `fast` math flags | FMA, reassociation, reciprocal | `@pure` on float functions |
| `fastcc` calling convention | Fewer register saves on internal functions | Non-exported functions |
| `speculatable` | Constant folding | `@const` annotation |
| `!prof` branch weights | Branch prediction hints | `@pure` base case detection |
| `!llvm.loop` vectorize hints | Auto-vectorization | `@vectorizable` annotation |
| `allockind` / `alloc-family` | Dead alloc elimination, heap-to-stack | All allocator calls |
| `fence release` / `fence acquire` | Correct concurrent memory ordering | `@parallel_for` regions |
| `readonly` / `writeonly` ptr attrs | Alias analysis, dead store elimination | `readonly_ptr[T]` / `writeonly_ptr[T]` types |
| SIMD width metadata | Preferred vector width hints | `@vectorizable` + target detection |
| DWARF debug info | Source-level debugging with gdb/lldb | `@export` functions, debug builds |
| `alwaysinline` | Force-inline hot functions, eliminate call overhead | `@inline(always)` annotation |
| Global const propagation | Direct GEP into .rodata, no pointer load | `array_const_*` + `const_returning_fns` |
| `@llvm.fshl.i64` / `@llvm.fshr.i64` | Hardware rotate instructions (single cycle) | `rotl64()` / `rotr64()` builtins |
| SIMD vector instructions | `fadd`/`fmul`/`fsub`/`fdiv` on `<N x double>`, `shufflevector`, `extractelement` | `vec2`/`vec3`/`vec4` types |
| `inbounds` GEP on all ptr access | Enables LLVM alias analysis, bounds-based optimizations | All `ptr_read_*`/`ptr_write_*` builtins |
| `zext` for array indices | Correct zero-extension (not sign-extension) for unsigned indices | Array/pointer index codegen |
| Interprocedural const ptr propagation | Direct GEP into global .rodata, eliminates pointer load | `array_const_*` + const-returning function detection |

---

## Repository Structure

```
axiom/
├── .github/
│   └── workflows/
│       └── ci.yml                  # GitHub Actions CI (test + build on push)
├── CLAUDE.md                       # This file -- project context
├── README.md                       # GitHub README
├── BENCHMARKS.md                   # Performance results
├── DESIGN.md                       # Living design document
├── Cargo.toml                      # Workspace root
├── crates/
│   ├── axiom-lexer/                # Tokenizer (63 tests)
│   ├── axiom-parser/               # Parser -> AST (52 tests)
│   ├── axiom-hir/                  # HIR + lowering (25 tests)
│   ├── axiom-mir/                  # Mid-level IR (stub)
│   ├── axiom-codegen/              # LLVM IR generation (165 tests)
│   ├── axiom-optimize/             # Optimization protocol + agent API (132 tests)
│   └── axiom-driver/               # CLI + MCP server + compilation (96 tests + 12 E2E/doc-tests)
│       └── runtime/
│           └── axiom_rt.c          # C runtime (I/O, coroutines, threads, jobs)
├── spec/                           # Formal language specification
│   ├── grammar.ebnf
│   ├── types.md
│   ├── annotations.md
│   ├── optimization.md
│   └── transfer.md
├── benchmarks/
│   ├── suite/                      # 115 simple benchmarks
│   ├── complex/                    # 30 complex benchmarks
│   ├── real_world/                 # 20 real-world benchmarks
│   ├── memory/                     # 30 memory benchmarks
│   ├── fib/                        # From drujensen/fib (908 stars)
│   └── leibniz/                    # From niklas-heer/speed-comparison
├── examples/                       # 38 example programs (including 21 C project ports)
│   ├── sort/                       # Bubble, insertion, selection sort
│   ├── nbody/                      # N-body gravitational simulation
│   ├── numerical/                  # Pi, roots, integration
│   ├── matmul/                     # Matrix multiplication demos
│   ├── crypto/                     # Caesar cipher
│   ├── ecs/                        # Entity-Component-System game demo
│   ├── raytracer/                  # Full raytracer (scalar + vec3 versions)
│   ├── image_filter/               # Image processing
│   ├── json_parser/                # JSON parser
│   ├── pathfinder/                 # Pathfinding algorithms
│   ├── physics_sim/                # Physics simulation
│   ├── compiler_demo/              # Compiler demo
│   ├── game_loop/                  # Frame allocator, zero per-frame allocs
│   ├── self_opt/                   # LLM optimization demos (primes, matmul)
│   ├── multi_agent/                # Multi-agent handoff demo
│   ├── self_host/                  # AXIOM lexer written in AXIOM
│   ├── siphash/                    # SipHash-2-4 port (400+ stars)
│   ├── qoi/                        # QOI image codec port (7,439 stars)
│   ├── xxhash/                     # xxHash32 port (10,954 stars)
│   ├── aes/                        # AES-128 ECB port (4,902 stars)
│   ├── heatshrink/                 # Heatshrink LZSS port (1,300+ stars)
│   ├── lz4/                        # LZ4 compression port (10,600 stars)
│   ├── cjson/                      # cJSON parser port (11,000 stars)
│   ├── fastlz/                     # FastLZ compression port (500+ stars)
│   ├── lzav/                       # LZAV compression port (400+ stars)
│   ├── turbopfor/                  # TurboPFor integer compression port (800+ stars)
│   ├── miniz/                      # Huffman codec port (miniz/2,300+ stars)
│   ├── base64/                     # Base64 codec (Turbo-Base64 algorithm)
│   ├── blake3/                     # BLAKE3 crypto hash port
│   ├── minimp3/                    # minimp3 IMDCT-36 port
│   ├── stb_jpeg/                   # stb_image JPEG IDCT port
│   ├── smhasher/                   # SMHasher hash functions port
│   ├── lodepng/                    # lodepng PNG decode port (2,200+ stars)
│   ├── fpng/                       # fpng fast PNG encode port (850+ stars)
│   ├── libdeflate/                 # libdeflate fast DEFLATE port (900+ stars)
│   ├── utf8proc/                   # utf8proc UTF-8 processing port (450+ stars)
│   └── roaring/                    # Roaring Bitmaps port (1,500+ stars)
├── lib/                            # AXIOM standard libraries
│   └── ecs.axm                     # ECS library (archetype storage)
├── scripts/                        # Development scripts
│   └── self_optimize.sh            # Self-optimization bootstrap script
├── tests/samples/                  # 24 test programs
├── docs/                           # Research documents
│   ├── MASTER_TASK_LIST.md         # 47-milestone task tracker (ALL COMPLETE)
│   ├── OPTIMIZATION_RESEARCH.md
│   ├── MEMORY_ALLOCATION_RESEARCH.md
│   ├── GAME_ENGINE_RESEARCH.md
│   ├── MULTITHREADING_ANALYSIS.md
│   ├── LUX_INTEGRATION_RESEARCH.md
│   ├── AXIOM_Language_Plan.md
│   └── OPTIMIZATION_KNOWLEDGE.md  # 14 rules + 6 anti-patterns (LLM knowledge base)
└── .pipeline/                      # Multi-agent development pipeline
```

---

## ALL Phases Complete (47/47 milestones)

### Phase A -- Fix UB + Soundness (MT-1) DONE
Removed incorrect `@pure`/`noalias`/`nosync` on shared pointers. Added `fence release`/`fence acquire` around parallel regions. Fixed `@pure` semantics so write-through-ptr functions are not marked `memory(none)`.

### Phase B -- Parallel For + Reductions (MT-2, MT-3) DONE
Implemented `@parallel_for` annotation with OpenMP-style data sharing clauses: `shared_read`, `shared_write`, `reduction(op: var)`, `private`. HIR validation ensures correct placement. Codegen emits correct LLVM IR with thread-local accumulation and final atomic combine for reductions. Identity values per type.

### Phase C -- Constraints + Const Eval + Platform (L1, L3, P1, P4) DONE
Constraint-driven LLM prompts: `@constraint { optimize_for: "performance" }` is extracted from source and threaded into the LLM optimization prompt, changing reasoning strategy. Recursive `@const` evaluation with full function body interpretation and depth limits. `--target` CLI flag for CPU architecture selection. Constraint-to-clang-flag mapping (`optimize_for: "performance"` -> `-O3`, `"memory"` -> `-Os`, `"size"` -> `-Oz`).

### Phase D -- Ownership + Dependencies + LLVM Metadata (MT-4, MT-5, MT-6) DONE
New pointer types: `readonly_ptr[T]` (read-only, enables LLVM `readonly` attribute), `writeonly_ptr[T]` (write-only, enables LLVM `writeonly` attribute). Lexer keywords, parser type expressions, HIR types, and codegen all updated. Job dependency graph: `job_dispatch_handle` returns a handle, `job_dispatch_after` waits for a dependency before executing, `job_wait_handle` blocks until a specific job completes. Atomic counter-based completion (Naughty Dog GDC pattern). `!llvm.access.group` and `!llvm.loop.parallel_accesses` metadata on proven-parallel loops.

### Phase E -- Sum Types + Strings + Dynamic Arrays + Closures (F1, F2, F3, F5) DONE
Option type as builtin functions: `option_none`, `option_some`, `option_is_some`, `option_is_none`, `option_unwrap` (tagged union packed into i64). Result type as builtin functions: `result_ok`, `result_err`, `result_is_ok`, `result_is_err`, `result_unwrap`, `result_err_code`. String builtins: `string_from_literal`, `string_len`, `string_ptr`, `string_eq`, `string_print` (fat pointer: ptr + len). Vec builtins: `vec_new`, `vec_push_i32`, `vec_push_f64`, `vec_get_i32`, `vec_get_f64`, `vec_set_i32`, `vec_set_f64`, `vec_len`, `vec_free`. Function pointer builtins: `fn_ptr`, `call_fn_ptr_i32`, `call_fn_ptr_f64`.

### Phase F -- Hardware Counters + CPUID + SIMD (L2, P2, P3) DONE
Hardware counter integration: `axiom profile` collects timing data and feeds it to the LLM optimizer. `cpu_features()` builtin queries CPUID at runtime and returns a feature bitmask. SIMD width metadata: `@vectorizable` loops emit preferred vector width hints based on target CPU features.

### Phase G -- Generics + Modules + Errors + Pattern Matching (F4, F6, F7, F8) DONE
Generics with monomorphization codegen. Module system with `import` declarations and separate compilation. Result type implemented via builtin functions (tagged union). While-let and if-let patterns with full codegen.

### Phase H -- CI + Debug Info + Formatter (E1, E2, E3) DONE
GitHub Actions CI pipeline (`.github/workflows/ci.yml`): runs `cargo test --workspace` on every push. DWARF debug info: source file and line metadata emitted in LLVM IR for debugger support. `axiom fmt` command: parse -> HIR -> pretty-print. `axiom profile` command: compile, benchmark, extract optimization surfaces, suggest tuning.

### Phase K -- Self-Improvement (S1-S3) DONE
Self-hosted AXIOM parser written in AXIOM (`examples/self_host/`). Compiler self-optimization via PGO bootstrap (`axiom pgo`): profile the compiler, recompile with profile data, iterate. Source-to-source AI optimizer (`axiom rewrite`): LLM rewrites AXIOM source code (not just ?params).

### Phase L -- Verified Development Pipeline (V1-V4) DONE
`@strict` module annotation enforces that all functions carry `@pure`/`@intent`/`@complexity` annotations -- any missing annotation is a compile error. `@precondition(expr)` and `@postcondition(expr)` on functions emit runtime checks in `--debug` builds (no overhead in release). `@test { input: (...), expect: value }` attaches inline test cases to functions, runnable via `axiom test`. `axiom verify` checks annotation completeness across a module. `axiom test --fuzz` auto-generates test inputs from `@precondition` constraints. New builtins: `assert(cond, msg)` for runtime assertions, `debug_print(expr)` for debug-mode-only output.

---

## CLI Commands (16 total)

```bash
# Build
cargo build --release

# Run tests
cargo test --workspace

# Compile AXIOM program
axiom compile program.axm -o output
axiom compile --emit=tokens|ast|hir|llvm-ir program.axm
axiom compile --target=x86-64-v4 program.axm -o output
axiom compile --debug program.axm -o output         # Enable runtime pre/postcondition checks
axiom compile --error-format=json program.axm        # JSON diagnostic output

# Tokenizer debug
axiom lex program.axm

# Optimization
axiom optimize program.axm --iterations 5
axiom optimize program.axm --dry-run
axiom bench program.axm --runs 10
axiom profile program.axm --iterations 10

# Formatting
axiom fmt program.axm

# Documentation generation
axiom doc program.axm

# Profile-guided optimization
axiom pgo program.axm --iterations 3

# Watch mode (recompile on change)
axiom watch program.axm

# Build project with dependency resolution
axiom build

# Source-to-source AI rewriter
axiom rewrite program.axm --strategy performance

# LSP server for editor integration
axiom lsp

# Verified development
axiom verify program.axm                # Check annotation completeness (@strict)
axiom test program.axm                  # Run @test blocks
axiom test program.axm --fuzz           # Auto-fuzz from @precondition

# Time-travel debugging
axiom replay program.trace.jsonl             # Replay execution trace
axiom replay program.trace.jsonl --filter fn # Filter by function name

# MCP server
axiom mcp

# Run benchmarks
python benchmarks/run_all.py --runs 3
```

---

## Development Workflow

### Commit Conventions
```
type(scope): description
```
Types: `feat`, `fix`, `refactor`, `test`, `spec`, `docs`, `bench`, `chore`, `perf`

### Testing Strategy
- **Unit tests**: `#[cfg(test)]` modules in every crate
- **Integration tests**: Parse `.axm` files through the full pipeline
- **E2E tests**: Compile to binary and verify output
- **Benchmark tests**: Compare AXIOM vs C performance

### Multi-Agent Development Pipeline
The project uses a 7-agent pipeline:
1. **Architect** -- designs specifications
2. **Optimistic Design Reviewer** -- reviews spec for completeness
3. **Pessimistic Design Reviewer** -- reviews spec for risks
4. **Coder** -- implements from spec
5. **QA** -- runs tests, verifies criteria
6. **Optimistic Code Reviewer** -- reviews for quality
7. **Pessimistic Code Reviewer** -- adversarial review for bugs and UB

---

## Code Style & Patterns

### Rust Conventions
- `thiserror` for error types, `miette` for diagnostic display
- Every public API has doc comments
- `Spanned<T>` wrapper for AST/IR nodes with source location
- Newtype pattern for IDs: `struct NodeId(u32);`
- Minimal `pub` surface

### Anti-Patterns to Avoid
1. **No type inference.** Every type is explicit.
2. **No implicit returns.** Every function has `return`.
3. **No operator overloading.** `+` always means numeric add.
4. **No code without tests.** Every feature ships with tests.
5. **Annotations are first-class data, not decorations.**
6. **Parser must recover gracefully** -- report ALL errors.
7. **No string types for structured data.**
8. **No `>>` operator.** AI-first design: use explicit `shr()` (arithmetic) or `lshr()` (logical) to eliminate ambiguity between signed and unsigned right shift.
