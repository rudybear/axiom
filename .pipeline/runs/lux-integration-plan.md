# Lux Vulkan Renderer Integration into axiom-renderer

## Status: SPECIFICATION
## Date: 2026-03-24

---

## 1. Executive Summary

Replace the wgpu-based renderer in `axiom-renderer` with Lux's ash-based Vulkan renderer, keeping the identical C ABI so AXIOM programs continue to work without changes. The current renderer (`axiom-renderer/src/renderer.rs`) uses wgpu for a simple 2D colored-vertex pipeline. The replacement uses ash (raw Vulkan), gpu-allocator, and Lux's SPIR-V reflection system.

---

## 2. Architectural Decision: Library Crate vs. File Copy

### Finding: Lux's playground is a binary crate only

`D:/ailang/lux/playground_rust/` has no `lib.rs`. All modules are declared in `main.rs`:
```
mod vulkan_context;
mod spv_loader;
pub mod reflected_pipeline;
mod raster_renderer;
mod scene;
// ... etc
```

There is no `[lib]` target in `D:/ailang/lux/playground_rust/Cargo.toml`.

### Decision: Extract a `lux-core` library crate

**Do NOT copy files.** Instead:

1. Create `D:/ailang/lux/playground_rust/src/lib.rs` that re-exports the modules we need.
2. Add a `[lib]` section to `D:/ailang/lux/playground_rust/Cargo.toml` alongside the existing `[[bin]]`.
3. Have `axiom-renderer` depend on `lux-core` via a path dependency.

This keeps a single source of truth for Vulkan context code.

### Concrete changes to `D:/ailang/lux/playground_rust/Cargo.toml`:

```toml
[package]
name = "lux-playground"
version = "0.1.0"
edition = "2021"

[lib]
name = "lux_core"
path = "src/lib.rs"

[[bin]]
name = "lux-playground"
path = "src/main.rs"
```

### New file `D:/ailang/lux/playground_rust/src/lib.rs`:

```rust
//! lux-core: reusable Vulkan infrastructure from the Lux playground.

pub mod vulkan_context;
pub mod spv_loader;
pub mod reflected_pipeline;
pub mod scene;
```

The `main.rs` module declarations for these four modules change from `mod` to `use lux_core::` imports. Other modules (`raster_renderer`, `rt_renderer`, etc.) stay private to the binary.

---

## 3. Minimal Subset of Lux Required

### Must use (from lux-core):

| Module | File | Why |
|---|---|---|
| `vulkan_context` | `vulkan_context.rs` (1206 lines) | Provides `VulkanContext` with instance, device, queues, command pool, gpu-allocator, swapchain, surface. Has both `new()` (headless) and `new_with_window()` (interactive) constructors. |
| `spv_loader` | `spv_loader.rs` (85 lines) | `load_spirv()`, `create_shader_module()`, `detect_stage()`. Reads `.spv` binary files, validates magic number, creates `VkShaderModule`. |
| `reflected_pipeline` | `reflected_pipeline.rs` (695 lines) | Deserializes `.lux.json` sidecar files, creates `VkDescriptorSetLayout`, `VkPipelineLayout`, vertex input state from reflection metadata. |
| `scene` | `scene.rs` (124 lines) | `PbrVertex`, `TriangleVertex` types, procedural geometry generators. Needed for vertex format definitions. |

### NOT needed for axiom-renderer:

- `raster_renderer.rs` -- too heavy (shadows, IBL, glTF material system); we write a simpler bridge
- `rt_renderer.rs`, `mesh_renderer.rs`, `deferred_renderer.rs`, `splat_renderer.rs` -- advanced render paths
- `camera.rs`, `gltf_loader.rs`, `scene_manager.rs`, `screenshot.rs`, `meshlet.rs` -- scene management

---

## 4. AXIOM C ABI: Functions to Preserve

From `D:/ailang/axiom-renderer/src/lib.rs`, these 8 `extern "C"` functions must keep identical signatures:

| Function | Signature |
|---|---|
| `axiom_renderer_create` | `(width: c_int, height: c_int, title: *const c_char) -> *mut c_void` |
| `axiom_renderer_destroy` | `(renderer: *mut c_void)` |
| `axiom_renderer_begin_frame` | `(renderer: *mut c_void) -> c_int` |
| `axiom_renderer_end_frame` | `(renderer: *mut c_void)` |
| `axiom_renderer_should_close` | `(renderer: *mut c_void) -> c_int` |
| `axiom_renderer_clear` | `(renderer: *mut c_void, color: c_uint)` |
| `axiom_renderer_draw_points` | `(renderer: *mut c_void, x_arr: *const c_double, y_arr: *const c_double, colors: *const c_uint, count: c_int)` |
| `axiom_renderer_draw_triangles` | `(renderer: *mut c_void, positions: *const c_float, colors_f: *const c_float, vertex_count: c_int)` |
| `axiom_renderer_get_time` | `(renderer: *mut c_void) -> c_double` |

**`lib.rs` stays unchanged.** All changes are in `renderer.rs`.

---

## 5. Shader Strategy

### Current state (wgpu)

The current renderer embeds a WGSL shader as a string constant (`SHADER_SRC` in `renderer.rs`). It defines a simple vertex shader that passes through 2D position + RGBA color.

### New state (ash/Vulkan)

We need equivalent SPIR-V shaders. Two options:

**Option A (recommended): Embed compiled SPIR-V as byte arrays**

Write a minimal GLSL shader pair:

`axiom_flat.vert`:
```glsl
#version 450
layout(location = 0) in vec2 inPosition;
layout(location = 1) in vec4 inColor;
layout(location = 0) out vec4 fragColor;
void main() {
    gl_Position = vec4(inPosition, 0.0, 1.0);
    fragColor = inColor;
}
```

`axiom_flat.frag`:
```glsl
#version 450
layout(location = 0) in vec4 fragColor;
layout(location = 0) out vec4 outColor;
void main() {
    outColor = fragColor;
}
```

Compile with `glslangValidator` to `.spv`, then embed as `const VERT_SPV: &[u8]` and `const FRAG_SPV: &[u8]` (same pattern as Lux's `SHADOW_VERT_SPV` in `raster_renderer.rs` line 46).

This avoids runtime file I/O and `.lux.json` reflection files for the basic AXIOM pipeline. The reflected_pipeline module is still available for future advanced use.

**Option B: Ship `.spv` + `.lux.json` files**

Use Lux's `spv_loader::load_spirv()` and `reflected_pipeline::load_reflection()` at runtime. Requires locating shader files relative to the DLL. Fragile for a cdylib.

**Decision: Option A.** Embed SPIR-V. Use `spv_loader::create_shader_module()` to create VkShaderModule from the embedded bytes at runtime.

---

## 6. Detailed Bridge Design: `renderer.rs` Rewrite

### 6.1 Struct layout

```rust
pub struct Renderer {
    // Lux Vulkan context (owns instance, device, queues, allocator, swapchain)
    ctx: lux_core::vulkan_context::VulkanContext,

    // Pipeline for 2D colored geometry
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    render_pass: vk::RenderPass,

    // Per-swapchain-image framebuffers
    framebuffers: Vec<vk::Framebuffer>,

    // Synchronization (double-buffered)
    image_available_semaphores: [vk::Semaphore; 2],
    render_finished_semaphores: [vk::Semaphore; 2],
    in_flight_fences: [vk::Fence; 2],
    current_frame: usize,

    // Command buffers (one per swapchain image)
    command_buffers: Vec<vk::CommandBuffer>,

    // Frame state
    draw_commands: Vec<DrawCommand>,
    should_close: bool,
    frame_count: u32,
    start_time: Instant,

    // Window + event loop (same winit pattern as current renderer)
    window: Arc<Window>,
    width: u32,
    height: u32,
}
```

### 6.2 Vertex format (unchanged from current)

```rust
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],  // NDC coordinates
    pub color: [f32; 4],     // RGBA
}
```

This maps to Vulkan vertex input:
- binding 0, stride 24, rate VERTEX
- location 0: R32G32_SFLOAT at offset 0 (position)
- location 1: R32G32B32A32_SFLOAT at offset 8 (color)

### 6.3 Initialization flow (`Renderer::new`)

1. Create winit window + event loop (same as current code)
2. Call `VulkanContext::new_with_window(&window, false, false)` -- no RT needed, no validation
3. Call `ctx.create_swapchain(width, height)` (already done inside `new_with_window`)
4. Create `VkRenderPass` with one color attachment matching `ctx.swapchain_format`
5. Create `VkPipelineLayout` (no descriptor sets, no push constants)
6. Create vertex/fragment shader modules from embedded SPIR-V via `spv_loader::create_shader_module()`
7. Create `VkPipeline` (graphics pipeline with vertex input matching `Vertex` layout, triangle list topology, no depth, alpha blending)
8. Create framebuffers for each swapchain image view
9. Allocate command buffers from `ctx.command_pool`
10. Create synchronization primitives (semaphores + fences)

### 6.4 Frame flow

**`begin_frame()`:**
1. Poll winit events (same pattern as current)
2. Clear draw command list
3. Return `!should_close`

**`draw_points()` / `draw_triangles()`:**
- Identical to current: convert pixel coords to NDC, push to `draw_commands`
- Points become 2-triangle quads (same as current)

**`end_frame()`:**
1. Wait for in-flight fence
2. `ctx.acquire_next_image(image_available_semaphore)` -- handle suboptimal/out-of-date by recreating swapchain
3. Collect all vertices from draw commands
4. Create a staging buffer via `gpu-allocator` (`MemoryLocation::CpuToGpu`)
5. Record command buffer:
   - Begin render pass (clear to `clear_color`, framebuffer for acquired image)
   - Bind pipeline
   - Bind vertex buffer
   - Draw triangle vertices
   - Draw point vertices
   - End render pass
6. Submit with wait on `image_available` and signal `render_finished`
7. `ctx.queue_present(image_index, render_finished_semaphore)`
8. Handle swapchain recreation on suboptimal/out-of-date

**`destroy()`:**
1. `device_wait_idle()`
2. Destroy framebuffers, pipeline, pipeline layout, render pass, shader modules
3. Destroy sync objects
4. Free vertex buffers via allocator
5. `ctx.destroy()` (cleans up device, instance, swapchain, surface, allocator)

### 6.5 Vertex buffer strategy

**Current (wgpu):** Creates a new `wgpu::Buffer` every frame via `create_buffer_init`. Simple but allocates every frame.

**New (ash):** Use a single persistent staging buffer sized to hold max expected vertices (e.g., 64K vertices = 1.5 MB). Map it, memcpy vertex data, unmap. If more vertices are needed, grow the buffer. This avoids per-frame allocation through gpu-allocator.

Implementation:
```rust
struct DynamicVertexBuffer {
    buffer: vk::Buffer,
    allocation: gpu_allocator::vulkan::Allocation,
    capacity: usize,      // in bytes
    mapped_ptr: *mut u8,  // persistently mapped (CpuToGpu allows this)
}
```

Create with `AllocationCreateDesc { location: MemoryLocation::CpuToGpu, .. }`. The buffer is `HOST_VISIBLE | HOST_COHERENT`, so no explicit flush needed.

---

## 7. Dependency Changes

### `D:/ailang/axiom-renderer/Cargo.toml` (new):

```toml
[package]
name = "axiom-renderer"
version = "0.1.0"
edition = "2021"
description = "AXIOM GPU renderer -- ash/Vulkan cdylib exposing C ABI for AXIOM programs"

[lib]
crate-type = ["cdylib"]

[dependencies]
lux_core = { path = "../lux/playground_rust" }
ash = "0.38"
gpu-allocator = { version = "0.28", features = ["vulkan"] }
winit = "0.30"
raw-window-handle = "0.6"
bytemuck = { version = "1", features = ["derive"] }
log = "0.4"
env_logger = "0.11"
```

**Removed:** `wgpu`, `pollster`
**Added:** `lux_core` (path dep), `ash`, `gpu-allocator`, `log`, `env_logger`

---

## 8. File-by-File Change List

| File | Action | Description |
|---|---|---|
| `lux/playground_rust/Cargo.toml` | MODIFY | Add `[lib]` section for `lux_core` |
| `lux/playground_rust/src/lib.rs` | CREATE | New file: `pub mod vulkan_context; pub mod spv_loader; pub mod reflected_pipeline; pub mod scene;` |
| `lux/playground_rust/src/main.rs` | MODIFY | Change `mod vulkan_context` etc. to `use lux_core::vulkan_context` for the 4 shared modules |
| `axiom-renderer/Cargo.toml` | REWRITE | Replace wgpu deps with ash + lux_core path dep |
| `axiom-renderer/src/lib.rs` | NO CHANGE | C ABI exports stay identical |
| `axiom-renderer/src/renderer.rs` | REWRITE | Replace wgpu renderer with ash-based renderer using `lux_core::vulkan_context::VulkanContext` |
| `axiom-renderer/src/shaders/axiom_flat.vert` | CREATE | GLSL vertex shader source (for reference/recompilation) |
| `axiom-renderer/src/shaders/axiom_flat.frag` | CREATE | GLSL fragment shader source (for reference/recompilation) |
| `axiom-renderer/src/embedded_shaders.rs` | CREATE | `const VERT_SPV: &[u8]` and `const FRAG_SPV: &[u8]` embedded SPIR-V |

---

## 9. Swapchain Resize Handling

The current wgpu renderer handles resize in `poll_events()` by reconfiguring the surface. The new renderer must:

1. Detect `WindowEvent::Resized` in the poll loop
2. Call `ctx.create_swapchain(new_width, new_height)` which handles old swapchain teardown
3. Recreate framebuffers for the new swapchain image views
4. Update `self.width` / `self.height`

Lux's `VulkanContext::create_swapchain()` already handles old-swapchain cleanup (lines 898-1025 of `vulkan_context.rs`), including destroying old image views and passing `old_swapchain` to the create info.

---

## 10. Key API Mappings

| AXIOM concept | wgpu (current) | ash/Lux (new) |
|---|---|---|
| Window creation | `EventLoop` + winit | Same `EventLoop` + winit |
| GPU init | `wgpu::Instance` + adapter + device | `VulkanContext::new_with_window()` |
| Surface | `wgpu::Surface` | `VulkanContext.surface` (VkSurfaceKHR) |
| Swapchain | `wgpu::SurfaceConfiguration` | `VulkanContext.swapchain` + `create_swapchain()` |
| Shader | `wgpu::ShaderModule` from WGSL | `spv_loader::create_shader_module()` from SPIR-V bytes |
| Pipeline | `wgpu::RenderPipeline` | `vk::Pipeline` (graphics) |
| Vertex buffer | `wgpu::Buffer` (created per frame) | `vk::Buffer` + `gpu_allocator::Allocation` (persistent, mapped) |
| Command recording | `wgpu::CommandEncoder` | `vk::CommandBuffer` from `ctx.command_pool` |
| Submit + present | `queue.submit()` + `output.present()` | `vkQueueSubmit` + `ctx.queue_present()` |
| Frame sync | Implicit (wgpu handles it) | Explicit semaphores + fences (double-buffered) |

---

## 11. Risk Analysis

| Risk | Mitigation |
|---|---|
| Lux's `VulkanContext` has features AXIOM doesn't need (RT, mesh shaders, bindless) | These are optional -- `new_with_window(window, false, false)` disables RT. Extra fields are just `None`/`false`. No performance cost. |
| gpu-allocator version conflicts between lux_core and axiom-renderer | Both use the same path dep, so Cargo resolves to one version. |
| Embedded SPIR-V must be compiled separately | Include pre-compiled bytes. Add a build script or Makefile target to regenerate from GLSL if needed. |
| Swapchain recreation is more complex than wgpu's `surface.configure()` | Lux's `create_swapchain()` handles it correctly already. Wrap in a `recreate_swapchain_resources()` helper that also rebuilds framebuffers. |
| Per-frame vertex buffer allocation via gpu-allocator would be slow | Use a persistent mapped buffer (CpuToGpu) with dynamic capacity. |
| winit event loop thread-local storage pattern from current renderer | Keep the same pattern. `VulkanContext` doesn't own the event loop. |

---

## 12. Implementation Order

### Phase 1: Extract lux-core (30 min)
1. Add `[lib]` to lux playground Cargo.toml
2. Create `lib.rs` with 4 `pub mod` declarations
3. Update `main.rs` to use `lux_core::` imports for shared modules
4. Verify `cargo build` for both lib and bin targets

### Phase 2: Write embedded shaders (15 min)
1. Write `axiom_flat.vert` and `axiom_flat.frag` in GLSL
2. Compile to SPIR-V with `glslangValidator -V`
3. Embed as byte arrays in `embedded_shaders.rs`

### Phase 3: Rewrite renderer.rs (2 hours)
1. Replace struct definition (remove wgpu types, add ash/Vulkan types)
2. Implement `Renderer::new()` using `VulkanContext::new_with_window()`
3. Implement render pass + pipeline creation
4. Implement dynamic vertex buffer
5. Implement `begin_frame()` / `end_frame()` with swapchain acquire/present
6. Implement `draw_points()` / `draw_triangles()` (logic unchanged, just different buffer upload)
7. Implement `destroy()` with correct teardown order

### Phase 4: Update Cargo.toml + build (15 min)
1. Replace dependencies
2. Build and fix any compilation errors

### Phase 5: Test (30 min)
1. Run existing AXIOM test programs
2. Verify window creation, clear, draw_points, draw_triangles, resize, close
3. Verify DLL exports match expected C ABI

---

## 13. SPIR-V Format Details

Lux's `spv_loader::load_spirv()` (line 13) reads `.spv` files as raw binary:
- Validates the SPIR-V magic number `0x07230203` (little-endian)
- File size must be a multiple of 4 bytes
- Returns `Vec<u32>` (word array)

`spv_loader::create_shader_module()` (line 45) takes `&[u32]` and creates a `VkShaderModule` via `vkCreateShaderModule`.

For embedded shaders, we skip `load_spirv()` and call `create_shader_module()` directly with the embedded `&[u32]` slice (reinterpreted from `&[u8]`).

---

## 14. Compatibility Notes

- Lux targets Vulkan 1.2+ (`api_version: vk::make_api_version(0, 1, 2, 0)`)
- The current wgpu renderer requests `VULKAN | DX12` backends; the new renderer is Vulkan-only
- This means the axiom-renderer will no longer work on systems without Vulkan 1.2 drivers
- On Windows 11 with modern GPUs this is not a concern
