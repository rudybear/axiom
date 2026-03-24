# AXIOM Game Engine Research: The Vision for a Game-Optimized Language

## A Comprehensive Technical Research Document

**Date:** 2026-03-23
**Purpose:** Define how AXIOM can become the ultimate language for building perfectly optimized games -- zero per-frame allocations, parallel job systems, efficient CPU/GPU synchronization, and Vulkan rendering. This is the "killer app" for AXIOM.

---

# Table of Contents

1. [Game Engine Architecture -- What Makes Games Fast](#1-game-engine-architecture----what-makes-games-fast)
2. [Job Systems / Parallel Task Execution](#2-job-systems--parallel-task-execution)
3. [Vulkan / GPU Programming from Custom Languages](#3-vulkan--gpu-programming-from-custom-languages)
4. [ABI Conformance / Rust Interop](#4-abi-conformance--rust-interop)
5. [What a "Game-Optimized Language" Would Need](#5-what-a-game-optimized-language-would-need)
6. [Specific Optimization Patterns for Games](#6-specific-optimization-patterns-for-games)
7. [I/O, Threading, Stdlib for Games](#7-io-threading-stdlib-for-games)
8. [AXIOM Game Engine Feature Roadmap](#8-axiom-game-engine-feature-roadmap)

---

# 1. Game Engine Architecture -- What Makes Games Fast

## 1.1 Data-Oriented Design (DOD)

### What It Is and Why It Matters

Data-Oriented Design is the fundamental architectural philosophy behind every high-performance game engine built in the last decade. Instead of organizing code around objects with methods (OOP), DOD organizes code around **data transformations** -- how data flows through the CPU cache hierarchy.

The core insight: **modern CPUs are not bottlenecked by computation. They are bottlenecked by memory access.** A cache miss costs 100-300 CPU cycles. An L1 cache hit costs 4 cycles. The entire goal of DOD is to ensure that when the CPU needs data, it is already in the cache.

### How Current Engines Handle It

**Naughty Dog** (The Last of Us, Uncharted): Pioneered DOD in AAA games. All engine code is structured as jobs operating on contiguous data arrays. Their GDC 2015 talk by Christian Gyrling ("Parallelizing the Naughty Dog Engine Using Fibers") showed how they restructured their entire PS4 engine around fiber-based job processing of flat data arrays.

**Unity DOTS** (Data-Oriented Technology Stack): A complete rewrite of Unity's runtime around ECS principles, delivering 50-100x performance improvements for CPU-bound workloads. Uses the Burst Compiler (LLVM-based) to translate C# IL to highly optimized native code with automatic SIMD vectorization. The `Unity.Mathematics` library maps `float4`, `float3`, etc. directly to hardware SIMD registers.

**Bevy Engine** (Rust): An open-source ECS game engine with ergonomic Rust syntax. Systems are regular Rust functions, and the scheduler automatically determines which systems can run in parallel by analyzing their data access patterns (read vs. write on which component types). Uses archetype-based storage where entities with identical component sets are stored in contiguous tables for cache-efficient iteration.

**Unreal Engine 5**: Uses a Task Graph System for parallel execution, distributing animation and physics calculations across worker threads. However, its classic Game Thread + Render Thread architecture "struggles scaling past 4 cores" due to synchronization points.

### How AXIOM Could Do It Better

AXIOM's annotation system is uniquely positioned to express DOD constraints:

```axiom
@layout(soa)                    // Compiler enforces Structure-of-Arrays layout
@cache_line(64)                 // Data aligned to cache line boundaries
@access_pattern(sequential)     // AI knows this is traversed linearly
struct ParticleData {
    position_x: array[f32, MAX_PARTICLES] @align(64),
    position_y: array[f32, MAX_PARTICLES] @align(64),
    position_z: array[f32, MAX_PARTICLES] @align(64),
    velocity_x: array[f32, MAX_PARTICLES] @align(64),
    velocity_y: array[f32, MAX_PARTICLES] @align(64),
    velocity_z: array[f32, MAX_PARTICLES] @align(64),
    lifetime:   array[f32, MAX_PARTICLES] @align(64),
}
```

**Concrete Feature Proposals:**

1. **`@layout(soa)` annotation**: Like Jai's `SOA` keyword and Odin's `#soa` directive, but integrated with AXIOM's optimization protocol. The AI agent can switch between `@layout(soa)` and `@layout(aos)` and benchmark both -- the optimization hole `?layout_mode` lets the AI systematically explore which layout is faster for a given access pattern.

2. **`@access_pattern` annotation**: Declares how data is accessed (sequential, random, strided). This feeds into the AI optimizer's prefetch strategy decisions.

3. **`@hot` / `@cold` field annotations**: Mark which struct fields are accessed in the hot loop vs. rarely. The compiler can split the struct into hot and cold parts automatically.

**Implementation Difficulty:** Medium. SOA transformation is a well-understood compiler transform. The annotation infrastructure already exists in AXIOM. The main work is in the HIR-to-LLVM lowering for layout transforms.

## 1.2 Zero-Allocation Game Loops

### What It Is and Why It Matters

A game must render frames at 60 FPS (16.67ms per frame) or 120 FPS (8.33ms per frame). **Any heap allocation during a frame is a potential frame stutter.** `malloc` can take microseconds to milliseconds depending on heap fragmentation. Worse, it can trigger page faults or the OS memory manager, causing multi-millisecond stalls.

The rule in professional game development: **all memory used during gameplay is pre-allocated at load time. Zero `malloc` calls between the first and last frame.**

### How Current Engines Handle It

**Frame/Arena Allocators**: The most common pattern. A large block of memory (e.g., 16MB) is allocated once at startup. Each frame, objects are allocated by simply bumping a pointer forward. At the end of the frame, the pointer resets to zero. All per-frame allocations are O(1) and have zero fragmentation.

Performance numbers: Arena allocation is **10.44x faster** than `malloc` for individual allocations. Deallocation is **358,000x faster** because it is a single integer assignment (reset the pointer).

**Ring Buffer Allocators**: For data that spans 2-3 frames (e.g., GPU command buffers in flight), a ring buffer allocator wraps around. You allocate forward, and when you wrap, you ensure the old allocations are no longer in use.

**Double-Ended Stack Allocator**: Resident allocations grow from the bottom; temporary allocations grow from the top. This splits long-lived and short-lived memory without fragmentation.

**Pool Allocators**: For fixed-size objects (entities, components, particles), a pool pre-allocates N slots. Allocation is O(1) -- pop from a free list. Deallocation is O(1) -- push to the free list.

**AAA Memory Budgets**: AAA games allocate a large block of memory at startup, managed by custom heap schedulers. On consoles, memory budgets are strictly enforced (e.g., 5GB of 8GB for gameplay, 2GB for streaming, 1GB reserved). On mobile, games typically use 1/4 to 1/2 of physical RAM.

### How AXIOM Could Do It Better

AXIOM's existing `@arena` annotation concept (mentioned but not yet implemented) is the foundation:

```axiom
@module game_loop;
@constraint { per_frame_allocations: 0 }  // Hard constraint: zero malloc per frame

// Frame allocator -- resets every frame
@allocator(frame)
@capacity(16MB)
@lifetime(frame)
let frame_arena: Arena = arena.create(16 * 1024 * 1024);

// Persistent allocator -- lives for the entire level
@allocator(persistent)
@capacity(256MB)
@lifetime(level)
let level_arena: Arena = arena.create(256 * 1024 * 1024);

// Pool allocator for fixed-size entities
@allocator(pool)
@element_size(sizeof(Entity))
@capacity(10000)
let entity_pool: Pool[Entity] = pool.create(10000);

@pure
@constraint { allocations: 0 }  // This function must not allocate
fn update_physics(
    dt: f32,
    positions: slice[Vec3] @arena(frame),
    velocities: slice[Vec3] @arena(frame),
) -> void {
    for i in range(positions.len) {
        positions[i] = positions[i] + velocities[i] * dt;
    }
}
```

**Concrete Feature Proposals:**

1. **`@constraint { per_frame_allocations: 0 }`**: A module-level constraint that the compiler **statically verifies**. Any function called within the frame loop that could allocate (malloc, box, dynamic array grow) produces a compile error. This is the killer feature -- no other language can guarantee zero allocations at compile time.

2. **First-class allocator types**: `Arena`, `Pool[T]`, `RingBuffer`, `DoubleStack` as built-in types with known semantics. The compiler understands their allocation patterns and can verify lifetime safety.

3. **`@lifetime(frame)` / `@lifetime(level)` / `@lifetime(permanent)`**: Explicit lifetime scopes that are coarser than Rust's borrow checker but perfectly suited for game memory patterns. The compiler ensures frame-lifetime data is not stored in persistent structures.

4. **`@arena(name)` on parameters**: Declares which arena a parameter's memory comes from. The compiler traces allocation provenance.

**Implementation Difficulty:** Hard. Static allocation analysis requires whole-program analysis. But AXIOM's explicit annotations make it feasible -- the programmer declares intent, and the compiler verifies it, rather than inferring everything.

## 1.3 Entity Component System (ECS)

### What It Is and Why It Matters

ECS separates identity (Entity = just an ID), data (Components = plain data, no behavior), and logic (Systems = functions that iterate over components). This is the cornerstone of data-oriented game architecture.

Why it is faster than OOP:
- **Cache coherency**: All instances of a component type are stored contiguously. When a system iterates over all `Position` components, they are packed in memory, resulting in sequential cache-line reads.
- **Parallelism**: Systems that access disjoint component sets can run in parallel with zero synchronization.
- **Composition over inheritance**: No vtable overhead, no pointer chasing through inheritance hierarchies.

### How Current Engines Handle It

**Archetype-based storage** (Bevy, Unity DOTS, flecs): Entities with the same component composition are grouped into "archetypes." Each archetype stores its components in columnar tables. When iterating, the system walks each matching archetype's tables sequentially.

**Sparse set storage** (EnTT): Uses sparse sets for O(1) component lookup and O(1) iteration. Better for components that are added/removed frequently.

**Hybrid** (Bevy): Uses archetype-based table storage as default, with sparse set storage as an option for frequently-toggled components.

### SOA vs AOS Performance

**Array of Structures (AOS)**: `[{x,y,z,vx,vy,vz}, {x,y,z,vx,vy,vz}, ...]`
- Pro: All data for one entity is adjacent.
- Con: When iterating only over positions, velocity data pollutes cache lines.

**Structure of Arrays (SOA)**: `{xs: [...], ys: [...], zs: [...], vxs: [...], ...}`
- Pro: When iterating over positions, only position data is in cache. For 10,000 entities, SOA pulls 1,250 cache lines vs 3,750 for AOS when accessing a subset of fields.
- Con: Accessing all fields of one entity requires multiple non-adjacent reads.

The performance difference depends on access patterns. For systems that touch few fields of many entities (the common case in games), SOA wins decisively.

### How AXIOM Could Do It Better

```axiom
@module ecs;

// SOA layout is a first-class annotation
@layout(soa)
@archetype  // Compiler knows this is an ECS archetype
struct Transform {
    position: Vec3,
    rotation: Quat,
    scale: Vec3,
}

@layout(soa)
@archetype
struct Physics {
    velocity: Vec3,
    acceleration: Vec3,
    mass: f32,
}

// System with automatic parallelization
@system
@parallel(entity_index)
@vectorizable(entity_index)
@reads(Transform, Physics)
@writes(Transform)
fn physics_system(
    dt: f32,
    transforms: slice[Transform] @layout(soa),
    physics: slice[Physics] @layout(soa),
) -> void {
    @strategy {
        simd_width: ?simd_width      // AI tunes: 4 (SSE), 8 (AVX2), 16 (AVX-512)
        prefetch:   ?prefetch_dist
        unroll:     ?unroll_factor
    }
    for i in range(transforms.len) {
        transforms[i].position = transforms[i].position
            + physics[i].velocity * dt;
    }
}
```

**Concrete Feature Proposals:**

1. **`@archetype` annotation**: Marks a struct as an ECS archetype. The compiler generates archetype storage tables, component iteration functions, and entity queries.

2. **`@system` annotation**: Marks a function as an ECS system. The compiler analyzes `@reads` and `@writes` to determine data dependencies and automatically schedule parallel execution.

3. **`@reads(C1, C2)` / `@writes(C1, C2)`**: Explicit declaration of which components a system accesses. This is what Bevy infers from Rust's type system -- AXIOM makes it explicit and machine-verifiable.

4. **Built-in `?simd_width` optimization hole**: The AI optimizer can explore different SIMD widths for component iteration, benchmarking each.

**Implementation Difficulty:** High. A full ECS framework is substantial. But AXIOM doesn't need to build the runtime -- it needs to generate the right code patterns that a thin runtime can execute.

## 1.4 Lessons from Casey Muratori and Handmade Hero

### Key Principles

Casey Muratori's Handmade Hero project and his 2023 Performance-Aware Programming course demonstrated several principles critical for AXIOM:

1. **"Clean code" OOP patterns destroy performance**: Muratori showed 20-25x speed differences between "clean code" polymorphism patterns and direct, data-oriented equivalents. Virtual function dispatch, type switches, and deep object hierarchies are performance disasters for hot loops.

2. **Compression-oriented programming**: Start with the simplest possible code, then compress repeated patterns. Don't pre-plan architecture. This is compatible with AXIOM's iterative optimization approach -- start naive, let the AI compress.

3. **No libraries, no frameworks, just code**: Handmade Hero builds everything from scratch (graphics, audio, input, memory) to maintain full control. AXIOM should enable this level of control while providing higher-level abstractions when desired.

4. **Hot code reloading**: Handmade Hero demonstrated live code editing within the first 30 episodes -- modifying C code and seeing changes reflected immediately during gameplay. This was achieved through DLL swapping.

### How AXIOM Could Apply These Lessons

```axiom
// No virtual dispatch -- direct function pointers with known types
@hot_reload    // This module supports hot reloading via DLL swap
@module game_logic;

// Instead of inheritance hierarchies:
@variant
type Entity = Player(PlayerData) | Enemy(EnemyData) | Projectile(ProjectileData);

// Process all entities without vtable overhead
@inline(always)
@vectorizable(entity_index)
fn update_entities(entities: slice[Entity], dt: f32) -> void {
    // Compiler generates a flat switch, not pointer chasing
    for i in range(entities.len) {
        match entities[i] {
            Player(p) => update_player(p, dt),
            Enemy(e) => update_enemy(e, dt),
            Projectile(proj) => update_projectile(proj, dt),
        }
    }
}
```

**Implementation Difficulty:** Medium. Sum types with `@variant` are planned but not yet implemented. Hot reloading requires DLL/shared library output and a stable ABI for game state.

---

# 2. Job Systems / Parallel Task Execution

## 2.1 Naughty Dog's Fiber-Based Job System

### What It Is and Why It Matters

The most influential job system in game development, presented at GDC 2015 by Christian Gyrling. Naughty Dog moved from a single-threaded PS3 engine to a fiber-based job system on PS4 to achieve 60 FPS for The Last of Us Remastered.

**Key technical details:**

- **Fibers, not threads**: Fibers are cooperative (not preemptive) execution contexts. Switching between fibers is a register save/restore -- no kernel context switch. This is 10-100x cheaper than thread switching.
- **All engine code is jobs**: Every piece of engine work (physics, animation, rendering, AI, audio) is expressed as a job. Jobs can spawn sub-jobs and wait on dependencies.
- **Job queues per worker thread**: N-1 worker threads for N cores, plus the main thread. Each thread has its own queue.
- **Fiber yield on dependency**: When a job needs to wait for another job's result, the fiber yields. Another job runs immediately on the same thread. The CPU never idles.

### The Molecular Matters Job System 2.0

Stefan Reinalter's blog series documented a complete lock-free work-stealing job system:

- **Work stealing**: Each worker thread has its own deque (double-ended queue). The owning thread pushes and pops from the front (LIFO). Other threads steal from the back (FIFO). This maximizes cache locality -- recently pushed jobs are likely still in L1 cache.
- **Lock-free queue**: Push() and Pop() only modify `bottom`. Steal() only modifies `top`. This property minimizes contention and enables a lock-free implementation using atomic compare-and-swap.
- **Thread-local allocation**: Each worker thread has its own linear allocator for job data. Zero contention on allocation.
- **Dependencies**: Jobs can declare dependencies on other jobs. The system tracks completion counts atomically.

### enkiTS Task Scheduler

A popular C/C++ task scheduler that uses work stealing with lock-free single-writer multi-reader pipes:

- Writer operates on front, readers steal from back
- Zero allocations during scheduling (pre-allocated task pools)
- Recently added tasks have data likely still in L1 cache
- Tasks can issue sub-tasks from within

## 2.2 Unity Job System + Burst Compiler

Unity's approach combines a job system with an optimizing compiler:

- **Job System**: Structures implement `IJob` or `IJobParallelFor`. The system schedules jobs with dependency handles.
- **Burst Compiler**: Translates C# IL to LLVM IR, then to highly optimized native code. Automatically vectorizes using SIMD. Maps `Unity.Mathematics` types (float4, int4) directly to SIMD registers.
- **Performance**: 10x-100x faster than standard C# for compute-intensive workloads. Auto-vectorization produces SSE/AVX/NEON code without explicit intrinsics.
- **Safety**: Static analysis ensures jobs don't access shared mutable state. Race conditions are compile errors.

## 2.3 Bevy's Parallel System Scheduler

Bevy's ECS automatically parallelizes systems:

- Systems are regular Rust functions with typed parameters (queries, resources).
- The scheduler analyzes parameter types to determine data access (read/write on which component types).
- Systems with non-conflicting data access run in parallel automatically.
- Explicit ordering constraints can be added with `.before()` / `.after()`.
- The borrow checker provides static guarantees that parallel access is safe.

## 2.4 How AXIOM Could Do It Better

AXIOM's annotations can express job system semantics directly:

```axiom
@module job_system;

// Job declaration with explicit data dependencies
@job
@reads(PhysicsWorld)
@writes(Transform)
@priority(high)
fn simulate_physics(
    world: ptr[PhysicsWorld] @read_only,
    transforms: slice[Transform] @write,
    dt: f32,
) -> void {
    @strategy {
        batch_size:    ?batch_size      // How many entities per job
        worker_count:  ?worker_count    // How many threads
        steal_policy:  ?steal_policy    // LIFO_local, FIFO_steal
    }

    @parallel(i)
    for i in range(transforms.len) {
        transforms[i] = integrate(world, transforms[i], dt);
    }
}

// Fork-join pattern with dependency tracking
@job_graph
fn frame_update(dt: f32) -> void {
    // Phase 1: Independent jobs run in parallel
    let physics_done = spawn(simulate_physics, world, transforms, dt);
    let animation_done = spawn(update_animation, skeletons, dt);
    let ai_done = spawn(update_ai, agents, dt);

    // Phase 2: Wait for dependencies, then render
    @depends(physics_done, animation_done)
    let render_data = spawn(prepare_render, transforms, meshes);

    // Phase 3: Submit GPU commands
    @depends(render_data)
    spawn(submit_gpu_commands, render_data);
}
```

**Concrete Feature Proposals:**

1. **`@job` annotation**: Marks a function as a schedulable job. The compiler generates the job wrapper, data dependency metadata, and scheduling hints.

2. **`@reads` / `@writes` on job functions**: Explicit data access declarations. The job scheduler uses these to determine safe parallel execution order. Unlike Bevy's inference from Rust types, AXIOM makes it explicit -- better for AI optimization.

3. **`@parallel(dim)` with `?batch_size`**: The AI optimizer can tune the granularity of parallelism. Too-small batches waste scheduling overhead. Too-large batches leave cores idle.

4. **`@job_graph` annotation**: Declares a function that orchestrates multiple jobs with fork-join semantics. The compiler generates the dependency graph and optimal scheduling order.

5. **`@depends(job1, job2)` annotation**: Explicit dependency declaration between jobs. The scheduler inserts the minimum necessary synchronization.

6. **`?steal_policy` optimization hole**: The AI can explore different work-stealing strategies (LIFO local + FIFO steal, priority-based, affinity-based) and benchmark each.

**Implementation Approach:**

The job system runtime would be a thin layer written in AXIOM itself (or initially in Rust with C ABI), consisting of:
- Worker thread pool (N-1 threads for N cores)
- Per-thread lock-free work-stealing deque
- Per-thread linear allocator for job data
- Atomic dependency counters
- Fiber support (optional, for yield-on-wait)

AXIOM's compiler generates the job wrappers and dependency metadata. The runtime schedules them.

**Implementation Difficulty:** High. A production-quality job system is complex. But the annotation-driven approach means AXIOM only needs to generate the right code patterns -- the runtime can start as a Rust library called via FFI.

## 2.5 SIMD in Game Engines

### Where SIMD Is Used

SIMD (Single Instruction Multiple Data) processes 4, 8, or 16 values simultaneously:

- **Physics**: Contact solver (Box2D v3 uses graph coloring + SIMD, achieving 2.1x speedup with AVX2), broad/narrow phase collision, constraint solving
- **Animation**: Bone matrix multiplication, skinning transforms, blend calculations
- **Particles**: Position/velocity/lifetime updates (cuts processing time in half with 4-wide SIMD)
- **Frustum culling**: Testing thousands of bounding spheres against frustum planes (SIMD cuts cost by 75%, 3x speedup over scalar)
- **Math operations**: Matrix multiply, dot product, cross product, normalize

### SIMD Design in Languages

**Unity Burst**: Maps `float4` directly to SIMD registers. Auto-vectorizes many patterns. Also exposes raw intrinsics (`Unity.Burst.Intrinsics`) for SSE through AVX2 and ARM NEON.

**Odin**: Built-in vector and matrix types with SIMD support. `#simd[4]f32` creates a 4-wide SIMD vector type.

**Jai**: Designed for cache-friendly data layouts; SOA keyword enables SIMD-friendly memory patterns.

**Rust (glam)**: `Vec3A`, `Vec4`, `Quat`, `Mat4` use 128-bit SIMD on x86/wasm. The `packed_simd` / `std::simd` nightly crate provides portable SIMD.

### How AXIOM Could Do It Better

```axiom
// SIMD-native math types
@simd(4)
type Vec4 = struct { x: f32, y: f32, z: f32, w: f32 };

@simd(4)
type Mat4 = struct { cols: array[Vec4, 4] };

// Explicit SIMD width as optimization hole
@vectorizable(i)
@strategy { simd_width: ?simd_width }  // AI picks: 4 (SSE), 8 (AVX2), 16 (AVX-512)
fn transform_points(
    points: slice[Vec4] @align(64) @layout(soa),
    matrix: Mat4 @align(64),
    out: slice[Vec4] @align(64) @layout(soa),
) -> void {
    for i in range(points.len) {
        out[i] = mat4_mul_vec4(matrix, points[i]);
    }
}

// Explicit SIMD intrinsics when auto-vectorization is not enough
@target { cpu.avx2 }
@inline(always)
fn dot_product_8wide(
    a: simd[f32, 8],
    b: simd[f32, 8],
) -> simd[f32, 8] {
    return simd.mul(a, b) |> simd.hadd() |> simd.hadd() |> simd.hadd();
}
```

**Concrete Feature Proposals:**

1. **`@simd(width)` annotation**: Declares a type should map to SIMD registers. Width is the number of elements.

2. **`simd[T, N]` built-in type**: Portable SIMD vector type. `simd[f32, 4]` maps to `__m128` on x86, `float32x4_t` on ARM.

3. **`simd.*` intrinsic functions**: `simd.mul`, `simd.add`, `simd.hadd`, `simd.shuffle`, `simd.gather`, `simd.scatter` -- portable intrinsics that lower to platform-specific instructions.

4. **`?simd_width` optimization hole**: The AI explores which SIMD width gives best performance on the target hardware, benchmarking automatically.

5. **`@target { cpu.avx2 }` for multi-versioning**: Generate multiple versions of a function for different SIMD ISAs. Runtime dispatch to the fastest version.

**Implementation Difficulty:** Medium. LLVM already handles SIMD well. The main work is mapping AXIOM's `simd[T,N]` type to LLVM vector types and providing portable intrinsics.

---

# 3. Vulkan / GPU Programming from Custom Languages

## 3.1 Vulkan Architecture Overview

Vulkan is the modern low-level graphics API that gives applications explicit control over GPU resources. Unlike OpenGL, Vulkan:

- Requires explicit memory management (the application allocates GPU memory)
- Requires explicit synchronization (fences, semaphores, barriers)
- Uses pre-compiled shader modules (SPIR-V)
- Records command buffers on the CPU, submits them to GPU queues
- Supports multi-threaded command buffer recording

This explicit control is both Vulkan's power and its complexity. It is a perfect fit for a language like AXIOM that values explicitness and machine-verifiable correctness.

## 3.2 Calling Vulkan from a Custom Language

### The FFI Path

Vulkan's API is defined as C function pointers loaded from `libvulkan.so` / `vulkan-1.dll`. Any language that can:
1. Load a shared library at runtime
2. Call C-convention functions
3. Pass C-layout structs

...can use Vulkan. This is how every non-C language does it:

**Rust (ash crate)**: Thin unsafe wrappers around raw Vulkan function pointers. 14.6M+ downloads. Provides type safety through Rust's type system while remaining zero-cost.

**Rust (vulkano)**: Safe wrappers with hand-written validation. Prevents invalid API usage at compile time. Higher overhead but catches errors early.

**Zig (vulkan-zig)**: Auto-generated bindings from `vk.xml`. Integrates Vulkan errors with Zig's error system. Renames fields to Zig style. Turns out-parameters into return values.

**Odin**: Official Vulkan bindings in the standard library. Uses Odin's built-in foreign function support.

### What AXIOM Needs

AXIOM needs two things for Vulkan:
1. **C FFI** -- ability to call C functions and pass C-layout structs (see Section 4)
2. **Vulkan binding generator** -- auto-generate AXIOM bindings from `vk.xml`

## 3.3 Command Buffer Generation

### Per-Frame CPU Work

Every frame, the CPU must:
1. Wait for the previous frame's GPU work to complete (fence)
2. Acquire the next swapchain image (semaphore)
3. Reset and re-record command buffers
4. Submit command buffers to the graphics queue
5. Present the rendered image

Command buffer recording is **CPU-intensive**. Each draw call requires:
- Binding the pipeline (shader + render state)
- Binding descriptor sets (textures, uniform buffers)
- Binding vertex/index buffers
- Setting viewport/scissor
- Recording the draw command

**Multi-threaded recording**: Vulkan allows recording secondary command buffers on multiple threads simultaneously. Each thread needs its own `VkCommandPool`. The primary command buffer executes the secondary ones.

Best practices:
- One command pool per thread per frame-in-flight
- Target 5-10 `vkQueueSubmit` calls per frame
- Target less than 100 command buffers per frame
- Each submit should represent >= 0.5ms of GPU work

### How AXIOM Could Handle It

```axiom
@module vulkan_renderer;

// Command buffer recording as a typed, structured operation
@gpu_command_buffer
@constraint { submit_count_per_frame: "< 10" }
fn record_frame(
    cmd: ptr[VkCommandBuffer],
    pass: ptr[VkRenderPass],
    framebuffer: ptr[VkFramebuffer],
    scene: ptr[SceneData] @read_only,
) -> void {
    gpu.begin_render_pass(cmd, pass, framebuffer);

    // Parallel recording of draw calls
    @parallel(object_index)
    @strategy { batch_size: ?draw_batch_size }
    for i in range(scene.object_count) {
        gpu.bind_pipeline(cmd, scene.objects[i].pipeline);
        gpu.bind_descriptors(cmd, scene.objects[i].descriptors);
        gpu.draw_indexed(cmd, scene.objects[i].mesh);
    }

    gpu.end_render_pass(cmd);
}
```

## 3.4 Vulkan Memory Management (VMA)

### Why It Matters

Vulkan requires applications to manage GPU memory explicitly:
- Query available memory types (device-local, host-visible, host-coherent)
- Allocate memory in large blocks
- Sub-allocate from those blocks for individual resources
- Handle different memory types for different usage patterns

The Vulkan Memory Allocator (VMA) is the industry-standard solution, used in the majority of Vulkan game titles on PC.

### VMA Usage Patterns

| Pattern | Use Case | Strategy |
|---------|----------|----------|
| GPU-only | Render targets, storage images | `DEVICE_LOCAL`, dedicated memory for large resources |
| Staging upload | CPU-to-GPU transfer | `HOST_VISIBLE` + sequential write access |
| Readback | GPU-to-CPU transfer | `HOST_VISIBLE` + `HOST_CACHED` + random read |
| Dynamic uniform | Per-frame uniforms | `DEVICE_LOCAL` + `HOST_VISIBLE` (if available), with staging fallback |

### How AXIOM Could Handle It

```axiom
@module gpu_memory;

// GPU memory allocation with explicit type annotations
@gpu_memory(device_local)
@lifetime(level)
let render_target: GpuImage = gpu.create_image(
    width: 1920, height: 1080,
    format: VK_FORMAT_R8G8B8A8_SRGB,
    usage: VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT,
);

// Staging buffer with frame lifetime
@gpu_memory(host_visible)
@lifetime(frame)
@access(sequential_write)
let staging: GpuBuffer = gpu.create_buffer(
    size: mesh_data_size,
    usage: VK_BUFFER_USAGE_TRANSFER_SRC_BIT,
);

// Per-frame uniform buffer with double-buffering
@gpu_memory(dynamic_uniform)
@lifetime(frame)
@frames_in_flight(2)
let uniform_buffer: GpuBuffer = gpu.create_buffer(
    size: sizeof(FrameUniforms),
    usage: VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT,
);
```

## 3.5 Vulkan Synchronization

### The Four Primitives

1. **Fences** (GPU-to-CPU): The CPU waits for the GPU to complete a queue submission. Used for frame pacing -- "don't start frame N+2 until frame N finishes."

2. **Semaphores** (GPU-to-GPU): Signals between queue submissions. "Don't start rendering until the swapchain image is acquired."

3. **Pipeline Barriers** (within command buffer): Memory/execution barriers between commands. "Ensure the texture upload is complete before the fragment shader reads it."

4. **Timeline Semaphores** (Vulkan 1.2+): A superset of both semaphores and fences. Uses an integer counter that increments upon completion. Allows CPU-to-GPU, GPU-to-CPU, and GPU-to-GPU signaling with finer granularity than regular fences/semaphores.

### CPU-GPU Synchronization Patterns

**Double buffering**: 2 frames in flight. While GPU renders frame N, CPU prepares frame N+1. Fences prevent the CPU from getting more than 1 frame ahead.

**Triple buffering**: 3 frames in flight. Smoother but adds 1 frame of latency. The GPU always has work queued.

**Mailbox mode**: The compositor takes the latest rendered frame and drops older ones. Best for latency when combined with VSync.

### How AXIOM Could Handle It

```axiom
@module sync;

// Frame synchronization with explicit frames-in-flight
@constraint { max_frames_in_flight: 2 }
@gpu_sync
struct FrameSync {
    render_fence:     array[VkFence, MAX_FRAMES_IN_FLIGHT],
    image_available:  array[VkSemaphore, MAX_FRAMES_IN_FLIGHT],
    render_finished:  array[VkSemaphore, MAX_FRAMES_IN_FLIGHT],
    frame_index:      u32,
}

@gpu_sync_pattern(double_buffer)
fn begin_frame(sync: ptr[FrameSync]) -> u32 {
    let idx: u32 = sync.frame_index % MAX_FRAMES_IN_FLIGHT;

    // Wait for this frame's previous GPU work to complete
    @gpu_wait(sync.render_fence[idx])
    gpu.wait_fence(sync.render_fence[idx]);
    gpu.reset_fence(sync.render_fence[idx]);

    // Acquire next swapchain image
    @gpu_signal(sync.image_available[idx])
    let image_index: u32 = gpu.acquire_image(
        swapchain, sync.image_available[idx]
    );

    return image_index;
}
```

## 3.6 SPIR-V Generation -- Shaders from AXIOM

### The MLIR Path

MLIR has a mature SPIR-V dialect that can generate GPU shader code. The path would be:

```
AXIOM Shader Source
    |
    v
AXIOM HIR (with @gpu annotations)
    |
    v
MLIR (GPU dialect + Arith dialect)
    |
    v
MLIR SPIR-V dialect (SPIRVConversionTarget)
    |
    v
SPIR-V binary (mlir::spirv::serialize())
    |
    v
Vulkan VkShaderModule
```

Key MLIR SPIR-V features:
- **SPIRVTypeConverter**: Converts builtin types to SPIR-V equivalents
- **SPIRVConversionTarget**: Validates operations against target environment
- **Capability/Extension handling**: Auto-generated from TableGen specs
- **Entry point metadata**: `spirv.entry_point_abi` and `spirv.interface_var_abi`

### How AXIOM Could Handle It

```axiom
// GPU shader written in AXIOM syntax
@shader(vertex)
@entry_point
fn vertex_main(
    @location(0) position: Vec3,
    @location(1) normal: Vec3,
    @location(2) uv: Vec2,
    @builtin(position) out_position: Vec4,
    @location(0) out_normal: Vec3,
) -> void {
    let world_pos: Vec4 = uniforms.model * vec4(position, 1.0);
    out_position = uniforms.view_proj * world_pos;
    out_normal = mat3(uniforms.normal_matrix) * normal;
}

@shader(fragment)
@entry_point
fn fragment_main(
    @location(0) normal: Vec3,
    @location(0) out_color: Vec4,
) -> void {
    let light_dir: Vec3 = normalize(vec3(1.0, 1.0, 1.0));
    let diffuse: f32 = max(dot(normalize(normal), light_dir), 0.0);
    out_color = vec4(diffuse, diffuse, diffuse, 1.0);
}
```

**Concrete Feature Proposals:**

1. **`@shader(stage)` annotation**: Marks a function as a GPU shader. The compiler generates SPIR-V via MLIR.

2. **`@location(N)`, `@binding(N)`, `@builtin(name)`**: Vulkan interface annotations that map directly to SPIR-V decorations.

3. **Unified syntax**: CPU and GPU code use the same AXIOM syntax. The compiler determines the target based on annotations. This is similar to how CUDA uses `__device__` and `__host__` qualifiers, but with AXIOM's richer annotation system.

4. **Cross-compilation**: The same AXIOM function can be compiled for CPU (LLVM) or GPU (SPIR-V) depending on annotations. The AI optimizer can decide where to run code.

**Implementation Approach:**
- Phase 1: SPIR-V generation via MLIR's SPIR-V dialect (AXIOM HIR -> MLIR GPU -> MLIR SPIR-V -> binary)
- Phase 2: Ahead-of-time shader compilation embedded in AXIOM build
- Phase 3: Runtime shader specialization using AXIOM's `@strategy` holes

**Implementation Difficulty:** Very High. SPIR-V generation through MLIR is the long-term correct approach, but requires MLIR integration (planned but not yet implemented in AXIOM). A pragmatic intermediate step would be to generate GLSL/HLSL strings from AXIOM shader annotations, then compile with existing shader compilers (glslc, dxc).

## 3.7 Shader Compilation: Runtime vs Ahead-of-Time

### The Stutter Problem

Vulkan compiles shaders into Pipeline State Objects (PSOs) at runtime. This compilation can take milliseconds, causing visible stutters. Solutions:

1. **Ahead-of-time PSO compilation**: Pre-compile all shader permutations at build time. Guarantees no runtime stutters but requires knowing all state combinations.

2. **Pipeline caches**: Save compiled PSO state to disk. First run stutters; subsequent runs are fast.

3. **`VK_EXT_graphics_pipeline_library`**: Allows compiling shader stages independently, earlier than full PSO creation. Reduces stutter severity.

4. **`VK_EXT_shader_object`**: Eliminates pipeline combinatorics entirely. Shaders are compiled as individual stages and dynamically combined. Best for dynamic rendering.

### AXIOM's Approach

AXIOM should support ahead-of-time PSO compilation as the default, with `@strategy` holes for runtime specialization:

```axiom
@shader_pipeline
@compile(ahead_of_time)  // Compile all permutations at build time
@strategy {
    specialization_constants: {
        MAX_LIGHTS: ?max_lights,        // AI chooses: 8, 16, 32, 64
        SHADOW_QUALITY: ?shadow_quality  // AI chooses: low, medium, high
    }
}
fn create_lighting_pipeline() -> VkPipeline { ... }
```

---

# 4. ABI Conformance / Rust Interop

## 4.1 C ABI Compatibility

### What It Is and Why It Matters

The C ABI (Application Binary Interface) is the universal lingua franca of systems programming. Every operating system, every hardware driver, and virtually every library exposes a C ABI. For AXIOM to call Vulkan, link with game libraries, or interop with any existing code, it must produce and consume C ABI-compatible code.

### The Two Major x86_64 ABIs

**System V AMD64 ABI** (Linux, macOS, BSDs):
- First 6 integer/pointer args: RDI, RSI, RDX, RCX, R8, R9
- First 8 float/double args: XMM0-XMM7
- Return value: RAX (integer), XMM0 (float)
- Stack aligned to 16 bytes at call site

**Windows x64 ABI** (Windows):
- First 4 args (any type): RCX, RDX, R8, R9 / XMM0-XMM3
- Return value: RAX (integer), XMM0 (float)
- 32-byte shadow space on stack
- Stack aligned to 16 bytes

### Struct Layout Rules

For C ABI compatibility, AXIOM structs must follow C struct layout rules:
- Fields laid out in declaration order
- Each field aligned to its natural alignment
- Struct padded to align the next array element
- Total size is a multiple of the largest alignment

### What AXIOM Needs

```axiom
// C-compatible struct layout (explicit)
@repr(C)                       // Use C struct layout rules
@align(16)                     // Override default alignment
struct VkExtent2D {
    width: u32,
    height: u32,
}

// C-compatible function (extern block)
@extern("C")
fn vkCreateInstance(
    create_info: ptr[VkInstanceCreateInfo],
    allocator: ptr[VkAllocationCallbacks],
    instance: ptr[VkInstance],
) -> i32;

// AXIOM function callable from C
@export
@calling_convention(c)
fn axiom_game_update(dt: f32) -> i32 {
    // Game logic here
    return 0;
}
```

**Concrete Feature Proposals:**

1. **`@repr(C)` annotation**: Forces C-compatible struct layout. Fields are laid out with C padding/alignment rules. Without this, AXIOM may reorder fields for optimization.

2. **`@extern("C")` block**: Declares external C functions. AXIOM generates the correct calling convention for the target platform.

3. **`@calling_convention(c)` / `@calling_convention(system_v)` / `@calling_convention(win64)`**: Explicit calling convention selection.

4. **`@export` with C ABI**: Already implemented in AXIOM. Functions marked `@export` use external linkage and C calling convention.

**Implementation Difficulty:** Low-Medium. AXIOM already has `@export` with C calling convention via LLVM. Adding `@repr(C)` struct layout is straightforward. The extern function declaration syntax needs parser support.

## 4.2 Rust Interop

### The Challenge

Rust does not have a stable ABI. Rust's struct layout, function name mangling, and calling convention are all unspecified and can change between compiler versions. The only stable interface is `extern "C"` + `#[repr(C)]`.

### How Rust FFI Works

```rust
// Rust side: expose functions with C ABI
#[repr(C)]
pub struct GameState {
    pub position: [f32; 3],
    pub health: i32,
}

#[no_mangle]
pub extern "C" fn rust_physics_step(state: *mut GameState, dt: f32) {
    // Physics code here
}
```

```axiom
// AXIOM side: call Rust's C ABI functions
@repr(C)
struct GameState {
    position: array[f32, 3],
    health: i32,
}

@extern("C")
fn rust_physics_step(state: ptr[GameState], dt: f32) -> void;
```

### Can AXIOM Call Rust's `.rlib` Directly?

**No.** Rust's `.rlib` format is unstable and version-specific. The only reliable interop paths are:

1. **C ABI** (recommended): Both sides use `extern "C"` / `#[repr(C)]`. This is what AXIOM should target.
2. **Static linking via C ABI**: Rust compiles to a `.a` / `.lib` static library with C-compatible symbols. AXIOM links against it.
3. **Dynamic linking**: Rust compiles to `.so` / `.dll`. AXIOM loads at runtime via `dlopen` / `LoadLibrary`.

### Practical Interop Architecture

For a game engine, the recommended architecture:

```
AXIOM Game Logic (.axm)
    |  C ABI calls
    v
Rust Engine Core (.a / .lib)
    |  Contains: physics, audio, networking, Vulkan wrapper
    |  Exposes: C ABI functions via #[no_mangle] extern "C"
    v
Vulkan / OS APIs (C ABI)
```

This lets AXIOM handle the game logic (which benefits from annotation-driven optimization) while Rust handles the engine infrastructure (which benefits from borrow-checking and ecosystem).

**Concrete Feature Proposals:**

1. **`@link("library_name")` annotation**: Declares a library to link against. The compiler passes this to the linker.

2. **Binding generator**: A tool that reads Rust `cbindgen` output (C headers from `#[repr(C)]` Rust code) and generates AXIOM `@extern` declarations automatically.

3. **`@repr(C)` struct validation**: The compiler can validate that AXIOM `@repr(C)` structs match the expected layout of C/Rust counterparts, catching layout mismatches at compile time.

**Implementation Difficulty:** Medium. C ABI is mostly working via `@export`. The main work is `@extern` declarations, `@repr(C)` layout, and linking infrastructure.

---

# 5. What a "Game-Optimized Language" Would Need

## 5.1 Jai -- Jonathan Blow's Game Language

### Key Features for Games

Jai is the most directly relevant comparison for AXIOM's game ambitions. Key features:

1. **`SOA` keyword**: Automatic Structure-of-Arrays layout. Change one keyword at the struct definition and all code automatically uses SOA access patterns. No code changes needed.

2. **`#run` directive**: Arbitrary compile-time code execution. Any function can run at compile time. Blow demonstrated running an entire game at compile time to bake assets into the binary.

3. **Compile speed**: Target of 1 million lines per second. Public demos show 80,000-line codebase compiling in under 1 second.

4. **Integrated build system**: Build configuration is Jai code executed at compile time. No makefiles, no CMake, no external build tools.

5. **Built-in reflection**: Type introspection at both runtime and compile time without external tooling. Enables automatic serialization, networking, save games.

6. **No garbage collection**: Manual memory management with allocator support.

7. **Polymorphic procedures**: `$T` syntax for generic functions. Compiler generates specialized versions per type.

### Status (as of March 2026)

Jai remains in closed beta. Jonathan Blow's game "Order of the Sinking Star" is scheduled for 2026, built entirely in Jai. After the game releases, the engine will be open-sourced.

### What AXIOM Can Learn from Jai

AXIOM should steal Jai's best ideas and improve on them:

| Jai Feature | AXIOM Equivalent | AXIOM Advantage |
|-------------|------------------|-----------------|
| `SOA` keyword | `@layout(soa)` annotation | AI can benchmark SOA vs AOS and choose |
| `#run` compile-time | `@const` functions + `@strategy` | AI-driven compile-time optimization |
| Fast compilation | Target <2s for 100K lines | LLVM backend may be slower initially |
| Integrated build | `@module` + `@target` annotations | Build config as structured metadata |
| Reflection | `@reflect` annotation | Selective reflection reduces binary size |

## 5.2 Odin -- Bill Hall's Systems Language

### Key Features for Games

1. **`#soa` directive**: Built-in SOA conversion. `particles: #soa [10000]Particle` rearranges memory layout automatically. Access syntax is unchanged -- `particles[i].position` works the same.

2. **Context system**: An implicit `context` parameter passed to all Odin-convention functions. Contains the current allocator, logger, and other ambient state. Allows libraries to use different allocators without changing their API.

3. **Built-in allocator interface**: `mem.Allocator` is a first-class type. Every standard library function that allocates accepts an optional `Allocator` parameter.

4. **Temp allocator**: `context.temp_allocator` provides a built-in arena for temporary allocations.

5. **Official graphics bindings**: Vulkan, Direct3D 11/12, Metal, OpenGL, WebGL -- all in the standard library. Plus SDL2, GLFW, raylib.

6. **Array programming**: Swizzling, built-in vector/matrix types, quaternion support.

### What AXIOM Can Learn from Odin

| Odin Feature | AXIOM Equivalent | AXIOM Advantage |
|--------------|------------------|-----------------|
| `#soa` directive | `@layout(soa)` | AI-tunable, benchmarkable |
| Context system | `@context` annotation | Machine-verifiable context propagation |
| Allocator interface | `@allocator` types | Static analysis of allocation patterns |
| Temp allocator | `@lifetime(frame)` arena | Compile-time lifetime verification |
| Graphics bindings | Auto-generated from XML | Always up-to-date, complete |

## 5.3 Compile-Time Execution

### Why It Matters for Games

Compile-time execution moves work from runtime to build time:
- **Asset processing**: Convert images to GPU-optimal formats, compress meshes, bake lighting
- **Code generation**: Generate serialization code, shader permutations, lookup tables
- **Validation**: Verify game data integrity (no broken references, valid ranges)
- **Optimization**: Pre-compute expensive constant expressions

### How Languages Handle It

**Jai's `#run`**: Unrestricted -- any function can run at compile time. Can read files, do I/O, run other programs. Most powerful but potentially slow builds.

**Zig's `comptime`**: Restricted -- no I/O, no allocation, no undefined behavior. Runs in the compiler's memory. Safer but less powerful. Saves 40% runtime cost in benchmarked scenarios.

### AXIOM's Approach

AXIOM's `@const` annotation already declares compile-time-evaluable functions. The extension would be:

```axiom
// Compile-time asset processing
@const
@intent("Pre-compute sine lookup table for fast runtime access")
fn generate_sin_table() -> array[f32, 4096] {
    let table: array[f32, 4096] = array_zeros[f32, 4096];
    for i in range(4096) {
        let angle: f32 = widen(i) * 6.28318530718 / 4096.0;
        table[i] = sin(angle);
    }
    return table;
}

// Used at compile time -- table is embedded in binary
@const
let SIN_TABLE: array[f32, 4096] = generate_sin_table();
```

**Concrete Feature Proposals:**

1. **`@const` functions with I/O** (Jai-style): Allow reading files at compile time for asset embedding. Gated behind an explicit `@const_io` capability.

2. **`@embed` annotation**: Embed file contents as byte arrays at compile time. `let font: slice[u8] = @embed("fonts/roboto.ttf");`

3. **Build-time strategy evaluation**: `@strategy` holes can be resolved at build time by running the optimization loop during compilation, not just at a separate optimization step.

**Implementation Difficulty:** Medium. The `@const` evaluator needs to run AXIOM code in the compiler. This is essentially an interpreter. Phase 1 can support constant folding; Phase 2 can add full evaluation.

## 5.4 Hot Reloading

### Why It Matters for Games

Hot reloading lets developers modify gameplay code while the game is running. The typical flow:
1. Game runs from `game.dll`
2. Developer modifies source
3. Compiler produces new `game_new.dll`
4. Engine detects the new DLL, unloads old, loads new
5. Game continues with new code, preserving game state

### Architecture Requirements

1. **Game state must be in shared memory**: Not in the DLL's static variables. All state is passed as function parameters or lives in engine-owned memory.
2. **No C++ virtual functions on persistent objects**: Vtable pointers become invalid when the DLL changes.
3. **C ABI for the game/engine boundary**: Stable function signatures that don't change between reloads.
4. **Sum types are problematic**: Adding/removing variants breaks existing state.

### How AXIOM Could Handle It

```axiom
@module game;
@hot_reload                    // This module supports hot reloading
@export_table                  // Generate a function pointer table for DLL interface

// All game state is in this struct, owned by the engine
@repr(C)
@stable_layout                 // Compiler warns if layout changes between reloads
struct GameState {
    player_x: f32,
    player_y: f32,
    score: u32,
    // Adding fields at the END is safe for hot reload
    // Reordering or removing fields is a compile error with @stable_layout
}

@export
fn game_update(state: ptr[GameState], dt: f32) -> void {
    // Game logic here -- can be modified and hot-reloaded
    state.player_x = state.player_x + 1.0 * dt;
}

@export
fn game_render(state: ptr[GameState], renderer: ptr[Renderer]) -> void {
    // Rendering commands -- can be modified and hot-reloaded
    renderer.draw_sprite(state.player_x, state.player_y);
}
```

**Concrete Feature Proposals:**

1. **`@hot_reload` module annotation**: The compiler generates DLL output with a function pointer table. The engine loader watches for file changes and swaps DLLs.

2. **`@stable_layout` struct annotation**: The compiler tracks struct layout across compilations. Warns (or errors) when a hot-reloaded struct changes in an incompatible way (field reorder, removal, type change).

3. **`@export_table` annotation**: Generates a struct of function pointers that the engine uses to call game functions. New functions can be added; existing signatures are checked for compatibility.

**Implementation Difficulty:** Medium. DLL output requires platform-specific linking. The hot-reload protocol is well-understood from Handmade Hero and other projects.

## 5.5 Deterministic Memory Layout

### Why Games Need It

Games require deterministic memory because:
1. **Networking**: Multiplayer games must agree on data representation. Deterministic layout enables sending raw bytes.
2. **Save games**: Game state serialized to disk must be readable across sessions and versions.
3. **Replays**: Input replay systems depend on deterministic execution, which requires deterministic memory.
4. **Debugging**: Reproducible memory layouts make debugging memory corruption feasible.
5. **Performance consistency**: Unpredictable memory managers that are "fast most of the time but slow at unpredictable times" are unacceptable in games.

### AXIOM's Advantage

AXIOM's philosophy of radical explicitness is perfectly aligned with deterministic layout:
- No type inference means no ambiguity about sizes
- `@repr(C)` gives C-compatible, deterministic layout
- `@align(N)` gives explicit alignment control
- No implicit conversions means no hidden widening/narrowing
- Explicit allocators mean no hidden heap behavior

---

# 6. Specific Optimization Patterns for Games

## 6.1 Spatial Data Structures

### Spatial Hashing

Spatial hashing maps object positions to grid cells using a hash function. It provides O(1) average-case lookups for nearby objects. Used for:
- Collision detection (broad phase)
- Neighbor queries (AI, flocking, sound propagation)
- LOD selection (level-of-detail)

Hierarchical spatial hashing uses multiple hash tables with different cell sizes (powers of two, e.g., 1 to 32,768), enabling efficient queries at different scales.

### AXIOM Integration

```axiom
@module spatial;

@layout(soa)
@cache_line(64)
struct SpatialHash {
    cell_keys:   array[u32, MAX_OBJECTS],
    cell_starts: array[u32, GRID_SIZE],
    cell_counts: array[u32, GRID_SIZE],
    object_ids:  array[u32, MAX_OBJECTS],
}

@pure
@vectorizable(i)
@strategy { hash_function: ?hash_fn, grid_resolution: ?grid_res }
fn hash_positions(
    positions: slice[Vec3] @layout(soa),
    grid_size: f32,
    out: slice[u32],
) -> void {
    for i in range(positions.len) {
        let gx: i32 = truncate(positions[i].x / grid_size);
        let gy: i32 = truncate(positions[i].y / grid_size);
        let gz: i32 = truncate(positions[i].z / grid_size);
        out[i] = hash_cell(gx, gy, gz);
    }
}
```

## 6.2 Frustum Culling with SIMD

### Why It Matters

Every frame, the engine must determine which objects are visible. Testing thousands of bounding volumes against 6 frustum planes is embarrassingly parallel and perfect for SIMD.

### Performance Data

- SIMD frustum culling: **75% cost reduction** vs scalar
- SSE: ~3x speedup over basic C++
- SSE + multithreading: ~8.7x overall speedup
- Data must be in SOA format: positions in one array, radii in another, for efficient vectorized testing

### AXIOM Integration

```axiom
@module culling;

@vectorizable(i)
@target { cpu.sse4, cpu.avx2 }
@strategy { simd_width: ?simd_w, batch_size: ?batch }
fn frustum_cull(
    // SOA bounding sphere data
    center_x: slice[f32] @align(64),
    center_y: slice[f32] @align(64),
    center_z: slice[f32] @align(64),
    radius:   slice[f32] @align(64),
    // Frustum planes (6 planes, each as normal + distance)
    planes:   array[Vec4, 6] @align(64),
    // Output: visibility bits
    visible:  slice[u32],
    count:    u32,
) -> u32 {
    let visible_count: u32 = 0;
    for i in range(count) {
        let inside: bool = true;
        for p in range(6) {
            let dist: f32 = planes[p].x * center_x[i]
                          + planes[p].y * center_y[i]
                          + planes[p].z * center_z[i]
                          + planes[p].w;
            if dist < 0.0 - radius[i] {
                inside = false;
            }
        }
        if inside {
            visible[visible_count] = truncate(i);
            visible_count = visible_count + 1;
        }
    }
    return visible_count;
}
```

## 6.3 Physics Engine Optimization

### Data Layout for SIMD Physics

Box2D v3 demonstrates the state of the art in SIMD physics:

1. **Graph coloring**: Constraints are colored so that same-color constraints don't share bodies. This enables parallel solving without locks.

2. **Wide data types**: Bodies are gathered into SIMD-width groups:
   - SSE2/NEON: 4-wide (`__m128`)
   - AVX2: 8-wide (`__m256`)

3. **Performance**: AVX2 achieves 2.1x speedup over scalar for large simulations (5,050 bodies, 14,950 contact pairs).

4. **Lesson**: Compiler auto-vectorization is NOT sufficient. Hand-written SIMD with proper data layout is required for physics-level performance.

### AXIOM Physics Integration

```axiom
@module physics;

// SIMD-width body for batch processing
@simd(?physics_simd_width)
@layout(soa)
struct BodyBatch {
    linear_velocity_x:  simd[f32, ?physics_simd_width],
    linear_velocity_y:  simd[f32, ?physics_simd_width],
    angular_velocity:   simd[f32, ?physics_simd_width],
    inv_mass:           simd[f32, ?physics_simd_width],
    inv_inertia:        simd[f32, ?physics_simd_width],
}

@strategy {
    simd_width:     ?physics_simd_width    // 4 (SSE), 8 (AVX2)
    solver_iters:   ?solver_iterations     // 4-16
    color_count:    ?graph_colors          // 8-32
}
fn solve_contacts(
    constraints: slice[ContactConstraint] @layout(soa),
    bodies: slice[BodyBatch],
    iterations: u32,
) -> void {
    for iter in range(iterations) {
        for color in range(graph_colors) {
            @parallel(batch_index)
            for batch_index in range(constraints_per_color[color]) {
                solve_constraint_batch(constraints, bodies, color, batch_index);
            }
        }
    }
}
```

## 6.4 Animation System Optimization

### Key Optimization Techniques

1. **Linear bone arrays**: Store bones in a flat array ordered by hierarchy traversal (Root -> Torso -> Arms -> Legs), not a tree. This maximizes cache coherency during animation evaluation.

2. **SIMD matrix multiplication**: Bone matrices are 4x4 float matrices -- perfect for SIMD. Ensure alignment with `@align(16)`.

3. **Dirty flags**: Only recalculate bones that changed this frame. If a bone's local transform didn't change, its world transform is cached.

4. **GPU skinning**: Offload vertex skinning to GPU. CPU only computes bone matrices (typically 50-100 per character), then uploads to a uniform/storage buffer.

5. **Binary search for keyframes**: Finding the right keyframe in an animation is O(log n) with sorted keyframe arrays and binary search.

### AXIOM Integration

```axiom
@module animation;

@layout(soa)
@cache_line(64)
struct BoneArray {
    local_transform:  array[Mat4, MAX_BONES] @align(64),
    world_transform:  array[Mat4, MAX_BONES] @align(64),
    parent_index:     array[i16, MAX_BONES],
    dirty:            array[bool, MAX_BONES],
}

@vectorizable(bone_index)
@strategy { simd_width: ?anim_simd }
fn evaluate_skeleton(
    bones: ptr[BoneArray],
    bone_count: u32,
) -> void {
    // Bones are pre-sorted in hierarchy order
    // Parent is always before child in the array
    for bone_index in range(bone_count) {
        if bones.dirty[bone_index] {
            let parent: i16 = bones.parent_index[bone_index];
            if parent >= 0 {
                bones.world_transform[bone_index] =
                    mat4_mul(bones.world_transform[parent],
                             bones.local_transform[bone_index]);
            } else {
                bones.world_transform[bone_index] =
                    bones.local_transform[bone_index];
            }
        }
    }
}
```

## 6.5 Particle System -- Zero Allocation

### The Pattern

1. **Pre-allocate all particle memory at load time**: Define maximum particle counts per system.
2. **Pool allocator for particles**: O(1) alloc/free via free list.
3. **Per-thread memory pools**: Eliminate contention during parallel update.
4. **SIMD update loop**: Process 4-8 particles simultaneously in SOA layout.
5. **Single dynamic vertex buffer**: All particles write to one buffer, mapped once per frame with `WRITE_DISCARD`.

### Performance Data (24,000 particles, 200 emitters)

| Configuration | Time |
|--------------|------|
| 8 pools + SIMD | 2.25ms |
| 1 pool + SIMD | 6.42ms |
| 8 pools, no SIMD | 5.26ms |
| 1 pool, no SIMD | 12.10ms (estimated baseline) |

Multithreading: 2.8x improvement. SIMD: 2.3x improvement. Combined: ~5.4x.

### AXIOM Integration

```axiom
@module particles;

@constraint { per_frame_allocations: 0 }
@layout(soa)
@pool(MAX_PARTICLES)
struct ParticlePool {
    pos_x:     array[f32, MAX_PARTICLES] @align(64),
    pos_y:     array[f32, MAX_PARTICLES] @align(64),
    pos_z:     array[f32, MAX_PARTICLES] @align(64),
    vel_x:     array[f32, MAX_PARTICLES] @align(64),
    vel_y:     array[f32, MAX_PARTICLES] @align(64),
    vel_z:     array[f32, MAX_PARTICLES] @align(64),
    lifetime:  array[f32, MAX_PARTICLES] @align(64),
    alive:     array[bool, MAX_PARTICLES],
    count:     u32,
    free_list: array[u32, MAX_PARTICLES],
    free_count: u32,
}

@parallel(pool_index)
@vectorizable(i)
@constraint { allocations: 0 }
@strategy {
    simd_width:    ?particle_simd
    batch_size:    ?particle_batch
    pool_count:    ?pool_count
}
fn update_particles(
    pools: slice[ParticlePool],
    dt: f32,
) -> void {
    for pool_index in range(pools.len) {
        for i in range(pools[pool_index].count) {
            if pools[pool_index].alive[i] {
                pools[pool_index].pos_x[i] = pools[pool_index].pos_x[i]
                    + pools[pool_index].vel_x[i] * dt;
                pools[pool_index].pos_y[i] = pools[pool_index].pos_y[i]
                    + pools[pool_index].vel_y[i] * dt;
                pools[pool_index].pos_z[i] = pools[pool_index].pos_z[i]
                    + pools[pool_index].vel_z[i] * dt;
                pools[pool_index].lifetime[i] = pools[pool_index].lifetime[i] - dt;
                if pools[pool_index].lifetime[i] <= 0.0 {
                    pools[pool_index].alive[i] = false;
                    // Return to free list (O(1), no allocation)
                    pools[pool_index].free_list[pools[pool_index].free_count] = truncate(i);
                    pools[pool_index].free_count = pools[pool_index].free_count + 1;
                }
            }
        }
    }
}
```

## 6.6 Render Graph

### What It Is

A render graph is a directed acyclic graph (DAG) that describes an entire frame's rendering operations. Instead of imperatively recording commands, you declare:
- What render passes exist
- What resources each pass reads and writes
- What the dependencies between passes are

The render graph compiler then:
1. Topologically sorts passes
2. Inserts optimal barriers (minimizing "hard barriers" by maximizing distance between producer and consumer)
3. Aliases transient resources (reuses memory for resources with non-overlapping lifetimes)
4. Merges compatible passes (for tile-based GPUs)
5. Handles cross-queue synchronization (graphics + async compute)

### Key Optimizations

- **Global knowledge of the entire frame**: Modules reason locally about inputs/outputs; the graph optimizes globally.
- **Automatic barrier placement**: The graph knows resource state transitions and inserts barriers only where needed.
- **Memory aliasing**: Transient resources that don't overlap in time share the same GPU memory allocation.
- **Pass culling**: Dead passes (whose outputs are never consumed) are removed before execution.

### AXIOM Integration

```axiom
@module render_graph;

@render_pass
@reads(GBuffer.position, GBuffer.normal, GBuffer.albedo)
@writes(LightBuffer)
fn lighting_pass(
    gbuffer: ptr[GBufferResources] @gpu_read,
    light_buffer: ptr[GpuImage] @gpu_write,
    lights: slice[Light] @gpu_uniform,
) -> void {
    // Shader and draw commands here
}

@render_pass
@reads(LightBuffer)
@writes(Swapchain)
fn tonemap_pass(
    light_buffer: ptr[GpuImage] @gpu_read,
    output: ptr[GpuImage] @gpu_write,
) -> void {
    // Post-processing here
}

@render_graph
@constraint { max_submits_per_frame: 10 }
@strategy {
    async_compute: ?use_async_compute
    pass_merge:    ?enable_pass_merging
    alias_memory:  ?enable_memory_aliasing
}
fn build_frame_graph() -> RenderGraph {
    let graph: RenderGraph = render_graph.new();

    // Declare passes -- the graph compiler determines order
    graph.add_pass(gbuffer_pass);
    graph.add_pass(shadow_pass);
    graph.add_pass(lighting_pass);
    graph.add_pass(tonemap_pass);
    graph.add_pass(ui_pass);

    // Compile: topological sort, barrier insertion, memory aliasing
    graph.compile();

    return graph;
}
```

---

# 7. I/O, Threading, Stdlib for Games

## 7.1 Game Engine I/O Model

### Async File Loading

Games cannot block the main thread on file I/O. Asset loading must be:
1. **Asynchronous**: File reads happen on a dedicated I/O thread
2. **Streaming**: Large assets (textures, meshes, audio) stream in progressively
3. **Priority-based**: Assets needed by the player's current position are loaded first
4. **Memory-mapped** (optional): Large pak files can be memory-mapped for random access without buffering

### Multi-Stage Asset Loading

1. **I/O read** (async, I/O thread): Read raw bytes from disk
2. **Parse/decode** (worker thread): Decompress, parse headers, validate
3. **GPU upload** (render thread): Create GPU resources, upload to VRAM
4. **Ready** (main thread): Asset is available for use

### AXIOM Integration

```axiom
@module asset_io;

@async
@io_thread
fn load_asset(path: slice[u8]) -> Future[AssetData] {
    let raw: slice[u8] = io.read_file_async(path);
    let parsed: AssetData = asset.parse(raw);
    return parsed;
}

@gpu_upload_thread
fn upload_texture(data: ptr[TextureData]) -> GpuTexture {
    let staging: GpuBuffer = gpu.create_staging_buffer(data.size);
    gpu.copy_to_staging(staging, data.pixels);
    let texture: GpuTexture = gpu.create_image(data.width, data.height, data.format);
    gpu.copy_buffer_to_image(staging, texture);
    return texture;
}
```

## 7.2 Threading Model for Games

### The Modern Architecture

The recommended threading model for modern game engines:

```
Main Thread (Game Thread)
  |-- Reads input
  |-- Runs game logic
  |-- Spawns parallel jobs for:
  |     |-- Physics (fork-join across worker threads)
  |     |-- Animation (fork-join across worker threads)
  |     |-- AI pathfinding (fork-join across worker threads)
  |     |-- Particle updates (fork-join across worker threads)
  |-- Prepares render data
  |-- Submits render data to render thread

Render Thread
  |-- Receives render data from game thread
  |-- Records Vulkan command buffers
  |-- Submits to GPU queues
  |-- Handles swapchain presentation

Worker Threads (N-2 threads for N cores)
  |-- Execute jobs from work-stealing queues
  |-- Used by both game thread and render thread

I/O Thread
  |-- Async file reads
  |-- Asset streaming
  |-- Network I/O

Audio Thread (real-time priority)
  |-- Mix and output audio
  |-- Must never stall (hard real-time)
```

**Key insight from Doom Eternal**: Task-based architectures scale from 4-core to 16-core smoothly. Dedicated-thread architectures (Unreal's Game Thread + Render Thread) struggle past 4 cores.

### AXIOM Integration

```axiom
@module threading;

@thread(main)
fn game_frame(state: ptr[GameState], dt: f32) -> RenderData {
    // Fork: parallel physics + animation
    let physics_job = spawn @job simulate_physics(state, dt);
    let anim_job = spawn @job update_animation(state, dt);

    // Join: wait for both
    wait(physics_job, anim_job);

    // Prepare render data on main thread
    let render_data: RenderData = prepare_render(state);
    return render_data;
}

@thread(render)
fn render_frame(data: RenderData) -> void {
    let cmd: VkCommandBuffer = begin_command_buffer();
    record_draw_calls(cmd, data);
    submit_and_present(cmd);
}

@thread(audio)
@priority(realtime)
@constraint { latency_ms: "< 5" }
fn audio_callback(
    output: slice[f32],
    sample_rate: u32,
    frames: u32,
) -> void {
    // Mix audio -- must complete within deadline
    // No allocations, no locks, no I/O
    mix_audio(output, frames);
}
```

## 7.3 Math Library for Games

### Required Operations

Every game needs:
- `Vec2`, `Vec3`, `Vec4` -- vector types
- `Mat3`, `Mat4` -- matrix types (column-major for GPU compatibility)
- `Quat` -- quaternion for rotations (avoids gimbal lock)
- `AABB` -- axis-aligned bounding box
- Basic ops: dot, cross, normalize, lerp, slerp, perspective, look_at, inverse

### SIMD-Optimized Math

All math types should map to SIMD registers:
- `Vec4` -> `__m128` (SSE) / `float32x4_t` (NEON)
- `Mat4` -> 4x `__m128`
- `Quat` -> `__m128`

Reference: Rust's `glam` crate uses SIMD for Vec3A, Vec4, Quat, Mat4 on x86/wasm.

### AXIOM Math Library

```axiom
@module math;

@simd(4)
@repr(C)
@align(16)
struct Vec4 {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

@simd(4)
@repr(C)
@align(16)
struct Quat {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

@repr(C)
@align(64)
struct Mat4 {
    cols: array[Vec4, 4],
}

@pure
@inline(always)
@vectorizable(component)
fn vec4_add(a: Vec4, b: Vec4) -> Vec4 {
    return Vec4 {
        x: a.x + b.x,
        y: a.y + b.y,
        z: a.z + b.z,
        w: a.w + b.w,
    };
}

@pure
@inline(always)
fn dot(a: Vec4, b: Vec4) -> f32 {
    return a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
}

@pure
@inline(always)
fn mat4_mul(a: Mat4, b: Mat4) -> Mat4 {
    // SIMD-friendly: each column of result is a linear combination
    // of columns of a, weighted by elements of corresponding column of b
    // This pattern auto-vectorizes well
    let result: Mat4 = Mat4 { cols: array_zeros[Vec4, 4] };
    for col in range(4) {
        for row in range(4) {
            result.cols[col] = vec4_add(
                result.cols[col],
                vec4_scale(a.cols[row], b.cols[col].element(row)),
            );
        }
    }
    return result;
}
```

## 7.4 String Handling for Games

### The Problem with std::string

In game engines, `std::string` is toxic:
- Heap allocates on every construction
- Copying strings allocates
- Comparing strings is O(n)
- Strings are scattered in memory (poor cache behavior)

### Game Engine String Solutions

1. **String IDs / Hashed Strings**: Hash the string at compile time or load time. Store a 32/64-bit integer. Comparison is O(1). `foonathan::string_id` stores both hash and pointer to original string for debugging.

2. **Interned Strings**: Store all unique strings in a global pool. Comparison is pointer equality O(1). Used for asset names, event names, component names.

3. **Fixed-size string buffers**: Stack-allocated `char[64]` for short strings. No heap allocation. Truncates if too long.

4. **String views / slices**: Never own string memory. Always borrow from somewhere.

### AXIOM String Design

```axiom
@module strings;

// String ID -- compile-time hashed, O(1) comparison
@const
@repr(C)
struct StringId {
    hash: u64,
}

@const
@inline(always)
fn sid(s: slice[u8]) -> StringId {
    return StringId { hash: fnv1a_64(s) };
}

// Usage: zero-cost string comparison
let PLAYER_TAG: StringId = sid("player");
let entity_tag: StringId = get_entity_tag(entity);
if entity_tag.hash == PLAYER_TAG.hash {
    // O(1) comparison, no string allocation
}

// Fixed buffer for debug strings
@repr(C)
struct FixedString64 {
    data: array[u8, 64],
    len: u8,
}
```

## 7.5 Allocator Interface Design

### Lessons from Zig and Odin

**Zig**: Every standard library function that allocates takes an `Allocator` parameter. The allocator is a slice of bytes (`[]u8`) -- combining pointer and length. This makes it "frictionless and idiomatic" to use different allocation strategies per call site.

**Odin**: Uses an implicit `context` system. The current allocator is in `context.allocator`, passed implicitly to all Odin-convention functions. Libraries use the context allocator unless overridden.

**Rust**: `GlobalAlloc` trait defines a single global allocator. `#[global_allocator]` selects it. Cannot use different allocators in different parts of a program (without unsafe).

### AXIOM's Allocator Design

AXIOM should combine the best of all approaches:

```axiom
@module allocator;

// Allocator is a first-class interface type
@interface
type Allocator = struct {
    alloc_fn:   fn(ptr[void], u64, u64) -> ptr[u8],  // ctx, size, align -> ptr
    free_fn:    fn(ptr[void], ptr[u8], u64) -> void,  // ctx, ptr, size -> void
    ctx:        ptr[void],
};

// Built-in allocator types
@allocator(arena)
fn create_arena(size: u64) -> Allocator { ... }

@allocator(pool)
fn create_pool(element_size: u64, count: u64) -> Allocator { ... }

@allocator(ring)
fn create_ring(size: u64) -> Allocator { ... }

// Context system (inspired by Odin)
@context
struct GameContext {
    frame_allocator: Allocator,    // Reset every frame
    level_allocator: Allocator,    // Reset on level load
    permanent_allocator: Allocator, // Never freed
    temp_allocator: Allocator,     // Reset on scope exit
}

// Functions receive context implicitly (or explicitly)
fn spawn_particles(
    count: u32,
    @context ctx: GameContext,  // Implicit context parameter
) -> slice[Particle] {
    // Uses frame allocator -- zero-cost, reset at frame end
    let particles: slice[Particle] = alloc(
        ctx.frame_allocator,
        count * sizeof(Particle)
    );
    return particles;
}
```

**Concrete Feature Proposals:**

1. **`@context` implicit parameter**: Like Odin's context system. Every function implicitly receives a context parameter containing the current allocators, logger, etc. Can be overridden at any call site.

2. **Built-in allocator types**: Arena, Pool, Ring, DoubleStack as standard library types with known semantics.

3. **`@allocator` annotations on containers**: Every dynamic container (dynamic arrays, hash maps) takes an explicit allocator. No global allocator.

4. **Static allocation analysis**: With explicit allocators, the compiler can prove that a function makes zero heap allocations (it only uses arena bump or pool free-list operations).

**Implementation Difficulty:** Medium-High. The allocator interface is straightforward. The context system requires implicit parameter passing in the calling convention. Static allocation analysis is the hard part.

---

# 8. AXIOM Game Engine Feature Roadmap

## Phase 1: Foundation (Builds on Current AXIOM)

These features build directly on AXIOM's existing infrastructure:

| Feature | Priority | Difficulty | Dependencies |
|---------|----------|------------|-------------|
| `@repr(C)` struct layout | Critical | Low | Parser, codegen |
| `@extern("C")` function declarations | Critical | Low | Parser, codegen |
| `@link("library")` directive | Critical | Low | Driver, linker |
| Fixed-size arrays with `@align` | Critical | Low | Already partially implemented |
| `slice[T]` as fat pointer | Critical | Medium | Codegen |
| `ptr[T]` with unsafe blocks | Critical | Medium | Parser, codegen |
| Wrapping/saturating arithmetic | Done | Done | Already implemented |

## Phase 2: Memory System

| Feature | Priority | Difficulty | Dependencies |
|---------|----------|------------|-------------|
| Arena allocator (built-in) | Critical | Medium | Runtime library |
| Pool allocator (built-in) | Critical | Medium | Runtime library |
| `@lifetime(frame/level/permanent)` | High | Hard | Static analysis |
| `@constraint { allocations: 0 }` | High | Hard | Whole-program analysis |
| `@context` implicit parameters | High | Medium | Calling convention |
| `@allocator` on containers | Medium | Medium | Type system |

## Phase 3: Data Layout

| Feature | Priority | Difficulty | Dependencies |
|---------|----------|------------|-------------|
| `@layout(soa)` struct transform | Critical | Medium | HIR-to-LLVM lowering |
| `@layout(aos)` (default, explicit) | Critical | Low | Already implicit |
| `@cache_line(N)` alignment | High | Low | Maps to LLVM align |
| `@hot` / `@cold` field splitting | Medium | Hard | Struct splitting pass |
| SOA/AOS as `?layout` opt hole | High | Medium | Optimization protocol |

## Phase 4: SIMD and Parallelism

| Feature | Priority | Difficulty | Dependencies |
|---------|----------|------------|-------------|
| `simd[T, N]` vector type | Critical | Medium | LLVM vector types |
| `simd.*` intrinsic functions | Critical | Medium | Platform-specific lowering |
| `@vectorizable` auto-vectorization | High | Low | Already specified |
| `@parallel` with `?batch_size` | High | Medium | Job system runtime |
| `@job` annotation | High | High | Runtime library |
| Work-stealing scheduler | High | High | Runtime library |
| `?simd_width` optimization hole | High | Low | Optimization protocol |

## Phase 5: Vulkan Integration

| Feature | Priority | Difficulty | Dependencies |
|---------|----------|------------|-------------|
| Vulkan binding generator (from vk.xml) | Critical | Medium | `@extern`, `@repr(C)` |
| GPU memory annotations | High | Medium | Annotation system |
| Command buffer recording helpers | High | Medium | FFI, runtime |
| `@render_pass` annotation | Medium | Medium | Render graph design |
| Render graph compiler | Medium | Very High | Full render graph impl |
| `@gpu_sync` annotations | Medium | Medium | Vulkan sync wrappers |

## Phase 6: Shader System (SPIR-V)

| Feature | Priority | Difficulty | Dependencies |
|---------|----------|------------|-------------|
| `@shader(vertex/fragment/compute)` | High | Very High | MLIR integration |
| MLIR SPIR-V dialect lowering | High | Very High | MLIR backend |
| `@location`, `@binding` annotations | High | Medium | Annotation system |
| Ahead-of-time shader compilation | Medium | High | Build system |
| Shader specialization constants | Medium | Medium | `@strategy` integration |

## Phase 7: Game Loop Infrastructure

| Feature | Priority | Difficulty | Dependencies |
|---------|----------|------------|-------------|
| `@hot_reload` module annotation | High | Medium | DLL output |
| `@stable_layout` struct checking | Medium | Medium | Cross-compile analysis |
| String ID system | Medium | Low | `@const` evaluation |
| Math library (Vec3, Mat4, Quat) | Critical | Low | SIMD types |
| Async I/O primitives | Medium | High | Runtime, OS bindings |
| Audio thread support | Medium | Medium | Thread priorities |

---

# Appendix A: Comparison Matrix -- AXIOM vs Existing Game Languages

| Feature | C++ | Jai | Odin | Zig | Rust | **AXIOM** |
|---------|-----|-----|------|-----|------|-----------|
| SOA layout | Manual | `SOA` keyword | `#soa` directive | Manual | Manual | `@layout(soa)` + AI benchmarking |
| Zero-alloc verification | None | None | None | None | None | `@constraint { allocations: 0 }` |
| Compile-time execution | constexpr (limited) | `#run` (unrestricted) | None | `comptime` (restricted) | const fn (limited) | `@const` + `@const_io` |
| SIMD types | Intrinsics | Planned | `#simd` | `@Vector` | Nightly `std::simd` | `simd[T,N]` + `?simd_width` |
| Job system | Library | Built-in | Library | Library | Library | `@job` + `@parallel` + `@depends` |
| Allocator interface | `std::pmr` | Context | Context | Parameter | `GlobalAlloc` | `@context` + `@allocator` |
| GPU shaders | HLSL/GLSL | Unknown | Separate | Separate | Separate | Same syntax with `@shader` |
| Hot reload | DLL swap | Built-in | DLL swap | Unknown | Complex | `@hot_reload` + `@stable_layout` |
| Optimization protocol | None | None | None | None | None | `@strategy` + `?holes` + `@optimization_log` |
| AI-driven tuning | None | None | None | None | None | Full optimization loop |
| Data dependency tracking | Manual | Unknown | Manual | Manual | Borrow checker | `@reads` + `@writes` annotations |
| Calling convention | Default | C ABI | C ABI | C ABI | `extern "C"` | `@repr(C)` + `@export` |

---

# Appendix B: The Killer Demo -- What to Build

The "killer app" demo that proves AXIOM's value for games should be:

## A Particle Physics Benchmark

**10,000+ particles** with collision detection, rendered via Vulkan:

1. **Zero per-frame allocations** -- verified by `@constraint { allocations: 0 }`
2. **Parallel physics** -- `@job` annotation with work-stealing scheduler
3. **SIMD collision** -- `@vectorizable` with `?simd_width` optimization hole
4. **SOA data layout** -- `@layout(soa)` with AI benchmarking AOS vs SOA
5. **Vulkan rendering** -- Command buffer recording, double-buffered frames
6. **AI-optimized** -- `@strategy` holes for batch size, SIMD width, thread count, all tuned by AXIOM's optimization protocol

This demo would show:
- AXIOM matches or beats C++ for a compute-intensive game workload
- The AI optimization protocol actually improves performance iteratively
- The annotation system catches bugs (allocation in hot loop, data race between jobs)
- The same source compiles to multiple targets (SSE vs AVX vs NEON)

If this demo runs at 60 FPS with 10,000 SIMD-colliding particles, zero allocations, and parallel execution across all cores -- **AXIOM has proven its value as a game language.**

---

# Appendix C: Sources

## Game Engine Architecture
- [Unity DOTS - Data-Oriented Technology Stack](https://unity.com/dots)
- [Entity Component System - Wikipedia](https://en.wikipedia.org/wiki/Entity_component_system)
- [Bevy ECS - Rust Game Engine](https://github.com/bevyengine/bevy)
- [ECS FAQ - SanderMertens](https://github.com/SanderMertens/ecs-faq)
- [Nomad Game Engine: AoS vs SoA](https://medium.com/@savas/nomad-game-engine-part-4-3-aos-vs-soa-storage-5bec879aa38c)
- [Handmade Hero](https://hero.handmade.network/)
- [Casey Muratori - Clean Code, Horrible Performance](https://www.computerenhance.com/p/clean-code-horrible-performance)

## Memory Management
- [Start Pre-allocating And Stop Worrying](https://gamesfromwithin.com/start-pre-allocating-and-stop-worrying)
- [Arena and Memory Pool Allocators: The 50-100x Secret](https://medium.com/@ramogh2404/arena-and-memory-pool-allocators-the-50-100x-performance-secret-behind-game-engines-and-browsers-1e491cb40b49)
- [Game From Zero: Part 2 - Memory](https://bitnenfer.com/blog/2018/09/30/Building-Game-2.html)
- [Molecular Matters Memory System](https://blog.molecular-matters.com/2011/07/15/memory-system-part-4/)
- [Custom Allocators in C++23](https://markaicode.com/custom-allocators-cpp23-game-engines/)
- [Object Pool Pattern - Game Programming Patterns](https://gameprogrammingpatterns.com/object-pool.html)

## Job Systems and Parallelism
- [Parallelizing the Naughty Dog Engine Using Fibers - GDC 2015](https://gdcvault.com/play/1022186/Parallelizing-the-Naughty-Dog-Engine)
- [Molecular Matters Job System 2.0: Lock-Free Work Stealing](https://blog.molecular-matters.com/2015/08/24/job-system-2-0-lock-free-work-stealing-part-1-basics/)
- [enkiTS Task Scheduler](https://github.com/dougbinks/enkiTS)
- [Unity Burst Compiler Guide](https://docs.unity3d.com/Packages/com.unity.burst@1.8/manual/index.html)
- [Multithreading for Game Engines - Vulkan Guide](https://vkguide.dev/docs/extra-chapter/multithreading/)
- [SergeyMakeev/TaskScheduler - Fiber-Based](https://github.com/SergeyMakeev/TaskScheduler)

## Vulkan and GPU Programming
- [Vulkan Command Buffers Tutorial](https://docs.vulkan.org/tutorial/latest/03_Drawing_a_triangle/03_Drawing/01_Command_buffers.html)
- [VMA - Vulkan Memory Allocator](https://github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator)
- [VMA Recommended Usage Patterns](https://gpuopen-librariesandsdks.github.io/VulkanMemoryAllocator/html/usage_patterns.html)
- [Render Graphs and Vulkan - A Deep Dive](https://themaister.net/blog/2017/08/15/render-graphs-and-vulkan-a-deep-dive/)
- [Understanding Vulkan Synchronization](https://www.khronos.org/blog/understanding-vulkan-synchronization)
- [Timeline Semaphores - ARM](https://developer.arm.com/community/arm-community-blogs/b/mobile-graphics-and-gaming-blog/posts/vulkan-timeline-semaphores)
- [VK_EXT_graphics_pipeline_library](https://www.khronos.org/blog/reducing-draw-time-hitching-with-vk-ext-graphics-pipeline-library)
- [Writing an Efficient Vulkan Renderer](https://zeux.io/2020/02/27/writing-an-efficient-vulkan-renderer/)
- [ash - Vulkan Bindings for Rust](https://github.com/ash-rs/ash)
- [vulkan-zig - Vulkan Binding Generator for Zig](https://github.com/Snektron/vulkan-zig)

## SPIR-V and Shaders
- [MLIR SPIR-V Dialect](https://mlir.llvm.org/docs/Dialects/SPIR-V/)
- [SPIR-V Wikipedia](https://en.wikipedia.org/wiki/Standard_Portable_Intermediate_Representation)
- [Shader Objects Extension](https://docs.vulkan.org/samples/latest/samples/extensions/shader_object/README.html)

## Language Design
- [Jai Programming Language - Wikipedia](https://en.wikipedia.org/wiki/Jai_(programming_language))
- [Jai Primer](https://github.com/BSVino/JaiPrimer/blob/master/JaiPrimer.md)
- [Jai, the Game Programming Contender](https://bitshifters.cc/2025/04/28/jai.html)
- [Jai in 2026: The State of Jonathan Blow's Language](https://www.mrphilgames.com/blog/jai-in-2026)
- [Odin Programming Language](https://odin-lang.org/)
- [Odin's Context System](https://www.gingerbill.org/article/2025/12/15/odins-most-misunderstood-feature-context/)
- [Zig Allocators](https://zig.guide/standard-library/allocators/)
- [Zig's Comptime](https://kristoff.it/blog/what-is-zig-comptime/)

## SIMD and Physics
- [Box2D: SIMD Matters](https://box2d.org/posts/2024/08/simd-matters/)
- [SIMD Optimization - Wolfire Games](http://blog.wolfire.com/2010/09/SIMD-optimization)
- [Practical Cross-Platform SIMD Math](https://www.gamedev.net/tutorials/programming/general-and-gameplay-programming/practical-cross-platform-simd-math-r3068/)
- [Frustum Culling with SIMD - Bitsquid](http://bitsquid.blogspot.com/2016/10/the-implementation-of-frustum-culling.html)
- [CPU Particle Systems](https://alextardif.com/Particles.html)
- [glam - Rust Math Library](https://github.com/bitshifter/glam-rs)

## ABI and Interop
- [x64 ABI Conventions - Microsoft](https://learn.microsoft.com/en-us/cpp/build/x64-software-conventions)
- [x86 Calling Conventions - Wikipedia](https://en.wikipedia.org/wiki/X86_calling_conventions)
- [Rust FFI - The Rustonomicon](https://doc.rust-lang.org/nomicon/ffi.html)
- [Effective Rust: Item 34 - Control FFI Boundaries](https://effective-rust.com/ffi.html)
- [rust-bindgen](https://github.com/rust-lang/rust-bindgen)

## Game Engine I/O and Threading
- [Hot Reload Gameplay Code - Karl Zylinski](https://zylinski.se/posts/hot-reload-gameplay-code/)
- [Hot Reloading in Exile](https://thenumb.at/Hot-Reloading-in-Exile/)
- [Game String Interning](https://akhenatengame.squarespace.com/devblog/game-string-interning)
- [String IDs for Games](https://cowboyprogramming.com/2007/01/04/practical-hash-ids/)
- [Skeletal Animation Optimization](https://www.gamedev.net/tutorials/programming/graphics/skeletal-animation-optimization-tips-and-tricks-r3988/)
- [Unreal Engine Async Asset Loading](https://docs.unrealengine.com/5.1/en-US/asynchronous-asset-loading-in-unreal-engine/)
- [Double Buffering - Vulkan Guide](https://vkguide.dev/docs/chapter-4/double_buffering/)
