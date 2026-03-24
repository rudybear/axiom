# Phase 7 — Game Engine: The Grand Plan

## Vision

Build a **fully playable demo** in AXIOM: 10K+ particles with physics, parallel job system, Vulkan rendering via Lux shaders, zero per-frame allocations, self-improving optimization. This proves AXIOM's thesis end-to-end.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                  AXIOM Game Runtime                  │
│                                                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐  │
│  │ Job      │  │ Frame    │  │ Coroutine        │  │
│  │ System   │  │ Allocator│  │ Scheduler        │  │
│  │ (@job)   │  │ (@arena) │  │ (yield/resume)   │  │
│  └────┬─────┘  └────┬─────┘  └────────┬─────────┘  │
│       │              │                 │             │
│  ┌────▼──────────────▼─────────────────▼──────────┐ │
│  │              ECS World                          │ │
│  │  Components: @layout(soa) arrays               │ │
│  │  Systems: @pure @job functions                  │ │
│  │  Entities: index into component arrays          │ │
│  └────────────────────┬───────────────────────────┘ │
│                       │                             │
│  ┌────────────────────▼───────────────────────────┐ │
│  │           Vulkan Renderer                       │ │
│  │  Command buffers via extern fn (C ABI)          │ │
│  │  Shaders compiled by Lux → SPIR-V              │ │
│  │  CPU↔GPU sync: fences + timeline semaphores    │ │
│  └─────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
```

## Lux Integration Strategy

AXIOM (CPU) and Lux (GPU shaders) share Rust-like syntax. Instead of building a new shader language, **use Lux as the shader backend**:

```
AXIOM source (.axm)                    Lux source (.lux)
       │                                      │
       ▼                                      ▼
  AXIOM Compiler                         Lux Compiler
       │                                      │
       ▼                                      ▼
  LLVM IR → native binary              SPIR-V → GPU pipeline
       │                                      │
       └──────────── linked at runtime ───────┘
                     via Vulkan API
```

**Convergence path:**
1. AXIOM programs `@import` Lux shaders as external resources
2. Shared type system: AXIOM's `f32`/`f64`/`array` maps to Lux's `scalar`/`vec3`/`mat4`
3. Shared optimization protocol: AXIOM's `?param` holes work with Lux's `schedule` system
4. Future: unified parser frontend that emits either LLVM IR (CPU) or SPIR-V (GPU)

## Milestone Breakdown

### M7.1 — Struct Codegen + C ABI (Foundation)
**Why first:** Everything else depends on proper struct support and C ABI.

Features:
- Struct definition → LLVM `%struct.Name = type { i32, f64, ... }`
- Struct field access → `getelementptr`
- `@repr(C)` for C-compatible layout
- Struct as function parameter (by pointer)
- Struct return values

Tests: struct creation, field access, nested structs, C-compatible layout

### M7.2 — I/O Primitives + File System
**Why:** Games need to load assets, write configs, read input.

Features:
- `file_read_bytes(path) -> ptr` — read file into heap buffer
- `file_write_bytes(path, data, len)` — write buffer to file
- `file_size(path) -> i64`
- `stdin_read_line() -> ptr` — basic input
- Command-line argument access

### M7.3 — Coroutines / Async
**Why:** Game logic loves coroutines (Unity's entire scripting model).

Design (library approach, not language syntax — like Zig):
```axiom
// Coroutine state stored in arena-allocated frame
let coro: ptr[i32] = coro_create(arena, my_generator);
// Each tick:
let value: i32 = coro_resume(coro);
let done: i32 = coro_is_done(coro);
```

Implementation: stackful coroutines via `setjmp`/`longjmp` or platform fibers. The coroutine frame is arena-allocated (zero malloc per coroutine switch).

### M7.4 — Multithreading + Job System
**Why:** Games need parallel physics, AI, animation, particle updates.

Features:
```axiom
// Thread primitives
let t: ptr[i32] = thread_spawn(my_function, arg);
thread_join(t);

// Atomic operations
let old: i32 = atomic_load(ptr);
atomic_store(ptr, val);
let prev: i32 = atomic_cas(ptr, expected, desired);

// Job system (built on @pure guarantee)
@job @pure
fn update_physics(bodies: ptr[f64], start: i32, end: i32, dt: f64) {
    // Compiler PROVES this is safe to parallelize
    // because @pure = no shared mutable state
}

// Dispatch 10000 bodies across all cores
job_dispatch(update_physics, bodies, 10000, dt);
```

The `@pure` annotation is the key: it proves non-interference without lock analysis.

### M7.5 — ECS Framework
**Why:** The dominant architecture pattern for game performance.

Features:
```axiom
@module ecs;

// Components are just arrays
@layout(soa) @align(64)
let positions: array[f64, 30000];   // x,y,z packed SOA
let velocities: array[f64, 30000];
let alive: array[i32, 10000];

// Systems are @pure @job functions
@pure @job @vectorizable(i)
fn integrate_positions(
    pos: ptr[f64], vel: ptr[f64], n: i32, dt: f64
) {
    for i: i32 in range(0, n) {
        ptr_write_f64(pos, i, ptr_read_f64(pos, i) + ptr_read_f64(vel, i) * dt);
    }
}
```

### M7.6 — Vulkan FFI + Lux Shader Integration
**Why:** Actual rendering.

Features:
```axiom
// Vulkan via extern fn declarations
extern fn vkCreateInstance(info: ptr[i32], alloc: ptr[i32], instance: ptr[i32]) -> i32;
extern fn vkCreateDevice(...) -> i32;

// Or use a Rust Vulkan wrapper via C ABI
extern fn renderer_init(width: i32, height: i32) -> ptr[i32];
extern fn renderer_begin_frame(r: ptr[i32]) -> i32;
extern fn renderer_draw(r: ptr[i32], vertices: ptr[f64], count: i32);
extern fn renderer_end_frame(r: ptr[i32]);

// Lux shaders loaded as SPIR-V blobs
extern fn load_shader(path: ptr[i32]) -> ptr[i32];
extern fn create_pipeline(device: ptr[i32], vert: ptr[i32], frag: ptr[i32]) -> ptr[i32];
```

Lux compiles shaders → SPIR-V. AXIOM loads SPIR-V via Vulkan. Both share the same data types.

### M7.7 — Frame Allocator + Zero-Allocation Game Loop
**Why:** The core performance requirement.

Features:
```axiom
@module game_loop;
@constraint { per_frame_allocations: 0 };

fn game_loop(renderer: ptr[i32]) -> i32 {
    // Ring buffer of 3 frame arenas (triple buffering)
    let frame_arenas: array[ptr[i32], 3] = array_zeros[ptr[i32], 3];
    // ... init arenas ...

    let frame: i32 = 0;
    while not should_quit() {
        let current_arena: ptr[i32] = frame_arenas[frame % 3];
        arena_reset(current_arena);  // Free ALL last frame's allocations

        // All per-frame allocations come from this arena
        let commands: ptr[i32] = arena_alloc(current_arena, 10000, 4);

        // Physics (parallel, pure, zero-alloc)
        job_dispatch(update_physics, ...);

        // Render (Vulkan commands)
        renderer_begin_frame(renderer);
        renderer_draw(renderer, ...);
        renderer_end_frame(renderer);

        frame = frame + 1;
    }
    return 0;
}
```

### M7.8 — Self-Improving Optimization
**Why:** The AXIOM thesis — AI agents iteratively improve the game's performance.

Features:
```bash
# Profile the game, find hot functions
axiom profile game.axm --frames 1000

# AI agent optimizes hot paths
axiom optimize game.axm \
    --target x86_64-avx2 \
    --constraint "frame_time < 16.6ms" \
    --agent claude \
    --iterations 10

# Agent fills ?params in @strategy blocks:
# - Tile sizes for physics batching
# - Prefetch distances for memory access patterns
# - Unroll factors for inner loops
# - Parallel chunk sizes for job dispatch
# - SOA vs AOS layout choices
```

The compiler can also PGO-bootstrap itself:
1. Compile with instrumentation
2. Run the game, collect profiles
3. Recompile with profile data
4. Repeat until convergence

### M7.9 — Killer Demo: Particle Galaxy
**The proof:**

10,000 particles:
- Gravity simulation (O(n log n) via spatial hashing)
- Collision detection (SIMD-optimized)
- Per-particle color/size/lifetime
- Vulkan instanced rendering via Lux shaders
- Zero heap allocations per frame
- All physics + collision on parallel jobs
- AI-optimized tile sizes and prefetch distances
- 60fps target, measured per-frame times

This demo, running at 60fps with zero allocations, proves every claim in the README.

## Agent Pipeline Plan

Each milestone runs through the 5-agent pipeline:

| Milestone | Architect Focus | Coder Complexity | Key Risk |
|-----------|----------------|-----------------|----------|
| M7.1 Structs | LLVM struct type layout | Medium | ABI alignment rules |
| M7.2 I/O | System call interface | Low | Platform differences |
| M7.3 Coroutines | Fiber/continuation design | High | Stack management |
| M7.4 Jobs | Work-stealing scheduler | High | Thread safety proof |
| M7.5 ECS | SOA transform, archetype storage | Medium | Cache line analysis |
| M7.6 Vulkan | FFI binding generation | Medium | Vulkan complexity |
| M7.7 Frame alloc | Ring buffer arena | Low | Sync with GPU frames |
| M7.8 Self-opt | PGO + AI optimization loop | Medium | Feedback loop design |
| M7.9 Demo | Integration | High | Everything must work together |

## Success Criteria

The demo must demonstrate:
- [ ] 10K+ particles rendered at 60fps
- [ ] Zero per-frame heap allocations (arena only)
- [ ] Physics running on parallel jobs (@pure @job)
- [ ] Lux shaders for rendering (SPIR-V)
- [ ] Self-optimized via axiom optimize
- [ ] Total frame time < 16.6ms consistently
- [ ] Measured and compared against equivalent C++/Vulkan implementation
