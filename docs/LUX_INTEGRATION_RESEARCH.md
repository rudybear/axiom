# Lux Shading Language: Deep Research & AXIOM Convergence Plan

## Research Report — 2026-03-23

---

## Table of Contents

1. [What Is Lux?](#1-what-is-lux)
2. [Lux Syntax — Rust-Influenced, Math-First](#2-lux-syntax)
3. [Shader Targets & Compilation Pipeline](#3-shader-targets--compilation-pipeline)
4. [Feature Inventory](#4-feature-inventory)
5. [Compiler Architecture](#5-compiler-architecture)
6. [Standard Library](#6-standard-library)
7. [Coroutines & Async in Game Engines](#7-coroutines--async-in-game-engines)
8. [Self-Improving Compilers & PGO](#8-self-improving-compilers--pgo)
9. [MLIR Self-Optimization](#9-mlir-self-optimization)
10. [AXIOM–Lux Convergence Plan](#10-axiom-lux-convergence-plan)

---

## 1. What Is Lux?

**Lux** is a math-first shader language designed for human readability and LLM compatibility. Repository: `github.com/rudybear/lux` (MIT license).

Core philosophy: *"Write rendering math directly — surfaces, materials, lighting — and the compiler handles GPU translation to SPIR-V for Vulkan and Metal."*

Lux eliminates GPU boilerplate entirely: no `layout` qualifiers, no `gl_Position` magic variables, no manual stage wiring, no descriptor set/binding management. The programmer writes mathematical material descriptions, and the compiler produces production-quality SPIR-V.

### Key Statistics

| Metric | Value |
|--------|-------|
| Test suite | 1,462+ tests |
| Stdlib functions | 160+ across 15 modules |
| Example shaders | 81 files |
| Rendering backends | 5 (Python/wgpu, C++/Vulkan, C++/Metal, Rust/ash, WebGPU/browser) |
| Development phases | 27 completed |
| Language | Python (compiler), targeting SPIR-V output |
| License | MIT |

### Design Principles

1. **Math-first vocabulary** — `scalar` instead of `float`, surfaces/BRDFs as first-class constructs
2. **Auto-layout** — locations, descriptor sets, and bindings assigned by declaration order
3. **Explicit types** — no type inference; every declaration carries its type
4. **Declarative materials** — separation of rendering math from GPU plumbing
5. **Algorithm/schedule separation** — quality tiers selected independently of material definitions
6. **Function inlining everywhere** — no SPIR-V function calls emitted
7. **LLM-friendly** — designed so AI can naturally translate physics equations into code

---

## 2. Lux Syntax

Lux syntax is Rust-influenced but domain-specialized for shaders. Key elements:

### Type Annotations (Rust-style colon syntax)

```lux
in position: vec3;
let roughness: scalar = 0.5;
fn fresnel(cos_theta: scalar, f0: vec3) -> vec3 { ... }
```

### Fundamental Types

| Category | Types |
|----------|-------|
| Scalar | `scalar` (f32), `int`, `uint`, `bool`, `void` |
| Vector | `vec2`, `vec3`, `vec4`, `ivec2-4`, `uvec2-4` |
| Matrix | `mat2`, `mat3`, `mat4`, `mat4x3` |
| Sampler | `sampler2d`, `samplerCube`, `sampler2DArray`, `samplerCubeArray` |
| Special | `storage_image`, `acceleration_structure` |
| Semantic | `type strict WorldPos = vec3` (compile-time safety, zero runtime cost) |

### Variable Declarations

```lux
let x: scalar = 1.0;          // Immutable
var mutable: vec3 = vec3(0.0); // Mutable (in functions only)
const PI: scalar = 3.14159265; // Module-level constant
```

### Control Flow

```lux
if condition { ... } else if other { ... } else { ... }
for i in 0..10 { ... }
while condition { ... }
// Ternary: condition ? true_expr : false_expr
```

### Hello Triangle (complete, zero boilerplate)

```lux
vertex {
    in position: vec3;
    in color: vec3;
    out frag_color: vec3;

    fn main() {
        frag_color = color;
        builtin_position = vec4(position, 1.0);
    }
}

fragment {
    in frag_color: vec3;
    out color: vec4;

    fn main() {
        color = vec4(frag_color, 1.0);
    }
}
```

### Declarative Surface/Pipeline System

```lux
import brdf;

surface CopperMetal {
    brdf: pbr(vec3(0.95, 0.64, 0.54), 0.3, 0.9),
}

geometry StandardMesh {
    position: vec3, normal: vec3, uv: vec2,
    transform: MVP { model: mat4, view: mat4, projection: mat4 },
    outputs {
        world_pos: (model * vec4(position, 1.0)).xyz,
        world_normal: normalize((model * vec4(normal, 0.0)).xyz),
        clip_pos: projection * view * model * vec4(position, 1.0),
    }
}

pipeline PBRForward {
    geometry: StandardMesh,
    surface: CopperMetal,
}
```

The compiler expands `pipeline` declarations into full vertex + fragment SPIR-V stages automatically.

### Layered Materials (Composable)

```lux
import brdf;
import color;
import ibl;

surface GltfPBR {
    sampler2d base_color_tex,
    sampler2d metallic_roughness_tex,
    sampler2d normal_tex,
    sampler2d emissive_tex,

    layers [
        base(albedo: srgb_to_linear(sample(base_color_tex, uv).xyz),
             roughness: sample(mr_tex, uv).y,
             metallic: sample(mr_tex, uv).z),
        normal_map(map: sample(normal_tex, uv).xyz),
        emission(color: srgb_to_linear(sample(emissive_tex, uv).xyz)),
    ]
}
```

### Compile-Time Features (Shader Permutations)

```lux
features {
    has_normal_map: bool,
    has_clearcoat: bool,
}

geometry StandardMesh {
    position: vec3,
    normal: vec3,
    tangent: vec4 if has_normal_map,
}

surface GltfPBR {
    sampler2d normal_tex if has_normal_map,
    layers [
        base(albedo: ..., roughness: ..., metallic: ...),
        normal_map(map: sample(normal_tex, uv).xyz) if has_normal_map,
    ]
}
```

### Schedule (Algorithm/Quality Separation)

```lux
schedule HighQuality {
    fresnel: schlick,
    distribution: ggx,
    geometry_term: smith_ggx,
    tonemap: aces,
}

schedule Mobile {
    distribution: ggx_fast,
    geometry_term: smith_ggx_fast,
    tonemap: reinhard,
}
```

### Annotations

| Annotation | Purpose |
|------------|---------|
| `@binding(N)` | Override automatic binding assignment |
| `@layer` | Define custom surface/lighting layer |
| `@differentiable` | Mark function for automatic differentiation |
| `@[debug]` | Strip entire block in release builds |

### Operators

Full set: arithmetic (`+`, `-`, `*`, `/`, `%`), comparison (`==`, `!=`, `<`, `>`, `<=`, `>=`), logical (`&&`, `||`, `!`), bitwise (`&`, `|`, `^`, `<<`, `>>`), ternary (`?:`), swizzle (`.xyz`, `.rg`).

---

## 3. Shader Targets & Compilation Pipeline

### Output Targets

| Target | Mechanism | Status |
|--------|-----------|--------|
| **SPIR-V** (Vulkan) | Primary native output | Full support |
| **Metal/MSL** | SPIR-V → SPIRV-Cross transpilation at runtime | Full support |
| **WGSL** (WebGPU) | SPIR-V → WGSL transpilation | Rasterization only |
| **GLSL** | `--transpile` flag (GLSL input conversion) | Available |

### SPIR-V Versions

- **1.0** — Rasterization stages (vertex, fragment)
- **1.4** — Ray tracing and mesh shader stages (with KHR extensions)

### Compilation Pipeline (17 Phases)

```
Source (.lux)
    │
    ├─ 1.  Lark Parser → AST dataclasses
    ├─ 2.  Feature stripping (compile-time conditionals)
    ├─ 3.  Import resolver (stdlib + local modules)
    ├─ 4.  Surface/geometry/pipeline expansion to stage blocks
    ├─ 5.  Deferred rendering expander (G-buffer + lighting)
    ├─ 6.  Splat expander (Gaussian splatting: compute + vertex + fragment)
    ├─ 7.  Autodiff expander (gradient functions from @differentiable)
    ├─ 8.  Type checker
    ├─ 9.  Debug stripper (release mode)
    ├─ 10. NaN checker (optional static analysis)
    ├─ 11. Constant folding & strength reduction
    ├─ 12. Function inlining (AST-level, release mode)
    ├─ 13. Dead code elimination
    ├─ 14. CSE (common subexpression elimination)
    ├─ 15. Auto-type analyzer (fp16 range analysis)
    ├─ 16. Layout assignment (auto-assign location/set/binding)
    ├─ 17. SPIR-V builder → spirv-as → spirv-val
    │
    ├─ [Optional] spirv-opt optimization
    ├─ [Optional] SPIRV-Cross → MSL/WGSL transpilation
    │
    └─ Output: .vert.spv, .frag.spv, .comp.spv, .rgen.spv, etc.
```

### Alternative Path: CPU Debugger

`--debug-run` activates a CPU interpreter that tree-walks the AST with 44+ math builtins, providing a gdb-style REPL for shader debugging without any GPU hardware.

### Compiler CLI Examples

```bash
# Basic compilation
python -m luxc examples/hello_triangle.lux

# With optimization
python -m luxc examples/pbr_surface.lux -O

# CPU debugger (no GPU required)
python -m luxc examples/debug_playground.lux --debug-run --stage fragment

# Auto-type precision report
python -m luxc examples/pbr_surface.lux --auto-type=report

# AI-generated shader
python -m luxc --ai "frosted glass with subsurface scattering" -o generated.lux

# Compile all feature permutations
python -m luxc examples/gltf_pbr.lux --all-permutations

# Hot reload with file watching
python -m luxc examples/hello_triangle.lux --watch
```

---

## 4. Feature Inventory

### Rendering Modes

| Mode | Declaration | Generated Stages |
|------|-------------|-----------------|
| Rasterization | `mode: rasterize` (default) | vertex + fragment |
| Ray Tracing | `mode: raytrace` | raygen + closest_hit + miss + (any_hit, intersection, callable) |
| Mesh Shaders | `mode: mesh_shader` | task + mesh + fragment |
| Deferred | `mode: deferred` | G-buffer pass + lighting pass (auto-generated) |
| Gaussian Splatting | `mode: gaussian_splat` | compute + vertex + fragment |
| Compute | standalone `compute { }` | compute |

### Shader Stage Support

| Stage | SPIR-V Model | Purpose |
|-------|-------------|---------|
| `vertex` | Shader | Vertex transformation |
| `fragment` | Shader | Pixel/fragment shading |
| `compute` | GLCompute | General-purpose GPU compute |
| `raygen` | RayGenerationKHR | Ray tracing entry point |
| `closest_hit` | ClosestHitKHR | Ray-surface intersection handler |
| `any_hit` | AnyHitKHR | Transparency/alpha testing |
| `miss` | MissKHR | Ray miss (environment) |
| `intersection` | IntersectionKHR | Custom intersection testing |
| `callable` | CallableKHR | Indirect function dispatch |
| `mesh` | MeshEXT | Mesh shader stage |
| `task` | TaskEXT | Task/amplification shader |

### Material System

- **Simple BRDF surfaces** — single `brdf:` expression
- **Layered surfaces** — composable `layers [...]` with built-in and custom layers
- **OpenPBR Surface v1.1** — 9 composable layers (base, specular, coat, fuzz, thin_film, transmission, emission, subsurface, normal_map)
- **Properties blocks** — abstract material parameters with defaults, generating UBOs with reflection JSON
- **Custom `@layer` functions** — user-defined layers receiving (base, normal, view, light) automatically

### Built-in Layers

| Layer | Block | Parameters |
|-------|-------|-----------|
| `base` | surface | albedo, roughness, metallic |
| `normal_map` | surface | map |
| `emission` | surface | color |
| `coat` | surface | factor, roughness |
| `sheen` | surface | color, roughness |
| `transmission` | surface | factor, ior, thickness |
| `directional` | lighting | direction, color |
| `ibl` | lighting | specular_map, irradiance_map, brdf_lut |
| `multi_light` | lighting | (reads LightData + ShadowEntry SSBOs) |

### Gaussian Splatting

First-class support with a single `splat` declaration generating a complete 3-stage pipeline:

```lux
splat GaussianCloud {
    sh_degree: 0,           // 0, 1, 2, or 3 (spherical harmonics)
    kernel: ellipse,
    color_space: srgb,
    sort: camera_distance,
    alpha_cutoff: 0.004,
}
```

Generates: compute (projection, covariance, SH evaluation, sort) + vertex (instanced quad) + fragment (2D Gaussian evaluation). KHR_gaussian_splatting conformance with 226 tests across 11 official Khronos assets.

### Automatic Differentiation

```lux
@differentiable
fn energy(x: scalar) -> scalar { return x * x; }
// Compiler generates: energy_d_x(x) -> scalar
```

Forward-mode autodiff supporting chain rule, product rule, quotient rule, and derivatives of all built-in math functions.

### Debug Tooling

- **CPU shader debugger** — gdb-style REPL with breakpoints, stepping, variable inspection
- **`debug_print`** — Printf-style debug output (stripped in release)
- **`assert`** — Compile-time assertions (stripped in release or `--assert-kill` for fragment discard)
- **`@[debug]` blocks** — Zero-cost debug instrumentation
- **Static NaN analysis** — `--warn-nan` flags unsafe operations (division, sqrt, normalize, pow, log)
- **RenderDoc integration** — `--rich-debug` emits source-level debug info
- **Debug visualization** — `debug_normal()`, `debug_heatmap()` helpers

### AI Integration

- **AI material authoring** — 5 providers (Anthropic, OpenAI, Gemini, Ollama, LM-Studio)
- **Natural language shader generation** — `--ai "frosted glass with subsurface scattering"`
- **AI critique** — automated shader review and suggestion

### Semantic Types (Coordinate-Space Safety)

```lux
type strict WorldPos = vec3;
type strict ViewPos = vec3;
type strict ClipPos = vec4;
// Compile error: cannot assign WorldPos to ViewPos
```

Zero-runtime-cost compile-time type safety preventing coordinate-space mixing.

---

## 5. Compiler Architecture

### Source Organization

```
luxc/
├── __init__.py
├── __main__.py
├── cli.py              # Command-line interface
├── compiler.py         # Main compilation orchestrator
├── hot_reload.py       # File watching & live reload
├── reload_protocol.py  # Reload communication protocol
├── watcher.py          # File system watcher
│
├── grammar/            # Lark grammar definitions
├── parser/             # Lexer + parser → AST
├── analysis/           # Type checking, semantic analysis
├── autotype/           # fp16/fp32 precision classification
├── autodiff/           # Automatic differentiation expansion
├── builtins/           # Built-in function definitions
├── codegen/            # SPIR-V code generation
├── debug/              # CPU debugger, debug instrumentation
├── expansion/          # Surface/pipeline/deferred/splat expansion
├── features/           # Compile-time feature system
├── optimization/       # Constant folding, DCE, CSE, inlining
├── stdlib/             # Standard library modules (15 modules)
├── transpiler/         # GLSL→Lux, WGSL output
└── ai/                 # AI shader generation & critique
```

### Optimization Details

**AST-Level Optimizations:**

| Optimization | Effect |
|-------------|--------|
| Constant folding | `1.0 + 2.0` → `3.0`, `pow(x, 2.0)` → `x * x` |
| Strength reduction | `pow(x, 0.5)` → `sqrt(x)`, `x * 1.0` → `x` |
| Dead code elimination | Removes unreferenced variables/assignments (up to 20 iterations) |
| CSE | Structural hashing to deduplicate expression trees |
| Function inlining | All user functions inlined at call site before CSE |

**SPIR-V-Level Optimizations:**

| Optimization | Effect |
|-------------|--------|
| Mem2Reg (SSA) | ~125 OpVariable → ~20 (only loop vars, accumulators) |
| Constant vector hoisting | Runtime vec construction → compile-time constants |
| spirv-opt standard (`-O`) | General code size reduction |
| spirv-opt performance (`--perf`) | Loop unrolling, if-conversion, scalar replacement |

**Result:** "21.7% fewer instructions than hand-written GLSL." Typical PBR shaders show 60-70% of variables classified as fp16-safe, enabling 2x throughput on mobile.

### Memory Layout

- **UBO** (Uniform Buffer Object) — std140 layout, auto-generated from `properties` blocks
- **SSBO** (Shader Storage Buffer Object) — for light data, material arrays, splat data
- **Push Constants** — std140 layout, detected via SPIR-V analysis

---

## 6. Standard Library

| Module | Functions | Purpose |
|--------|-----------|---------|
| `brdf` | 30+ | Fresnel, GGX, Smith, clearcoat, sheen, transmission |
| `ibl` | 8 | Image-based lighting |
| `sdf` | 18 | Signed distance field primitives and CSG |
| `noise` | 13 | Perlin, value noise, gradient noise, FBM, Voronoi |
| `color` | 5 | Color space conversion (sRGB ↔ linear) |
| `colorspace` | 8 | HSV, contrast, saturation adjustments |
| `texture` | 11 | TBN matrix, triplanar projection, parallax mapping |
| `lighting` | 7 | Multi-light evaluation, attenuation |
| `shadow` | 4 | Shadow map sampling, PCF, cascade selection |
| `toon` | 1 | Cel-shading @layer function |
| `compositing` | 2 | IBL composition |
| `pbr_pipeline` | 1 | Single-call PBR (`pbr_shade()`) |
| `gaussian` | 6 | Gaussian splatting utilities |
| `openpbr` | 18 | OpenPBR Surface v1.1 |
| `debug` | 5 | Debug visualization helpers |

Total: **160+ functions** across 15 modules.

---

## 7. Coroutines & Async in Game Engines

### 7.1 Unity Coroutines

Unity coroutines use C#'s `IEnumerator` pattern — cooperative multitasking on the main thread (NOT multi-threaded). A coroutine suspends at `yield return` and resumes on the next frame (or after a condition).

**How it works internally:**
- Coroutines are `IEnumerator` methods with `yield return` statements
- The Unity runtime stores the enumerator and calls `MoveNext()` each frame
- `yield return null` → resume next frame
- `yield return new WaitForSeconds(t)` → resume after t scaled seconds
- `yield return new WaitUntil(predicate)` → resume when condition is true

**Limitations:**
- No return values
- No try/catch support
- Cannot run without the engine
- Allocates GC pressure (each `new WaitForSeconds` allocates unless cached)
- Single-threaded — synchronous operations still block the main thread

**Best practice:** Cache `WaitForSeconds` instances to avoid per-frame allocation.

### 7.2 Unreal Engine Async

Unreal uses C++20 coroutines (since UE5) with two modes:

- **Latent** — hooks into Unreal's built-in latent system; Unreal manages lifecycle
- **Asynchronous** — standard C++20 `co_await`; function suspends and resumes when awaited operation completes

Unreal also provides:
- `FRunnable` for background threads
- `AsyncTask` for thread pool work
- `FLatentActionInfo` for Blueprint-exposed async operations

### 7.3 Async/Await vs Coroutines in Games

| Feature | Unity Coroutines | async/await |
|---------|-----------------|-------------|
| Return values | No | Yes |
| Synchronous execution | No | Yes |
| Engine-independent | No | Yes |
| try/catch | No | Yes |
| Cancellation | StopCoroutine() | CancellationToken |
| Thread control | Main thread only | Can use any thread |

**Microsoft Game Development Kit pattern:** Tasks + Task Queues — tasks represent async work, queues determine which thread processes them.

**Godot:** Uses `await` keyword in GDScript — pauses function, returns control to engine, resumes when awaiting completes.

### 7.4 C++ Stackless vs Stackful Coroutines

| Property | Stackless (C++20) | Stackful (Fibers) |
|----------|-------------------|-------------------|
| Memory per suspension | 16-256 bytes + locals | 4KB-2MB (full stack) |
| Suspension points | Only at coroutine entry | Any point in call stack |
| Compiler support | Required (state machine transform) | Runtime library |
| Deep suspension | Cannot yield from nested calls | Can yield from anywhere |
| Performance | Lower overhead | Higher overhead |
| Use case | Lightweight async, generators | Complex algorithms needing deep suspension |

**C++20 coroutines are stackless.** The compiler transforms the coroutine into a state machine, storing local variables in heap-allocated coroutine frames. This is memory-efficient but restricts suspension to direct coroutine entry points.

**Game engine recommendation:** Use stackless for lightweight async (I/O, timers, animations). Use stackful (fibers) for complex AI behavior trees or job systems requiring deep suspension.

### 7.5 Zig Async — Removed and Redesigned

**History:**
- Zig had experimental async/await tied to language-level stackless coroutines
- Removed in Zig 0.11 because "it never felt finished, it never felt like it was good enough"
- Complete redesign underway — decoupling async I/O from stackless coroutines

**New design (Zig 0.16+):**
- async/await are NOT language keywords — they become Standard Library constructs via the Io Interface
- Decoupled from execution model: code works with blocking, thread pool, or stackless implementations
- `suspend`/`resume` machinery may remain as low-level primitives (Proposal #23446)
- The Standard Library itself is being rewritten to use the new Io interface

**Lesson for AXIOM:** Language-level async is controversial. Zig's experience suggests keeping async as a library/runtime concern rather than baking it into syntax. The execution model should be pluggable.

### 7.6 Rust/Bevy Async Patterns

**Bevy's architecture:**
- Built on the `smol` async runtime (NOT Tokio)
- Three task pools:
  - `AsyncComputeTaskPool` — CPU-bound background computation
  - `IoTaskPool` — I/O-bound operations (networking, file access)
  - `ComputeTaskPool` — main ECS parallel iteration
- Tasks return handles; systems poll for completion
- Common pattern: one system spawns tasks, another system handles results

**Tokio integration challenges:**
- Bevy uses `smol`, not Tokio — mixing them causes panics
- Community solutions: `bevy-tokio-tasks`, `bevy-async-runner`
- `bevy_mod_async` provides ergonomic async without needing another runtime

**Key insight:** Bevy does NOT use Rust's async/await for gameplay logic. Systems are synchronous functions. Async is reserved for background tasks (asset loading, networking, heavy computation). The ECS schedule handles parallelism.

---

## 8. Self-Improving Compilers & PGO

### 8.1 Compiler Bootstrapping

A self-compiling compiler compiles its own source code. The improvement loop:

1. Build compiler v1 (with existing tools)
2. Compile compiler source with v1 → produces compiler v2
3. v2 incorporates any backend improvements → better code generation
4. Compile again with v2 → v3 with further improvements
5. Converges when v(N) = v(N+1)

Historical: NELIAC (1958), Algol (1961), LISP (1962). Modern: GCC, Clang, Rust, Go all self-host.

### 8.2 Profile-Guided Optimization (PGO)

PGO uses profiling data from real workloads to optimize the final binary.

**Three-phase process:**
1. **Instrument** — Build with profiling instrumentation
2. **Profile** — Run instrumented binary on representative workloads
3. **Optimize** — Rebuild using collected profile data

**Applied to compiler bootstrap (LLVM/Clang):**
1. Build standard Release Clang + `libclang_rt.profile`
2. Build Clang again with instrumentation enabled
3. Run instrumented Clang on representative compilation tasks → collect profiles
4. Convert raw profiles to final PGO profile
5. Build final PGO-optimized Clang

**Performance gains:**
- Clang/LLVM with PGO: **~20% faster compilation**
- Go with PGO: **2-14% runtime improvement** (as of Go 1.22)
- Rust supports PGO via `-Cprofile-generate` and `-Cprofile-use`
- GraalVM Native Image supports PGO for ahead-of-time compiled Java

### 8.3 Google's Scalable Self-Improvement (Iterative BC-Max)

Google's Iterative BC-Max (NeurIPS 2024, ML For Systems workshop):

- Targets **binary size reduction** through improved inlining decisions
- Uses **supervised learning** instead of RL — more stable and computationally efficient
- Self-improvement loop: compile corpus → learn best per-program decisions → compile again
- Applied to ~30,000 programs
- Creates a **decision-making policy** that improves over iterations

### 8.4 LLM-Based Compiler Optimization

| Approach | Model | Result |
|----------|-------|--------|
| Meta LLM Compiler | Foundation model | Trained on compiler optimization tasks |
| Large Language Models for Compiler Optimization | 7B transformer | 3.0% instruction count reduction over compiler baseline |
| Reasoning Compiler (LLM + MCTS) | LLM-guided search | 2x speedup with 16x fewer samples than TVM evolutionary search |

**Key insight for AXIOM:** Self-improvement doesn't require the compiler to be self-hosting. An LLM can serve as the "improvement agent," analyzing compiler output and proposing optimization pass sequences or code transformations. This aligns directly with AXIOM's `@strategy` and `?param` optimization holes.

---

## 9. MLIR Self-Optimization

### 9.1 MLIR Pass Infrastructure

MLIR (Multi-Level Intermediate Representation) provides:
- **Dialects** — custom operations, types, attributes for specific domains
- **Passes** — transformation and optimization algorithms
- **Pass Manager** — orchestrates pass execution, supports parallel pipeline execution on function-level passes
- **Progressive lowering** — high-level dialects → mid-level → LLVM IR

### 9.2 Built-in Optimization Passes

| Pass | Description |
|------|-------------|
| Constant propagation | Detect constant values, propagate, replace, eliminate |
| CSE | Common subexpression elimination using Memory SideEffect interface |
| Canonicalization | Pattern-based simplification per dialect |
| Inlining | Function inlining with cost model |
| LICM | Loop-invariant code motion |
| Dead code elimination | Remove unused operations |
| Symbol DCE | Remove unused symbols |
| Buffer deallocation | Automatic memory management |

### 9.3 Self-Optimization Research

**DESIL (2025):** Introduces operation-aware optimization recommendation — selects optimization passes based on which MLIR operations appear in a given program. Produces different optimized variants via different pass sequences for differential testing and bug detection.

**Implications:** MLIR's pass infrastructure is modular enough that an AI agent could learn which pass sequences are optimal for specific IR patterns. This is the foundation for AXIOM's self-optimization vision:

```
AXIOM Source → AXIOM HIR → MLIR (AXIOM dialect)
                                    ↓
                          AI selects pass sequence
                                    ↓
                          MLIR optimized → LLVM IR → native
```

---

## 10. AXIOM-Lux Convergence Plan

### 10.1 The Complementary Domains

AXIOM and Lux occupy **complementary, non-overlapping domains**:

| Dimension | AXIOM | Lux |
|-----------|-------|-----|
| **Target hardware** | CPU (x86, ARM, RISC-V) | GPU (Vulkan, Metal, WebGPU) |
| **Execution model** | Sequential + SIMD + threads | Massively parallel shader stages |
| **Compilation target** | MLIR → LLVM IR → native | AST → SPIR-V |
| **Optimization model** | AI-driven `?param` exploration | Built-in compiler passes |
| **Type system** | Tensor-first with shape constraints | Vector/matrix-first with semantic types |
| **Memory model** | Explicit ownership + arenas | GPU buffers (UBO, SSBO, push constants) |
| **Language** | Compiler in Rust | Compiler in Python |
| **Maturity** | Design phase | 27 phases completed, 1462+ tests |

### 10.2 Shared Syntax Foundation

Both languages already use Rust-influenced syntax. The overlap is substantial:

| Feature | AXIOM Syntax | Lux Syntax | Compatible? |
|---------|-------------|------------|-------------|
| Type annotations | `x: f32` | `x: scalar` | Yes (alias `scalar` = `f32`) |
| Functions | `fn name(p: T) -> R { }` | `fn name(p: T) -> R { }` | Identical |
| Constants | `const X: f32 = 1.0` | `const X: scalar = 1.0` | Yes |
| Let bindings | `let x: T = v` | `let x: T = v` | Identical |
| Control flow | `if/else`, `for`, `while` | `if/else`, `for`, `while` | Identical |
| Module imports | `import module` | `import module;` | Identical |
| Annotations | `@pure`, `@constraint` | `@layer`, `@differentiable` | Same mechanism |
| Structs | `struct Name { field: T }` | `struct Name { field: T }` | Identical |

**Conclusion:** A shared parser frontend is achievable. The languages diverge at the semantic/backend level, not at syntax.

### 10.3 Unified Language Architecture

```
                    ┌─────────────────────────┐
                    │   Unified Source (.axm)  │
                    │   Rust-style syntax      │
                    │   Shared type system     │
                    └────────────┬────────────┘
                                 │
                    ┌────────────┴────────────┐
                    │     Shared Frontend      │
                    │  Parser → AST → Types    │
                    │  @annotations, features  │
                    └────────────┬────────────┘
                                 │
                 ┌───────────────┼───────────────┐
                 │               │               │
        ┌────────┴───────┐ ┌────┴─────┐ ┌───────┴────────┐
        │   CPU Backend  │ │  GPU     │ │  Compute       │
        │   (AXIOM)      │ │  Backend │ │  Backend       │
        │                │ │  (Lux)   │ │  (shared)      │
        │  MLIR → LLVM   │ │  SPIR-V  │ │  SPIR-V compute│
        │  → native      │ │  Vulkan  │ │  or LLVM       │
        └────────────────┘ │  Metal   │ └────────────────┘
                           │  WebGPU  │
                           └──────────┘
```

### 10.4 Integration Strategy: Five Phases

#### Phase I: Syntax Unification (Month 1-2)

**Goal:** Establish a shared grammar that both backends accept.

1. **Type aliasing** — `scalar` = `f32`, `vec3` = `vector[f32, 3]`, `mat4` = `matrix[f32, 4, 4]`
2. **Shared annotation system** — Merge AXIOM's `@strategy`, `@constraint`, `@intent` with Lux's `@layer`, `@differentiable`, `@binding`
3. **Unified `features` blocks** — Lux's compile-time features become AXIOM's conditional compilation
4. **Shared `const`/`let`/`var`/`fn` declarations** — Already identical

**Deliverable:** A single `.axm` file that contains both CPU functions and GPU shader stages, parseable by one frontend.

#### Phase II: Cross-Domain Type System (Month 2-3)

**Goal:** Types flow seamlessly between CPU and GPU code.

```axiom
// Shared type definitions
type strict WorldPos = vec3;        // From Lux — works on both CPU and GPU
tensor[f32, N, 3] @layout(row_major) // From AXIOM — can be bound as SSBO

// Cross-domain function
@pure @target(cpu, gpu)
fn fresnel_schlick(cos_theta: f32, f0: vec3) -> vec3 {
    return f0 + (vec3(1.0) - f0) * pow(1.0 - cos_theta, 5.0);
}
// CPU: compiles via MLIR → LLVM (for offline precomputation)
// GPU: compiles via SPIR-V (for real-time rendering)
```

Key decisions:
- Lux's `scalar` maps to AXIOM's `f32`
- Lux's `vec2/3/4` map to AXIOM's `vector[f32, 2/3/4]`
- AXIOM's `tensor` types can generate GPU SSBOs when bound to shader stages
- Semantic types (`type strict`) work across both domains

#### Phase III: Shared Optimization Protocol (Month 3-5)

**Goal:** AXIOM's `?param` optimization holes work for GPU shaders too.

```axiom
// GPU schedule with optimization holes
schedule Adaptive {
    fresnel: ?fresnel_model,           // AI explores: schlick, schlick_fast, exact
    distribution: ?ndf_model,          // AI explores: ggx, ggx_fast, beckmann
    tonemap: ?tonemap_model,           // AI explores: aces, reinhard, uncharted2
    precision: ?float_precision,       // AI explores: fp32, fp16, mixed
}

@strategy {
    quality: ?quality_level,           // AI tunes per-platform
    workgroup_size: ?wg_size,          // AI tunes for compute shaders
}

@optimization_log {
    v1: { fresnel_model: schlick, ndf_model: ggx, precision: fp32 }
        -> gpu_time: 2.3ms, visual_quality: 0.97
    v2: { fresnel_model: schlick_fast, ndf_model: ggx_fast, precision: mixed }
        -> gpu_time: 0.8ms, visual_quality: 0.94
}
```

This directly extends Lux's existing `schedule` system with AXIOM's exploration protocol.

#### Phase IV: CPU-GPU Data Pipeline (Month 5-7)

**Goal:** Unified memory model for host-device data transfer.

```axiom
// CPU-side: prepare data
@target(cpu)
fn prepare_lights(scene: Scene) -> buffer[LightData] @usage(storage) {
    let lights = scene.collect_lights();
    return lights.to_gpu_buffer();
}

// GPU-side: consume data
@target(gpu)
lighting SceneLighting {
    // Automatically bound from CPU buffer
    storage LightBuffer { lights: array[LightData] },

    layers [
        multi_light(data: LightBuffer.lights),
    ]
}

// Pipeline declaration connecting CPU and GPU
pipeline ForwardRenderer {
    cpu_setup: prepare_lights,        // Runs on CPU
    geometry: StandardMesh,           // GPU vertex stage
    surface: GltfPBR,                 // GPU fragment stage
    lighting: SceneLighting,          // GPU lighting
}
```

#### Phase V: Self-Improving Shader Compilation (Month 7-12)

**Goal:** AI agents optimize both CPU and GPU code in a unified loop.

```axiom
@module pbr_renderer
@intent "Real-time PBR rendering pipeline"
@constraint {
    gpu_frame_time < 16.6ms,          // 60 FPS target
    cpu_frame_time < 8ms,             // CPU budget
    memory < 256MB,                    // Total GPU memory
    visual_quality > 0.95,            // Perceptual metric
}

@transfer {
    source_agent: "claude-opus-4.6"
    context: "Optimized GPU shaders for mobile. CPU-side light culling
              reduces draw calls by 40%. Consider tiled deferred for
              scenes with >100 lights."
    open_questions: [
        "Is forward+ better than deferred for this scene complexity?",
        "Can we move BVH traversal to compute shader?",
    ]
}
```

The AI agent sees both CPU and GPU code, understands the full pipeline, and can make cross-domain optimization decisions:
- Move computation from GPU to CPU (or vice versa)
- Adjust quality schedules based on measured frame times
- Rebalance CPU-GPU workload based on profiling
- Explore rendering mode changes (forward vs deferred vs ray traced)

### 10.5 Technical Integration Points

#### Compiler Architecture Merger

| Component | Current State | Unified Plan |
|-----------|--------------|--------------|
| Parser | Lux: Lark (Python), AXIOM: Rust | Shared Rust parser (chumsky/nom) |
| AST | Lux: Python dataclasses, AXIOM: Rust structs | Shared Rust AST |
| Type checker | Lux: Python, AXIOM: Rust | Shared Rust type checker |
| CPU backend | AXIOM only (MLIR → LLVM) | Keep as-is |
| GPU backend | Lux only (SPIR-V) | Port to Rust, use `rspirv` crate |
| Optimization | Lux: AST-level; AXIOM: MLIR passes | Both, with shared constant folding/DCE |
| AI integration | Lux: 5 providers; AXIOM: agent API | Unified MCP-based agent interface |

#### Rewriting Lux's Compiler in Rust

Lux is currently a Python compiler. For convergence, the GPU backend should be rewritten in Rust:

1. **Parser** — Replace Lark with `chumsky` or `lalrpop` (same grammar, Rust implementation)
2. **AST** — Rust enums/structs replacing Python dataclasses
3. **Type checker** — Rust implementation with trait-based type resolution
4. **SPIR-V generation** — Use `rspirv` crate (Rust SPIR-V builder, already mature)
5. **Optimization** — Rust reimplementation of constant folding, DCE, CSE, inlining
6. **spirv-opt** — Continue using as external tool, or integrate `spirv-tools-rs`

This is feasible because Lux's compiler is well-structured with clean phase separation.

#### Shared Reflection/Metadata System

Both AXIOM and Lux need to emit structured metadata:
- Lux: reflection JSON (binding layouts, material properties, performance hints)
- AXIOM: optimization history, strategy parameters, benchmarks

A unified reflection format could serve both:

```json
{
    "module": "pbr_renderer",
    "targets": {
        "cpu": {
            "functions": ["prepare_lights", "cull_objects"],
            "optimization_log": [...]
        },
        "gpu": {
            "stages": ["vertex", "fragment"],
            "bindings": [...],
            "performance": {
                "instruction_count": 847,
                "texture_samples": 5,
                "vgpr_pressure": "medium"
            },
            "optimization_log": [...]
        }
    }
}
```

### 10.6 What Lux Teaches AXIOM

| Lux Innovation | AXIOM Lesson |
|----------------|-------------|
| Declarative `surface`/`pipeline` | Domain-specific declarative blocks scale well; AXIOM should support `@domain` blocks for compute/ML/networking |
| Schedule separation | Algorithm selection should be independent of algorithm definition — matches `@strategy` perfectly |
| Compile-time `features` | Conditional compilation via `features { }` blocks is cleaner than `#ifdef` — adopt directly |
| Auto-layout | Boilerplate elimination through convention-over-configuration — apply to AXIOM's memory layout |
| Semantic types | `type strict` for compile-time safety with zero cost — adopt for AXIOM's tensor dimensions |
| `@differentiable` | Forward-mode autodiff is essential for ML workloads — extend to AXIOM's CPU code |
| CPU shader debugger | AST-walking interpreter for debugging — AXIOM should have a similar `--debug-run` mode |
| 160+ function stdlib | A rich standard library accelerates adoption — prioritize AXIOM's stdlib early |

### 10.7 What AXIOM Brings to Lux

| AXIOM Innovation | Lux Benefit |
|------------------|------------|
| `?param` optimization holes | GPU shader tuning becomes systematic, not ad-hoc |
| `@optimization_log` | Shader optimization history is tracked and reusable |
| `@transfer` protocol | Multiple AI agents can collaborate on shader optimization |
| `@constraint` | Frame time budgets become compiler-enforced |
| MLIR integration | Future: MLIR SPIR-V dialect could replace custom SPIR-V builder |
| Tensor types | GPU compute kernels get first-class tensor support |
| `@intent` annotations | Semantic documentation that AI agents can reason about |
| Self-hosting goal | Lux compiler rewritten in the unified language — dogfooding |

### 10.8 Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|-----------|
| Lux Python → Rust rewrite | High (engineering effort) | Incremental: start with shared parser, port modules over time |
| SPIR-V vs MLIR divergence | Medium | MLIR has a SPIR-V dialect; investigate using it |
| Feature scope creep | High | Start with compute shaders (overlap domain), expand to graphics |
| GPU async model differs from CPU | Medium | Keep execution models separate; unify only the source syntax |
| Lux's "no loops" limitation | Low | Lux recently added for/while; convergence pressure will keep expanding |

### 10.9 Recommended Starting Point

**Start with compute shaders.** They sit at the intersection of AXIOM (computation) and Lux (GPU). A unified language that can express:

```axiom
@target(cpu)
fn matmul_cpu(a: tensor[f32, M, K], b: tensor[f32, K, N]) -> tensor[f32, M, N] {
    @strategy { tiling: { M: ?tm, N: ?tn, K: ?tk } }
    // ... CPU implementation via MLIR → LLVM
}

@target(gpu)
compute matmul_gpu {
    storage_buffer a: array[f32];    // Bound from CPU tensor
    storage_buffer b: array[f32];
    storage_buffer result: array[f32];

    @strategy { workgroup_size: ?wg }

    fn main() {
        let gid: uvec3 = global_invocation_id;
        // ... GPU implementation via SPIR-V
    }
}

// The AI agent can benchmark both and choose
@strategy { execution_target: ?target }  // cpu or gpu
```

This proves the unified syntax, cross-domain types, shared optimization protocol, and CPU-GPU dispatch — all in one focused prototype.

---

## Appendix A: Lux Rendering Backends

| Backend | Language | API | Capabilities |
|---------|----------|-----|-------------|
| Python/wgpu | Python | WebGPU | Headless rasterization, Gaussian splatting, deferred |
| C++/Vulkan | C++ | Vulkan/GLFW | Full: raster, RT, mesh shaders, deferred, interactive |
| C++/Metal | C++ | Metal/GLFW | Raster, mesh shaders (Metal 3), macOS only |
| Rust/ash | Rust | Vulkan/winit | Full feature parity with C++ Vulkan |
| WebGPU/browser | TypeScript | WebGPU/Vite | Browser-based, rasterization only |

All backends consume SPIR-V + reflection JSON, ensuring consistent cross-platform results.

## Appendix B: Lux Example Inventory (81 files)

**PBR & Materials:** `pbr_basic.lux`, `pbr_surface.lux`, `gltf_pbr.lux`, `advanced_materials_demo.lux`, `brdf_gallery.lux`
**Ray Tracing:** `rt_manual.lux`, `rt_pathtracer.lux`, `gltf_pbr_rt.lux`
**Gaussian Splatting:** Multiple variants with SH degrees 0-3
**OpenPBR:** Glass, car paint, velvet, aluminum, pearl
**Compute:** `compute_mandelbrot.lux`, `compute_gradient.lux`, `compute_histogram.lux`, `compute_saxpy.lux`, `compute_reduction.lux`, `compute_image.lux`, `compute_double.lux`
**Deferred:** `deferred_basic.lux`
**Mesh Shaders:** `mesh_shader_manual.lux`
**Autodiff:** `differentiable.lux`
**Specialized:** `procedural_noise.lux`, `texture_demo.lux`, `sdf_shapes.lux`

## Appendix C: Key Current Limitations of Lux

From the specification:
- No user-defined structs in shader stages (planned)
- No arrays (manual unrolling required)
- No recursion (except hardware-handled `trace_ray`)
- No string types or preprocessor directives
- Numeric arithmetic primarily on floats (integers promoted)
- WebGPU backend limited to rasterization (no RT/mesh shaders)

## Appendix D: Coroutine/Async Summary for AXIOM Design

| Engine/Language | Approach | Execution Model | Recommendation for AXIOM |
|----------------|----------|-----------------|------------------------|
| Unity | `IEnumerator` coroutines | Main thread, cooperative | Too limited — don't copy |
| Unreal | C++20 coroutines + latent actions | Stackless, engine-managed | Good model for game-specific async |
| Zig | Removed async; redesigning as library | Io Interface (pluggable) | **Best model** — keep async out of syntax |
| Bevy/Rust | `smol` task pools + ECS parallelism | Cooperative + work-stealing | Good for CPU compute tasks |
| C++20 | Stackless coroutines | Compiler-generated state machine | Low-level primitive; too complex for language surface |

**AXIOM recommendation:** Follow Zig's approach. Keep `async/await` out of the language syntax. Provide:
1. Stackless coroutines as a low-level primitive (for generators, state machines)
2. Task pools as a library construct (for background work)
3. The ECS/schedule system handles parallelism (for game engine integration)
4. `@target(gpu)` blocks express GPU parallelism (via Lux backend)

---

*Document generated 2026-03-23. This research supports the convergence of AXIOM (CPU computation, AI-optimizable) and Lux (GPU shading, math-first) into a unified language for full-stack game engine development.*
