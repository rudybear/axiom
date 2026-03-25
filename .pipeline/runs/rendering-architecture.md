# AXIOM Rendering Architecture: Full System Design

## Status: SPECIFICATION
## Date: 2026-03-24
## Author: Architect Agent

---

## 1. Executive Summary

This document evaluates three architectural approaches for AXIOM's rendering system and provides a concrete recommendation. The two user requirements are:

1. **Full rendering engine** -- PBR, lights, shadows, deferred shading, RT, Gaussian splatting -- matching what Lux's playground already demonstrates.
2. **Vulkan API wrapping in AXIOM** -- the renderer logic itself written in AXIOM, enabling `@pure`, `@strategy`, `@parallel_for`, and LLM optimization on rendering code, with a path to GPGPU and general GPU programming.

**Recommendation: Approach C (Hybrid Layered Architecture)** with a phased rollout that starts with Approach A for immediate rendering capability, then progressively migrates rendering logic into AXIOM.

---

## 2. Current State Assessment

### What exists today

| Component | Location | State |
|---|---|---|
| AXIOM compiler | `crates/axiom-{lexer,parser,hir,codegen,driver}` | Complete: 30K LOC, 450 tests, beats C by 3% |
| AXIOM renderer DLL | `axiom-renderer/` | wgpu-based, 2D only: triangles + points + Lux SPIR-V loading |
| C runtime renderer | `crates/axiom-driver/runtime/axiom_rt.c` | Win32 software raster fallback |
| Lux playground (Rust) | `lux/playground_rust/` | 26K LOC ash/Vulkan: raster, RT, deferred, mesh, splat renderers |
| Lux compiler | `lux/luxc/` | Python, 1,462 tests, produces SPIR-V + reflection JSON |
| Lux stdlib | `lux/luxc/stdlib/` | 15 modules: brdf, lighting, shadow, ibl, pbr_pipeline, noise, etc. |
| Lux shaders | `lux/examples/` | 60+ shaders: PBR, deferred, RT pathtracing, Gaussian splats |

### AXIOM language capabilities relevant to rendering

- **Struct types**: Parsed, HIR-lowered, codegen emits `%struct.Name = type { ... }` with field access and `memset` zero-init. Field read/write via GEP codegen.
- **extern fn**: Full support. User-declared extern functions emit `declare` in LLVM IR and link at binary time.
- **Pointers**: `ptr[T]`, `readonly_ptr[T]`, `writeonly_ptr[T]` with `noalias` on all params.
- **Arrays**: `array[T, N]` fixed-size stack-allocated.
- **Annotations**: `@pure`, `@strategy`, `@vectorizable`, `@parallel_for`, `@inline`, `@lifetime`, `@export`.
- **Job system**: `job_dispatch`, `job_dispatch_handle`, `job_dispatch_after`, `job_wait_handle` -- dependency graphs.
- **Atomics**: `atomic_load`, `atomic_store`, `atomic_add`, `atomic_cas`.
- **File I/O**: `file_read`, `file_write`, `file_size`.
- **Current renderer builtins**: 12 functions (`renderer_create`, `renderer_draw_triangles`, `shader_load`, `pipeline_create`, etc.).

### Key limitations of AXIOM today

1. **No generics codegen** -- parsed but not emitted. Cannot write `fn create_buffer[T](...)`.
2. **No method syntax** -- no `obj.method()` dispatch on structs.
3. **No enum/sum type codegen** -- option/result are builtin i64-packed hacks, not general.
4. **No dynamic dispatch** -- `fn_ptr` builtins exist but are limited to single-arg calls.
5. **No string formatting** -- `print_i32`, `print_f64` only; no sprintf.
6. **Struct limitations** -- no nested structs, no struct-in-arrays, no struct return values from functions (only primitives returned).

---

## 3. Approach A: "Engine as a Rust DLL"

### Architecture

```
AXIOM program (.axm)
    |
    | calls via extern fn (C ABI)
    v
axiom-renderer.dll (Rust, ash-based)
    |--- vulkan_context.rs    (Vulkan bootstrap)
    |--- raster_renderer.rs   (forward PBR + shadows)
    |--- deferred_renderer.rs (G-buffer + lighting pass)
    |--- rt_renderer.rs       (ray tracing)
    |--- splat_renderer.rs    (Gaussian splatting)
    |--- scene_manager.rs     (GPU buffer management)
    |--- gltf_loader.rs       (glTF mesh + material loading)
    |--- reflected_pipeline.rs (Lux .json -> VkPipeline)
    v
Lux SPIR-V shaders (.spv) loaded at runtime
```

### What the AXIOM program looks like

```axiom
fn main() -> i32 {
    let r: ptr[i32] = renderer_create(1280, 720, "AXIOM Scene");
    renderer_load_scene(r, "sponza.gltf");
    renderer_set_mode(r, 1);  // 0=raster, 1=deferred, 2=RT
    renderer_add_light(r, 0, 0.0, -1.0, 0.0, 1.0, 1.0, 1.0, 5.0);

    while renderer_should_close(r) == 0 {
        renderer_begin_frame(r);
        renderer_render(r);
        renderer_end_frame(r);
    }
    renderer_destroy(r);
    return 0;
}
```

### Effort estimate

- Refactor `lux/playground_rust/` into a `lux-core` lib crate: **2-3 days**
- New C ABI exports (load_scene, set_mode, add_light, render): **3-5 days**
- Integrate lux-core as dependency in axiom-renderer: **1-2 days**
- Total: **~2 weeks** for full Lux feature parity

### Pros

1. Immediate access to all 5 Lux render paths (26K LOC of battle-tested Vulkan code).
2. Full PBR, IBL, shadows, ray tracing, Gaussian splatting, mesh shaders on day 1.
3. glTF loading, per-material textures, bindless descriptor indexing already implemented.
4. Lux shader reflection (`.lux.json`) auto-creates descriptor sets and pipelines.
5. Minimal AXIOM compiler changes needed (just add ~10 new builtin names).

### Cons

1. **AXIOM is just a caller, not the engine.** The language's unique advantages (`@pure`, `@strategy`, `@parallel_for`, LLM optimization) cannot optimize rendering code -- it's all in Rust.
2. **No path to GPGPU.** Computing in AXIOM would require more Rust wrapper functions for every new use case.
3. **Black box.** Users cannot see, modify, or optimize the renderer from AXIOM.
4. **Dependency bloat.** `ash`, `gpu-allocator`, `gltf`, `image`, `glam`, `serde` all pulled into the AXIOM build.
5. **Violates the AXIOM philosophy** -- "AI agents read/write here" -- but the renderer is in Rust.

---

## 4. Approach B: "Raw Vulkan in AXIOM"

### Architecture

```
AXIOM program (.axm) -- IS the renderer
    |
    | extern fn vkCreateBuffer(...)
    | extern fn vkCmdDraw(...)
    | extern fn vkQueueSubmit(...)
    |
    v
Thin Rust bootstrap DLL (~300 lines)
    |--- Create VkInstance, VkDevice, VkQueue
    |--- Create VkSurface + VkSwapchain
    |--- Expose device/queue/swapchain handles via C ABI
    v
vulkan-1.dll (system Vulkan loader)
```

### What the AXIOM program looks like

```axiom
// Bootstrap: get Vulkan handles from the thin helper
extern fn vk_bootstrap_init(w: i32, h: i32, title: ptr[i8]) -> ptr[i32];
extern fn vk_bootstrap_get_device(ctx: ptr[i32]) -> ptr[i32];
extern fn vk_bootstrap_get_queue(ctx: ptr[i32]) -> ptr[i32];
extern fn vk_bootstrap_acquire_image(ctx: ptr[i32]) -> i32;
extern fn vk_bootstrap_present(ctx: ptr[i32], image_idx: i32);

// Raw Vulkan calls (linked directly against vulkan-1.dll)
extern fn vkCreateBuffer(device: ptr[i32], info: ptr[i32], alloc: ptr[i32], buf: ptr[i32]) -> i32;
extern fn vkAllocateMemory(device: ptr[i32], info: ptr[i32], alloc: ptr[i32], mem: ptr[i32]) -> i32;
extern fn vkBindBufferMemory(device: ptr[i32], buf: ptr[i32], mem: ptr[i32], offset: i64) -> i32;
extern fn vkCreateGraphicsPipelines(device: ptr[i32], cache: ptr[i32], count: i32, infos: ptr[i32], alloc: ptr[i32], pipelines: ptr[i32]) -> i32;
extern fn vkCmdBindPipeline(cmd: ptr[i32], bind_point: i32, pipeline: ptr[i32]);
extern fn vkCmdBindVertexBuffers(cmd: ptr[i32], first: i32, count: i32, buffers: ptr[i32], offsets: ptr[i64]);
extern fn vkCmdDraw(cmd: ptr[i32], vertex_count: i32, instance_count: i32, first_vertex: i32, first_instance: i32);
extern fn vkCmdBeginRenderPass(cmd: ptr[i32], info: ptr[i32], contents: i32);
extern fn vkCmdEndRenderPass(cmd: ptr[i32]);
extern fn vkQueueSubmit(queue: ptr[i32], count: i32, submits: ptr[i32], fence: ptr[i32]) -> i32;

// The AXIOM program manages everything
@pure @vectorizable
fn update_transforms(model: ptr[f64], view: ptr[f64], proj: ptr[f64],
                     mvp_out: ptr[f64], count: i32) {
    for i: i32 in range(0, count) {
        // matrix multiply -- @pure enables fast-math, SIMD
        mat4_mul(proj, view, model, mvp_out, i);
    }
}

@job
fn cull_objects(aabbs: ptr[f64], frustum: ptr[f64], visible: ptr[i32],
               count: i32) {
    // Frustum culling -- runs on job system threads
    for i: i32 in range(0, count) {
        visible[i] = frustum_test(aabbs, frustum, i);
    }
}
```

### Effort estimate

- Thin bootstrap DLL (instance, device, swapchain, present): **3-5 days**
- Vulkan struct layout definitions in AXIOM (VkBufferCreateInfo, VkRenderPassCreateInfo, etc.): **5-10 days**
- AXIOM rendering library (buffers, pipelines, render passes, draw): **15-25 days**
- PBR material system in AXIOM: **10-15 days**
- Shadow mapping in AXIOM: **5-10 days**
- Total: **~2-3 months** for basic PBR. **6+ months** for feature parity with Lux playground.

### The Vulkan struct problem

This is the critical blocker. Vulkan API functions take complex nested structs with `sType`, `pNext` chains, bitfields, and pointer members. For example, `VkGraphicsPipelineCreateInfo` alone has:

```c
typedef struct VkGraphicsPipelineCreateInfo {
    VkStructureType                    sType;           // u32
    const void*                        pNext;           // ptr chain
    VkPipelineCreateFlags              flags;           // u32
    uint32_t                           stageCount;      // u32
    const VkPipelineShaderStageCreateInfo* pStages;     // ptr to array of structs
    const VkPipelineVertexInputStateCreateInfo* pVertexInputState;   // ptr to struct
    const VkPipelineInputAssemblyStateCreateInfo* pInputAssemblyState; // ptr to struct
    // ... 7 more struct pointers
    VkPipelineLayout                   layout;          // handle (u64)
    VkRenderPass                       renderPass;      // handle (u64)
    // ... more fields
} VkGraphicsPipelineCreateInfo;
```

AXIOM today can define structs and emit field GEPs, but:
- No nested struct types (struct fields that are themselves structs)
- No struct pointer fields (pNext chains)
- No struct-typed function parameters passed by value
- No arrays of structs
- No void pointer casting

This means every Vulkan create-info struct must be built by manually writing bytes at computed offsets -- feasible but extremely error-prone and unreadable.

### Pros

1. **Full AXIOM optimization on rendering code.** `@pure` on transform math, `@strategy` on tile sizes, `@parallel_for` on culling, LLM optimization on the entire renderer.
2. **GPGPU path.** Once you can call `vkCmdDispatch`, AXIOM programs can launch compute shaders.
3. **True GPU programming language.** AXIOM becomes a systems language for GPU programming, not just CPU.
4. **AI agents can optimize the renderer itself** -- the ultimate AXIOM demonstration.

### Cons

1. **Enormous API surface.** Vulkan has 200+ functions and 500+ struct types. Even the 20-30 most essential ones require thousands of lines of AXIOM struct definitions.
2. **AXIOM struct codegen is incomplete.** No nested structs, no struct parameters, no struct arrays, no void* casts. Major compiler work needed before a single Vulkan call.
3. **Months to basic rendering.** A simple triangle via raw Vulkan requires ~800 lines of initialization. In AXIOM without good struct support, this balloons to 2000+.
4. **No glTF loader, no IBL, no shadow infrastructure** -- all must be built from scratch in AXIOM.
5. **Duplication.** Lux's 26K LOC playground already has all of this in Rust. Rewriting it in AXIOM is massive effort for no new capability.
6. **Fragile.** Vulkan structs have precise alignment requirements. Manual byte offset management in AXIOM will produce subtle GPU crashes.

---

## 5. Approach C: "Hybrid Layered Architecture" (RECOMMENDED)

### Core Insight

The right answer is **both** -- use Lux's Rust infrastructure for what it does well (Vulkan plumbing, swapchain, GPU memory), and write the _rendering logic_ in AXIOM where AXIOM's optimizations matter. This mirrors how real game engines work:

- **Unreal Engine**: C++ platform layer (RHI) + C++ engine logic + HLSL shaders
- **Unity**: C++ native runtime + C# scripting + HLSL/GLSL shaders
- **Our design**: Rust Vulkan layer + AXIOM rendering logic + Lux shaders

### Architecture: Four Layers

```
 Layer 3: User game code (AXIOM .axm)
    |  game_loop(), update_entities(), custom rendering
    |  @pure, @strategy, @parallel_for -- all work here
    |
 Layer 2: AXIOM Rendering Library (AXIOM .axm, compiled to .o)
    |  Transform management, draw call batching, frustum culling,
    |  material system, light management, camera math
    |  THIS IS THE KEY LAYER -- rendering logic in AXIOM
    |
 Layer 1: GPU Backend DLL (Rust, C ABI)
    |  Vulkan resource management: buffers, images, pipelines, render passes
    |  Swapchain management, frame synchronization
    |  Descriptor set management, push constants
    |  SPIR-V shader module loading + Lux reflection
    |  glTF mesh/scene loading to GPU buffers
    |  Single "submit draw list" entry point
    |
 Layer 0: Lux Shaders (.lux -> .spv)
    |  PBR materials, lighting, shadows, post-processing
    |  Compiled separately by luxc, loaded at runtime
    v
 Vulkan driver + GPU hardware
```

### What goes where -- PRECISE BREAKDOWN

#### Layer 1: Rust GPU Backend DLL (`axiom-gpu/`)

This is a new crate replacing `axiom-renderer/`. It depends on `lux-core` (extracted from `lux/playground_rust/`).

**Rust owns** (things AXIOM cannot or should not do):

| Responsibility | Why Rust | Lines (est.) |
|---|---|---|
| VkInstance + VkDevice + VkQueue creation | Vulkan bootstrap is pure boilerplate with sType/pNext chains | Reuse `vulkan_context.rs` (1205 lines) |
| VkSwapchain management + present | Requires WSI integration, resize handling, image acquisition | Reuse from `vulkan_context.rs` |
| VkBuffer / VkImage creation | Requires gpu-allocator for memory management | Wrap `scene_manager.rs` helpers |
| VkRenderPass / VkFramebuffer creation | Complex Vulkan struct setup | New, ~300 lines |
| VkPipeline creation from SPIR-V | Requires reflection JSON parsing (serde), descriptor set layout | Reuse `reflected_pipeline.rs` (695 lines) |
| VkShaderModule creation | Trivial, but needs device handle | Reuse `spv_loader.rs` (85 lines) |
| Descriptor set allocation + update | Descriptor pool management, binding updates | New, ~400 lines |
| VkCommandBuffer allocation + submit | Frame synchronization (fences, semaphores) | New, ~300 lines |
| glTF loading to GPU buffers | Requires `gltf` crate, image decoding | Reuse `gltf_loader.rs` (1288 lines) |
| IBL environment map loading | HDR image loading, mipmap generation | Reuse from `scene_manager.rs` |
| Shadow map depth buffer management | Vulkan image + view creation | Extract from `raster_renderer.rs` |
| Screenshot / readback | GPU -> CPU image transfer | Reuse `screenshot.rs` (228 lines) |

**C ABI exposed to AXIOM** (~40 functions):

```c
// === Lifecycle ===
void* gpu_init(int width, int height, const char* title);
void  gpu_shutdown(void* ctx);
int   gpu_should_close(void* ctx);

// === Frame ===
int   gpu_begin_frame(void* ctx);          // acquire swapchain image, begin cmd buffer
void  gpu_end_frame(void* ctx);            // submit cmd buffer, present

// === Resource Creation ===
uint64_t gpu_create_buffer(void* ctx, int64_t size, int usage);  // returns handle
void     gpu_destroy_buffer(void* ctx, uint64_t buf);
void     gpu_upload_buffer(void* ctx, uint64_t buf, const void* data, int64_t size);
uint64_t gpu_create_image(void* ctx, int w, int h, int format);
void     gpu_upload_image(void* ctx, uint64_t img, const void* pixels, int64_t size);

// === Pipeline ===
uint64_t gpu_load_shader(void* ctx, const char* spv_path);
uint64_t gpu_create_pipeline(void* ctx, const char* shader_base);  // auto-loads .lux.json
void     gpu_destroy_pipeline(void* ctx, uint64_t pipeline);

// === Scene Loading (high-level) ===
uint64_t gpu_load_gltf(void* ctx, const char* path);             // returns scene handle
int      gpu_scene_mesh_count(void* ctx, uint64_t scene);
void     gpu_scene_get_mesh_aabb(void* ctx, uint64_t scene, int mesh_idx, double* aabb_out);
int      gpu_scene_material_count(void* ctx, uint64_t scene);

// === Draw Commands (the "draw list" pattern) ===
void gpu_cmd_begin_render_pass(void* ctx, int pass_type);        // 0=shadow, 1=gbuffer, 2=forward, 3=lighting
void gpu_cmd_end_render_pass(void* ctx);
void gpu_cmd_bind_pipeline(void* ctx, uint64_t pipeline);
void gpu_cmd_set_push_constants(void* ctx, const void* data, int size);
void gpu_cmd_bind_vertex_buffer(void* ctx, uint64_t buf);
void gpu_cmd_bind_index_buffer(void* ctx, uint64_t buf);
void gpu_cmd_draw(void* ctx, int vertex_count, int instance_count);
void gpu_cmd_draw_indexed(void* ctx, int index_count, int instance_count);
void gpu_cmd_dispatch_compute(void* ctx, int gx, int gy, int gz);

// === Uniform/Descriptor Updates ===
void gpu_set_uniform(void* ctx, uint64_t pipeline, int set, int binding, const void* data, int size);
void gpu_bind_texture(void* ctx, uint64_t pipeline, int set, int binding, uint64_t image);

// === Query ===
double gpu_get_time(void* ctx);
int    gpu_get_width(void* ctx);
int    gpu_get_height(void* ctx);
```

**Key design decision:** The draw commands record into a Vulkan command buffer managed by Rust. AXIOM calls `gpu_cmd_*` functions which translate to `vkCmd*` calls internally. This gives AXIOM control over _what_ is drawn and _in what order_, while Rust handles _how_ Vulkan state is managed.

#### Layer 2: AXIOM Rendering Library (`lib/render/`)

This is a collection of `.axm` files that are compiled and linked with user programs. THIS is where AXIOM's unique advantages shine.

**AXIOM owns** (things that benefit from AXIOM's annotations):

| Module | What it does | Why AXIOM | Key annotations |
|---|---|---|---|
| `transform.axm` | Model/view/projection matrix math, MVP computation | `@pure` -> fast-math + memory(none), vectorizable | `@pure`, `@vectorizable` |
| `camera.axm` | Perspective projection, look-at, orbit camera, FPS camera | `@pure` math functions | `@pure`, `@const` |
| `culling.axm` | Frustum culling, occlusion culling, LOD selection | Massively parallel, perfect for job system | `@parallel_for`, `@job` |
| `sort.axm` | Draw call sorting (front-to-back, back-to-front, by material) | `@strategy { algorithm: ?sort_algo }` | `@strategy`, `@pure` |
| `batch.axm` | Draw call batching, instancing, indirect draw buffer building | CPU-side optimization, benefits from `@pure` | `@pure`, `@vectorizable` |
| `lights.axm` | Light management, shadow matrix computation, light culling | `@parallel_for` on tile-based light assignment | `@parallel_for`, `@pure` |
| `materials.axm` | Material parameter packing, texture binding management | Data layout optimization | `@layout`, `@align` |
| `scene.axm` | Scene graph traversal, transform hierarchy, dirty flags | Tree traversal with `@parallel_for` on sibling groups | `@parallel_for` |
| `render_loop.axm` | Main render orchestration: shadow pass -> G-buffer -> lighting -> post | Sequencing logic, AXIOM controls the pipeline | `@strategy { render_path: ?mode }` |
| `aabb.axm` | AABB computation, intersection tests | `@pure` + `@vectorizable` -- perfect SIMD candidates | `@pure`, `@vectorizable` |
| `math.axm` | vec3/vec4/mat4 operations, quaternion math | `@pure` -> fast-math, `@const` for compile-time values | `@pure`, `@const`, `@inline(always)` |

**Example: `culling.axm`** -- showcasing AXIOM's optimization surface:

```axiom
@module culling;
@intent("Frustum and occlusion culling with parallel dispatch");

// Frustum planes: 6 planes x 4 floats (nx, ny, nz, d) = 24 floats
@pure
fn point_in_frustum(frustum: readonly_ptr[f64], px: f64, py: f64, pz: f64) -> i32 {
    for plane: i32 in range(0, 6) {
        let base: i32 = plane * 4;
        let nx: f64 = ptr_read_f64(frustum, base);
        let ny: f64 = ptr_read_f64(frustum, base + 1);
        let nz: f64 = ptr_read_f64(frustum, base + 2);
        let d:  f64 = ptr_read_f64(frustum, base + 3);
        let dist: f64 = nx * px + ny * py + nz * pz + d;
        if dist < 0.0 {
            return 0;  // Outside this plane
        }
    }
    return 1;  // Inside all planes
}

@pure
fn aabb_vs_frustum(frustum: readonly_ptr[f64],
                   min_x: f64, min_y: f64, min_z: f64,
                   max_x: f64, max_y: f64, max_z: f64) -> i32 {
    // Test AABB corners against frustum planes
    // ...
    return 1;
}

// This is the key: AXIOM's @parallel_for with the job system
// An AI agent can optimize the batch size via @strategy
@strategy { batch_size: ?cull_batch_size, range: [64, 4096], default: 512 }
@parallel_for(shared_read: [aabbs, frustum], shared_write: [visible], private: [])
fn cull_scene(aabbs: readonly_ptr[f64], frustum: readonly_ptr[f64],
              visible: writeonly_ptr[i32], count: i32) {
    for i: i32 in range(0, count) {
        let base: i32 = i * 6;
        visible[i] = aabb_vs_frustum(
            frustum,
            ptr_read_f64(aabbs, base),     ptr_read_f64(aabbs, base + 1),
            ptr_read_f64(aabbs, base + 2), ptr_read_f64(aabbs, base + 3),
            ptr_read_f64(aabbs, base + 4), ptr_read_f64(aabbs, base + 5)
        );
    }
}
```

**Example: `render_loop.axm`** -- orchestrating the full pipeline:

```axiom
@module render_loop;
@intent("Main render orchestration: shadow -> geometry -> lighting -> post-process");

// An AI agent can switch between forward and deferred rendering
@strategy { render_path: ?mode, options: ["forward", "deferred"], default: "deferred" }
fn render_frame(ctx: ptr[i32], scene: ptr[i32], camera: ptr[f64],
                lights: ptr[f64], light_count: i32) -> i32 {
    // 1. Update transforms (parallel, @pure math)
    update_scene_transforms(scene);

    // 2. Frustum culling (parallel job dispatch)
    let visible_count: i32 = cull_visible_objects(scene, camera);

    // 3. Sort draw calls (strategy-selectable algorithm)
    sort_draw_calls(scene, camera, visible_count);

    // 4. Shadow pass (for each shadow-casting light)
    for li: i32 in range(0, light_count) {
        if light_casts_shadow(lights, li) == 1 {
            gpu_cmd_begin_render_pass(ctx, 0);  // shadow pass
            render_shadow_map(ctx, scene, lights, li);
            gpu_cmd_end_render_pass(ctx);
        }
    }

    // 5. Geometry pass
    gpu_cmd_begin_render_pass(ctx, 1);  // G-buffer or forward
    render_geometry(ctx, scene, visible_count);
    gpu_cmd_end_render_pass(ctx);

    // 6. Lighting pass (deferred only)
    gpu_cmd_begin_render_pass(ctx, 3);  // lighting
    render_lighting(ctx, lights, light_count);
    gpu_cmd_end_render_pass(ctx);

    return visible_count;
}
```

#### Layer 0: Lux Shaders (unchanged)

Lux shaders are compiled separately by `luxc` and loaded as SPIR-V at runtime.

| Shader | File | Used by |
|---|---|---|
| Forward PBR | `gltf_pbr.lux` | Layer 2 forward path |
| Deferred G-buffer | `deferred_basic.lux` (gbuf pass) | Layer 2 deferred geometry |
| Deferred lighting | `deferred_basic.lux` (light pass) | Layer 2 deferred lighting |
| Shadow depth | Embedded SPIR-V (vertex-only) | Layer 2 shadow maps |
| Post-processing | `tonemap.lux` | Layer 2 post-process |
| Compute (GPGPU) | `compute_*.lux` | Layer 2/3 compute dispatch |
| RT pathtracer | `gltf_pbr_rt.lux` | Layer 2 RT path |
| Gaussian splat | `gaussian_splat*.lux` | Layer 2 splat path |

Layer 3 (user code) can also write its own Lux shaders and load them.

#### Layer 3: User Game Code (AXIOM .axm)

```axiom
@module my_game;
@intent("PBR scene with dynamic lights and particle effects");

fn main() -> i32 {
    let ctx: ptr[i32] = gpu_init(1280, 720, "My Game");

    // Load scene
    let scene: ptr[i32] = gpu_load_gltf(ctx, "assets/sponza.gltf");

    // Load shaders (Lux-compiled SPIR-V)
    let pbr_pipeline: i64 = gpu_create_pipeline(ctx, "shaders/gltf_pbr");

    // Setup camera (AXIOM math library)
    let camera: array[f64, 16] = array_zeros[f64, 16];
    camera_look_at(camera, 0.0, 2.0, 5.0, 0.0, 1.0, 0.0);

    // Setup lights
    let lights: array[f64, 128] = array_zeros[f64, 128];
    light_set_directional(lights, 0, 0.0, -1.0, -0.5, 1.0, 1.0, 0.9, 3.0);
    light_set_point(lights, 1, 2.0, 3.0, 0.0, 1.0, 0.4, 0.2, 10.0);

    // Main loop
    while gpu_should_close(ctx) == 0 {
        gpu_begin_frame(ctx);

        // AXIOM rendering library orchestrates everything
        let visible: i32 = render_frame(ctx, scene, camera, lights, 2);

        gpu_end_frame(ctx);
    }

    gpu_shutdown(ctx);
    return 0;
}
```

---

## 6. Comparison Matrix

| Criterion | Approach A (Rust DLL) | Approach B (Raw Vulkan) | Approach C (Hybrid) |
|---|---|---|---|
| Time to first PBR frame | **1-2 weeks** | 2-3 months | **3-4 weeks** |
| Full Lux feature parity | **Immediate** | 6+ months | 2-3 months |
| `@pure` on rendering code | No | **Yes** | **Yes** (Layer 2) |
| `@strategy` optimization | No | **Yes** | **Yes** (Layer 2) |
| `@parallel_for` culling | No | **Yes** | **Yes** (Layer 2) |
| LLM optimization of renderer | No | **Yes** | **Yes** (Layer 2) |
| GPGPU / compute shaders | No | **Yes** | **Yes** (via gpu_cmd_dispatch) |
| Compiler changes needed | Minimal | **Massive** (structs, generics) | Moderate |
| Risk of Vulkan crashes | Low | **Very high** | Low (Rust guards) |
| Lux shader ecosystem | **Fully integrated** | Manual loading | **Fully integrated** |
| glTF loading | **Built-in** | Must build from scratch | **Built-in** (Layer 1) |
| IBL / environment maps | **Built-in** | Must build from scratch | **Built-in** (Layer 1) |
| Shadow mapping | **Built-in** | Must build from scratch | **Built-in** (Layer 1 + 2) |
| Code AXIOM agents can see | 0% | 100% | **~60%** (all rendering logic) |
| Lines of new code | ~500 | ~15,000+ | ~4,000 |

---

## 7. Recommendation: Approach C with Phased Rollout

### Phase 1: Foundation (Week 1-2)

**Goal:** Get a window with a PBR sphere rendering via the new layered architecture.

1. **Extract `lux-core` library crate** from `lux/playground_rust/`
   - Add `[lib]` target to `lux/playground_rust/Cargo.toml`
   - Create `lux/playground_rust/src/lib.rs` exporting: `vulkan_context`, `spv_loader`, `reflected_pipeline`, `scene`, `scene_manager`, `gltf_loader`, `camera`, `screenshot`
   - Modify `main.rs` to use `lux_core::` imports

2. **Create `axiom-gpu/` crate** (new, replaces `axiom-renderer/`)
   - Depends on `lux-core` via path dependency
   - Implements C ABI: `gpu_init`, `gpu_shutdown`, `gpu_begin_frame`, `gpu_end_frame`, `gpu_should_close`
   - Implements: `gpu_create_buffer`, `gpu_upload_buffer`, `gpu_create_pipeline`, `gpu_load_gltf`
   - Implements: `gpu_cmd_begin_render_pass`, `gpu_cmd_end_render_pass`, `gpu_cmd_bind_pipeline`, `gpu_cmd_draw`, `gpu_cmd_draw_indexed`
   - Implements: `gpu_set_uniform`, `gpu_bind_texture`
   - Internally uses `lux-core`'s `VulkanContext`, `reflected_pipeline`, `scene_manager`

3. **Register new builtins in AXIOM compiler**
   - Add ~30 new extern declarations in `axiom-codegen/src/llvm.rs` (similar to existing renderer builtins)
   - Add detection for `gpu_*` function names in the builtin scanner

4. **Create AXIOM math library** (`lib/render/math.axm`)
   - `mat4_identity`, `mat4_mul`, `mat4_translate`, `mat4_rotate`, `mat4_scale`
   - `vec3_normalize`, `vec3_dot`, `vec3_cross`, `vec3_length`
   - All `@pure @inline(always)` -- will be fast-math optimized

5. **Create minimal render loop** (`examples/pbr_sphere/pbr_sphere.axm`)
   - Load sphere geometry, create PBR pipeline, setup camera + light, render

**Deliverable:** AXIOM program renders a PBR-shaded sphere with a single directional light.

### Phase 2: Rendering Library (Week 3-5)

**Goal:** Frustum culling, multiple lights, shadow maps, and draw call sorting -- all in AXIOM.

1. **`lib/render/camera.axm`** -- perspective, look-at, orbit, FPS camera
2. **`lib/render/culling.axm`** -- frustum culling with `@parallel_for`
3. **`lib/render/lights.axm`** -- light management, shadow matrix computation
4. **`lib/render/sort.axm`** -- draw call sorting with `@strategy`
5. **`lib/render/batch.axm`** -- instanced draw call batching
6. **`lib/render/render_loop.axm`** -- full render orchestration

**GPU Backend additions:**
- Shadow map render pass support (`gpu_cmd_begin_render_pass(ctx, 0)`)
- Push constant support (`gpu_cmd_set_push_constants`)
- Multiple render pass types (shadow, forward, deferred)

**Deliverable:** AXIOM program loads glTF scene, culls objects in parallel, renders with shadows.

### Phase 3: Deferred + Advanced (Week 6-8)

**Goal:** Deferred rendering pipeline, IBL, multiple materials.

1. **Deferred rendering path** -- G-buffer pass + lighting pass in `render_loop.axm`
2. **IBL integration** -- environment mapping via Layer 1
3. **Per-material textures** -- material parameter packing in AXIOM
4. **Post-processing** -- tonemap, exposure via fullscreen pass

**Deliverable:** Full deferred PBR with IBL, matching Lux playground quality.

### Phase 4: GPGPU + Compute (Week 9-10)

**Goal:** Compute shader dispatch from AXIOM, enabling GPGPU programming.

1. **`gpu_cmd_dispatch_compute(ctx, gx, gy, gz)`** -- dispatch compute shaders
2. **`gpu_create_storage_buffer`** -- read/write GPU buffers
3. **`gpu_readback_buffer`** -- GPU -> CPU data transfer
4. **Lux compute shaders** -- `compute_*.lux` loaded and dispatched from AXIOM
5. **Example: GPU particle system** -- compute shader updates, AXIOM dispatches

**Deliverable:** AXIOM program dispatches Lux compute shaders, reads results back.

### Phase 5: Ray Tracing + Splats (Week 11-14)

**Goal:** RT and Gaussian splatting from AXIOM.

1. **RT acceleration structure building** via Layer 1
2. **RT pipeline dispatch** via `gpu_cmd_trace_rays`
3. **Gaussian splat loading + rendering** via Layer 1 + Layer 2

**Deliverable:** Full feature parity with Lux playground, controlled from AXIOM.

---

## 8. Required AXIOM Compiler Changes

The hybrid approach requires **moderate** compiler changes, far less than Approach B:

### Must have (Phase 1)

| Change | Effort | Description |
|---|---|---|
| New GPU builtins | Small (1 day) | Add `gpu_*` function names to builtin detection, emit `declare` statements. Same pattern as existing `renderer_*` builtins in `llvm.rs` lines 620-678. |
| Struct field access codegen fixes | Medium (2-3 days) | Ensure `GEP` codegen for struct fields handles all primitive types (f32, i32, i64, f64). Current code works but needs testing with rendering-relevant layouts. |

### Should have (Phase 2)

| Change | Effort | Description |
|---|---|---|
| `f32` literal support | Small (1 day) | AXIOM has `f32` as a type but codegen may not handle `f32` arithmetic everywhere. Verify `fadd float`, `fmul float` paths. |
| Array-of-primitives as function params | Small | Already works via `ptr` passing. Verify with `f32` arrays. |
| Struct-as-function-parameter (by pointer) | Medium (3 days) | Structs passed as `ptr[MyStruct]` should work today. Verify GEP chains. |

### Nice to have (Phase 3+)

| Change | Effort | Description |
|---|---|---|
| Multi-file compilation | Large (1 week) | AXIOM library files (`lib/render/*.axm`) compiled to `.o` and linked with user `.axm`. Currently single-file only. |
| Include/import system | Large (1 week) | `import render.math;` resolves to `lib/render/math.axm`. |
| Generic functions | Large (2 weeks) | `fn create_buffer[T](data: ptr[T], count: i32)` -- currently parsed but no codegen. |

**Important:** The multi-file compilation is the biggest blocker. Without it, the AXIOM rendering library must be concatenated into a single file with user code, which works but is inelegant. The pragmatic approach: implement a simple file concatenation pre-pass in the driver (like C's `#include`) before tackling real modules.

---

## 9. File Layout

```
D:/ailang/
  axiom-gpu/                          # NEW: replaces axiom-renderer/
    Cargo.toml                        # depends on lux-core
    src/
      lib.rs                          # C ABI exports (~40 functions)
      context.rs                      # Wraps lux_core::VulkanContext
      resources.rs                    # Buffer/image/pipeline handle management
      commands.rs                     # Command buffer recording + draw list
      scene.rs                        # glTF scene handle management

  lib/                                # NEW: AXIOM standard rendering library
    render/
      math.axm                        # vec3, vec4, mat4, quaternion ops
      camera.axm                      # perspective, look_at, orbit camera
      culling.axm                     # frustum culling (@parallel_for)
      lights.axm                      # light management, shadow matrices
      sort.axm                        # draw call sorting (@strategy)
      batch.axm                       # draw call batching, instancing
      materials.axm                   # material parameter packing
      render_loop.axm                 # main render orchestration
      aabb.axm                        # bounding box math

  examples/
    pbr_sphere/                       # Phase 1 demo
      pbr_sphere.axm
    sponza_scene/                     # Phase 2 demo
      sponza_scene.axm
    compute_particles/                # Phase 4 demo
      compute_particles.axm

  lux/
    playground_rust/
      Cargo.toml                      # MODIFIED: adds [lib] target
      src/
        lib.rs                        # NEW: re-exports public modules
        main.rs                       # MODIFIED: uses lux_core:: imports
        (existing files unchanged)
```

---

## 10. How AXIOM's Unique Advantages Apply

### `@pure` -> fast-math on rendering math

Every matrix multiply, dot product, normalize, and AABB test in `lib/render/*.axm` is `@pure`. LLVM will:
- Use FMA instructions (fused multiply-add)
- Reorder operations for better pipelining
- Eliminate redundant computations via CSE
- Mark as `memory(none)` enabling hoisting out of loops

### `@parallel_for` -> parallel frustum culling

Testing 10,000 objects against 6 frustum planes is embarrassingly parallel. AXIOM's `@parallel_for` with the job system splits this across all CPU cores. The job system already uses dependency graphs, so shadow matrix computation can depend on camera update.

### `@strategy` -> AI-optimizable rendering decisions

```axiom
@strategy {
    render_path: ?mode,             // "forward" vs "deferred"
    shadow_resolution: ?shadow_res,  // 512, 1024, 2048, 4096
    cull_batch_size: ?cull_batch,    // 64..4096
    sort_algorithm: ?sort_algo,      // "radix", "merge", "insertion"
    max_lights_per_tile: ?tile_lights // 8..64
}
```

An LLM optimizer can benchmark different strategy combinations and find the optimal settings for a given scene complexity and GPU.

### `@vectorizable` -> SIMD AABB tests

AABB-vs-frustum tests operate on 6 float tuples -- perfect for 256-bit AVX operations. `@vectorizable(6)` hints LLVM to use `<6 x double>` vector operations.

### LLM optimization of the renderer itself

Because the rendering logic is AXIOM source, the `axiom optimize` command can:
1. Profile the render loop
2. Extract optimization surfaces (strategy holes)
3. Feed source + LLVM IR + timings to Claude
4. Get suggestions for parameter tuning and code restructuring
5. Apply and re-benchmark

This is impossible with Approach A (Rust is opaque) and the killer feature of this architecture.

---

## 11. GPGPU / Compute Shader Path

The hybrid approach naturally supports GPGPU:

```axiom
// AXIOM dispatches a Lux compute shader
let compute_pipe: i64 = gpu_create_pipeline(ctx, "shaders/compute_particles");
let positions_buf: i64 = gpu_create_buffer(ctx, 40000, 7);  // STORAGE usage
let velocities_buf: i64 = gpu_create_buffer(ctx, 40000, 7);

// Upload initial data from AXIOM arrays
gpu_upload_buffer(ctx, positions_buf, positions, 40000);

// Each frame: dispatch compute, then draw
gpu_cmd_bind_pipeline(ctx, compute_pipe);
gpu_cmd_dispatch_compute(ctx, 256, 1, 1);  // 256 workgroups
// ... barrier ...
gpu_cmd_bind_pipeline(ctx, render_pipe);
gpu_cmd_bind_vertex_buffer(ctx, positions_buf);
gpu_cmd_draw(ctx, 10000, 1);
```

The Lux compute shader (`compute_particles.lux`) does the GPU-side math. AXIOM controls _when_ and _how many_ workgroups to dispatch. Over time, AXIOM could also emit its own SPIR-V for simple compute kernels (future work).

---

## 12. Migration Path from Approach A to C

The phased approach means we can start with a minimal Approach-A-like surface (high-level calls) and progressively expose more granular control:

| Phase | AXIOM calls | Rendering control in AXIOM |
|---|---|---|
| Phase 1 | `gpu_init`, `gpu_load_gltf`, `gpu_render_scene` | ~5% (lifecycle only) |
| Phase 2 | + `gpu_cmd_*`, culling, sorting in AXIOM | ~40% (draw list, culling, sorting) |
| Phase 3 | + deferred orchestration, material management | ~60% (render path selection) |
| Phase 4 | + compute dispatch, GPU buffer management | ~70% (GPGPU) |
| Phase 5 | + RT dispatch, splat control | ~80% (full render control) |

At each phase, more rendering logic moves from Rust into AXIOM, and more of the renderer becomes optimizable by AI agents.

---

## 13. Risks and Mitigations

| Risk | Probability | Impact | Mitigation |
|---|---|---|---|
| Struct codegen bugs in AXIOM | Medium | High | Test struct layouts against C ABI early. Use `@align` annotations. |
| Multi-file compilation delays | Medium | Medium | Use file concatenation as stopgap. |
| Performance overhead of C ABI calls | Low | Medium | `gpu_cmd_*` calls are thin wrappers. Profile and inline-optimize. |
| Lux-core extraction breaks playground | Low | Low | Run Lux playground tests after extraction. |
| AXIOM render loop is slower than Rust | Low | Low | The CPU-side render loop is not the bottleneck -- GPU is. Even 2x slower CPU culling is negligible. |
| Feature creep in Layer 1 API | Medium | Medium | Keep API minimal. Add functions only when Layer 2 needs them. |

---

## 14. Success Criteria

1. **Phase 1 exit:** AXIOM program renders a PBR sphere with specular highlights and a directional light.
2. **Phase 2 exit:** AXIOM program loads Sponza, frustum-culls in parallel, renders with 3 shadow-casting lights.
3. **Phase 3 exit:** Deferred rendering path produces output matching Lux playground screenshots.
4. **Phase 4 exit:** AXIOM program dispatches a Lux compute shader and reads results.
5. **Phase 5 exit:** Ray-traced rendering from AXIOM.
6. **Overall:** `axiom optimize render_loop.axm` produces measurably better rendering performance through strategy tuning.

---

## 15. Conclusion

**Approach C (Hybrid Layered Architecture)** is the clear winner because it:

1. **Satisfies both user requirements** -- full PBR rendering (via Lux shaders + Rust GPU backend) AND Vulkan API control from AXIOM (via gpu_cmd_* draw list pattern)
2. **Maximizes AXIOM's unique value** -- `@pure`, `@strategy`, `@parallel_for`, and LLM optimization all apply to the rendering logic in Layer 2
3. **Is realistically implementable** -- Phase 1 delivers a PBR frame in 2 weeks by leveraging 26K LOC of existing Lux Rust code
4. **Enables GPGPU** -- `gpu_cmd_dispatch_compute` is a natural extension
5. **Uses Lux's ecosystem** -- all existing Lux shaders work unchanged
6. **Avoids the Vulkan struct nightmare** -- Rust handles all Vulkan struct setup; AXIOM works with simple handles and flat data
7. **Is incrementally adoptable** -- each phase adds more AXIOM control without breaking what works
