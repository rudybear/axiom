# Rendering Plan — Real Vulkan, Not Stubs

## Current State (Honest Assessment)

| Component | Status |
|-----------|--------|
| Win32 window creation | **REAL** — opens a window |
| Pixel framebuffer + GDI blit | **REAL** — software rasterization works |
| Software point/triangle raster | **REAL** — basic but functional |
| Vulkan instance/device/swapchain | **NOT IMPLEMENTED** |
| GPU memory (VMA) | **NOT IMPLEMENTED** |
| SPIR-V shader loading | **STUB** — returns dummy handle |
| Pipeline creation | **STUB** — no-op |
| Command buffers | **NOT IMPLEMENTED** |
| GPU synchronization | **NOT IMPLEMENTED** |
| Lux shader integration | **NOT IMPLEMENTED** |

## Architecture Decision: How to Add Real Vulkan

### Option A: Pure C in axiom_rt.c (Direct Vulkan)
- PRO: No extra dependencies beyond Vulkan SDK
- PRO: Single-file, compiles with clang
- CON: Vulkan in C is ~1500 lines minimum for a triangle
- CON: No memory allocator (need VMA or manual)
- CON: Painful to maintain

### Option B: Rust crate with C ABI exports (Recommended)
- PRO: Use `ash` (thin Vulkan bindings) + `gpu-allocator` + `winit`
- PRO: Same stack as Lux's Rust renderer
- PRO: Memory-safe Vulkan wrapper
- PRO: Can reuse patterns from Lux's `playground_rust/`
- CON: Adds Rust crate compilation step to the build
- CON: Need to ship .dll/.lib alongside compiled AXIOM programs

### Option C: Link against existing Vulkan renderer library
- PRO: Use an existing C library like `sokol_gfx` or `bgfx`
- CON: Another dependency to manage
- CON: Abstracts away Vulkan specifics

**Recommendation: Option B** — Rust crate. Matches Lux's stack, safe, maintainable.

## Phased Implementation Plan

### Phase R1: Vulkan Bootstrap (Window + Triangle)
**Goal:** Open a window, clear to a color, render ONE hardcoded triangle.

Tasks:
1. Create `axiom-renderer/` Rust crate with `ash`, `winit`, `gpu-allocator`
2. Implement Vulkan instance + physical device + logical device
3. Implement swapchain creation + recreation on resize
4. Implement render pass + framebuffers
5. Implement command pool + command buffers
6. Implement synchronization (fences + semaphores for frames-in-flight)
7. Hardcode a triangle vertex buffer + simple vert/frag shaders
8. Export C ABI: `axiom_renderer_create`, `_begin_frame`, `_end_frame`, `_destroy`
9. Build as `cdylib` → `axiom_renderer.dll`
10. Update AXIOM's `compile.rs` to link the renderer .dll

**Deliverable:** `axiom compile triangle.axm -o triangle.exe && ./triangle.exe` opens a window with a colored triangle rendered by the GPU.

### Phase R2: Vertex Data Pipeline
**Goal:** AXIOM arrays → GPU vertex buffers → rendered geometry.

Tasks:
1. Implement `renderer_upload_vertices(positions, colors, count)` → creates VkBuffer
2. Implement staging buffer + transfer queue for GPU upload
3. Implement `renderer_draw(vertex_buffer, count)` with real draw calls
4. Test: particle galaxy with GPU-rendered points

**Deliverable:** Particle galaxy runs on GPU instead of software rasterizer.

### Phase R3: Lux Shader Integration
**Goal:** Load Lux-compiled SPIR-V shaders, create real pipelines.

Tasks:
1. Implement `shader_load(path)` → reads .spv, creates VkShaderModule
2. Implement `pipeline_create(vert, frag)` → real VkPipeline with vertex input
3. Parse Lux's `.lux.json` reflection for auto descriptor set layout
4. Test: render triangle with a Lux-compiled fragment shader

**Deliverable:** `lux compile shader.lux` → `shader.frag.spv` → loaded by AXIOM → GPU rendering.

### Phase R4: Descriptor Sets + Uniforms
**Goal:** Pass data from AXIOM to shaders (MVP matrix, time, etc.)

Tasks:
1. Implement uniform buffer creation + update
2. Implement descriptor set layout + allocation + writing
3. Implement `renderer_set_uniform(name, data, size)`
4. Test: rotating triangle with MVP matrix from AXIOM

### Phase R5: Production Renderer
**Goal:** Full-featured renderer for the particle galaxy demo.

Tasks:
1. Instanced rendering (10K particles in one draw call)
2. Depth buffer
3. Compute shaders (for GPU physics)
4. ImGui integration (optional, for debug UI)

## Prerequisites

- **Vulkan SDK** installed (vulkan-1.lib, validation layers)
- **Rust** (for building the renderer crate)
- **Lux compiler** (for compiling .lux → .spv shaders)

## Estimated Effort

| Phase | Effort | Lines of Rust |
|-------|--------|---------------|
| R1: Bootstrap | 3-5 days | ~1500 |
| R2: Vertex pipeline | 2-3 days | ~500 |
| R3: Lux shaders | 2-3 days | ~400 |
| R4: Uniforms | 1-2 days | ~300 |
| R5: Production | 3-5 days | ~800 |
| **Total** | **~2-3 weeks** | **~3500** |

## Alternative: Quick Win with wgpu

If Vulkan SDK setup is too heavy, `wgpu` provides the same capability with less boilerplate:
- Automatically selects Vulkan/DX12/Metal
- ~500 lines for a triangle (vs ~1500 for raw Vulkan)
- Can still load SPIR-V shaders (Lux output)
- Used by Bevy game engine in production

The tradeoff: less control, slightly higher overhead, but 3x faster to implement.
