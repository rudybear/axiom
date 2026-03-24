# Vulkan Integration Plan for AXIOM + Lux

## Research Date: 2026-03-23

---

## 1. Lux Repository Analysis (github.com/rudybear/lux)

### 1.1 Repository Structure

```
lux/
  luxc/               # Python compiler (Lark parser -> AST -> SPIR-V)
  examples/           # 81 .lux shader files (triangle, PBR, compute, RT, mesh, splat)
  playground_rust/    # Rust/ash Vulkan renderer
  playground_cpp/     # C++/Vulkan + Metal renderer (60 source files)
  playground_web/     # WebGPU/TypeScript/Vite renderer
  playground/         # Python/wgpu renderer (screenshot tests)
  tests/              # 1424+ compiler tests
  docs/               # Language reference
  shaders/            # Utility shaders (radix sort)
  assets/             # glTF models, HDR environments (LFS)
  projects/nadrin-pbr/# PBR validation project
```

### 1.2 Rust Renderer (playground_rust/)

**Cargo.toml dependencies:**
```toml
ash = "0.38"              # Vulkan bindings (thin, unsafe, 1:1 mapping)
ash-window = "0.13"       # Window surface creation
gpu-allocator = "0.28"    # GPU memory allocation (VMA-like)
winit = "0.30"            # Cross-platform windowing
raw-window-handle = "0.6" # Window handle abstraction
glam = "0.29"             # SIMD math (vec3, mat4, etc.)
image = "0.25"            # Image loading/saving
bytemuck = "1"            # Safe transmute for GPU buffers
clap = "4"                # CLI argument parsing
serde = "1"               # JSON deserialization
serde_json = "1"          # .lux.json reflection parsing
gltf = "1"                # glTF model loading (many KHR extensions)
log = "0.4"               # Logging facade
env_logger = "0.11"       # Log output
```

**Source files (15 modules):**
```
src/
  main.rs               # Entry point, CLI, render loop, renderer dispatch
  vulkan_context.rs     # Instance, device, swapchain, sync, command pools
  spv_loader.rs         # SPIR-V file loading + VkShaderModule creation
  reflected_pipeline.rs # JSON reflection -> descriptor layouts -> pipeline layout
  raster_renderer.rs    # Forward rasterization pipeline (PBR, permutations)
  deferred_renderer.rs  # G-buffer + lighting pass deferred rendering
  rt_renderer.rs        # VK_KHR_ray_tracing_pipeline ray tracing
  mesh_renderer.rs      # VK_EXT_mesh_shader mesh shading
  splat_renderer.rs     # Gaussian splatting renderer
  gltf_loader.rs        # glTF scene loading
  scene.rs              # Scene data structures
  scene_manager.rs      # Scene management + IBL loading
  camera.rs             # Orbit camera for interactive mode
  meshlet.rs            # Meshlet generation for mesh shaders
  screenshot.rs         # Headless rendering to PNG
```

### 1.3 How SPIR-V is Loaded

From `spv_loader.rs` (complete, ~80 lines):

```rust
pub fn load_spirv(path: &Path) -> Result<Vec<u32>, String> {
    let bytes = fs::read(path)?;
    // Validate: size >= 4, size % 4 == 0, magic == 0x07230203
    let words: Vec<u32> = bytes.chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    Ok(words)
}

pub fn create_shader_module(device: &ash::Device, code: &[u32]) -> Result<vk::ShaderModule, String> {
    let create_info = vk::ShaderModuleCreateInfo::default().code(code);
    unsafe { device.create_shader_module(&create_info, None) }
}

pub fn detect_stage(filename: &str) -> Result<vk::ShaderStageFlags, String> {
    // *.vert.spv -> VERTEX, *.frag.spv -> FRAGMENT, *.comp.spv -> COMPUTE
    // *.rgen.spv -> RAYGEN_KHR, *.rchit.spv -> CLOSEST_HIT_KHR, *.rmiss.spv -> MISS_KHR
}
```

**Key insight:** SPIR-V loading is trivial. The hard part is reflection-driven pipeline creation.

### 1.4 How Pipelines are Created

The system is **reflection-driven**. Lux compiler outputs:
1. `shader.vert.spv` + `shader.frag.spv` (binary SPIR-V)
2. `shader.lux.json` (reflection metadata)

The `.lux.json` contains:
- Descriptor set layouts (set number, binding number, type, stage flags)
- Push constant ranges (size, stage flags, field layout)
- Vertex attributes (location, format, offset)
- Binding types ("uniform_buffer", "sampled_image", "storage_buffer", etc.)
- Bindless array configuration (max_count, variable descriptor count)

`reflected_pipeline.rs` parses this JSON and builds:
1. `VkDescriptorSetLayout` from merged vertex+fragment bindings
2. `VkPipelineLayout` from descriptor set layouts + push constant ranges
3. `VkVertexInputBindingDescription` + `VkVertexInputAttributeDescription` from vertex attributes
4. Permutation pipelines via `ShaderManifest` (feature flags -> compiled variants)

The `raster_renderer.rs` then creates `VkGraphicsPipeline` with:
- Shader stages (from loaded SPIR-V modules)
- Vertex input state (from reflection)
- Pipeline layout (from reflection)
- Render pass compatibility
- Rasterization, multisample, depth/stencil, color blend states

### 1.5 No C ABI / FFI Exports

The Lux project does **not** expose a C API. All renderers are standalone applications that:
1. Parse CLI args to select a shader
2. Run `luxc` compiler to produce .spv + .lux.json
3. Load those files and create Vulkan pipelines from reflection data
4. Render to window or screenshot

Integration with AXIOM requires building our own Vulkan host layer.

---

## 2. Example Lux Shaders

### hello_triangle.lux (23 lines, zero boilerplate)
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

### Declarative PBR Pipeline
```lux
pipeline PBRForward {
    geometry: StandardMesh,
    surface: CopperMetal,
}
```
Expands to linked vertex + fragment stages automatically.

### Output file convention
```
hello_triangle.vert.spv    # Vertex SPIR-V
hello_triangle.frag.spv    # Fragment SPIR-V
hello_triangle.lux.json    # Reflection metadata
```

---

## 3. Minimal Vulkan Requirements

### 3.1 Absolute Minimum for a Triangle (from research)

A minimal Vulkan triangle in C requires ~500-1000 lines covering:

**Initialization (one-time):**
1. `vkCreateInstance()` with surface extensions
2. `vkEnumeratePhysicalDevices()` + select discrete GPU
3. `vkCreateDevice()` with graphics queue family
4. `vkGetDeviceQueue()`
5. `vkCreateWin32SurfaceKHR()` (platform-specific)
6. `vkCreateSwapchainKHR()` + `vkGetSwapchainImagesKHR()`
7. `vkCreateImageView()` per swapchain image
8. `vkCreateRenderPass()` with color attachment
9. `vkCreateFramebuffer()` per swapchain image
10. `vkCreateShaderModule()` x2 (vertex + fragment SPIR-V)
11. `vkCreateDescriptorSetLayout()` (empty for triangle)
12. `vkCreatePipelineLayout()`
13. `vkCreateGraphicsPipelines()` with all state
14. `vkCreateCommandPool()` + `vkAllocateCommandBuffers()`
15. `vkCreateSemaphore()` x2 + `vkCreateFence()`

**Per-frame render loop:**
1. `vkWaitForFences()` (wait for previous frame)
2. `vkAcquireNextImageKHR()` (get swapchain image)
3. `vkBeginCommandBuffer()`
4. `vkCmdBeginRenderPass()`
5. `vkCmdBindPipeline()`
6. `vkCmdDraw(3, 1, 0, 0)` (3 vertices, 1 instance)
7. `vkCmdEndRenderPass()`
8. `vkEndCommandBuffer()`
9. `vkQueueSubmit()` with semaphores
10. `vkQueuePresentKHR()`

### 3.2 Key Libraries for C Integration

| Library | Purpose | Files | License |
|---------|---------|-------|---------|
| **volk** (zeux/volk) | Vulkan function loader, replaces vulkan-1.dll linking | volk.h + volk.c | MIT |
| **SPIRV-Reflect** (Khronos) | Parse SPIR-V for descriptor layouts | spirv_reflect.h + spirv_reflect.c | Apache 2.0 |
| **VMA** (GPUOpen) | GPU memory allocator | vk_mem_alloc.h | MIT |
| **SDL2/GLFW** | Windowing + surface creation | System library | Various |

**volk** is particularly valuable: it dynamically loads Vulkan without linking vulkan-1.dll, supports C89, and loads device functions directly from the driver for minimal dispatch overhead.

**SPIRV-Reflect** is the C equivalent of what Lux's `reflected_pipeline.rs` does: it parses SPIR-V bytecode to extract descriptor set layouts and push constant ranges, then populates `VkDescriptorSetLayoutCreateInfo` structs automatically.

---

## 4. Integration Strategies (Ranked by Speed-to-Pixels)

### Strategy A: Rust Renderer Library via C ABI (RECOMMENDED)

**Approach:** Build a thin Rust library wrapping Lux's playground_rust patterns, exposing a C ABI that AXIOM calls via `extern fn`.

**Why this is fastest:**
- Lux's Rust renderer already handles all Vulkan complexity
- ash + gpu-allocator + winit is battle-tested
- Reflection-driven pipeline creation from .lux.json is already implemented
- We only need to wrap it with `#[no_mangle] pub extern "C" fn` exports

**C ABI surface (what AXIOM sees):**
```c
// Lifecycle
void* renderer_create(int width, int height, const char* title);
void  renderer_destroy(void* ctx);

// Shader loading (from Lux compiler output)
void* renderer_load_pipeline(void* ctx, const char* shader_base_path);
void  renderer_destroy_pipeline(void* ctx, void* pipeline);

// Per-frame rendering
int   renderer_begin_frame(void* ctx);
void  renderer_set_camera(void* ctx, const float* view_4x4, const float* proj_4x4);
void  renderer_draw_mesh(void* ctx, void* pipeline, const float* vertices, int vertex_count,
                         const uint32_t* indices, int index_count);
void  renderer_end_frame(void* ctx);

// Utility
int   renderer_should_close(void* ctx);
void  renderer_resize(void* ctx, int width, int height);
```

**Implementation steps:**
1. Create `crates/axiom-renderer/` as a new workspace member
2. Depend on ash, ash-window, gpu-allocator, winit, serde_json, glam, bytemuck
3. Port the core patterns from Lux's playground_rust:
   - `VulkanContext` (instance, device, swapchain)
   - `spv_loader` (SPIR-V loading, shader module creation)
   - `reflected_pipeline` (JSON reflection -> pipeline layout)
   - Simplified raster renderer (forward only initially)
4. Add `#[no_mangle] pub extern "C" fn` wrappers
5. Build as `cdylib` (produces `axiom_renderer.dll` / `.so`)
6. AXIOM codegen emits `extern fn` calls to this library

**Effort estimate:** 2-3 days for triangle, 1 week for PBR with reflection.

### Strategy B: Pure C Renderer with volk + SPIRV-Reflect

**Approach:** Write a minimal C Vulkan renderer that AXIOM links directly, using volk for function loading and SPIRV-Reflect for pipeline creation from Lux SPIR-V.

**Advantages:**
- No Rust dependency at runtime
- Complete control over memory allocation
- AXIOM's LLVM codegen can link statically
- Smallest possible binary

**Disadvantages:**
- Must rewrite all Vulkan boilerplate in C (~1000+ lines for triangle)
- Must rewrite reflection-driven pipeline creation from scratch
- No gpu-allocator equivalent (must use VMA or manual allocation)
- Significantly more work than Strategy A

**Key files needed:**
```
renderer/
  volk.h, volk.c              # Vulkan loader (from zeux/volk)
  spirv_reflect.h, .c         # SPIR-V reflection (from Khronos)
  vk_mem_alloc.h              # GPU memory allocator (from GPUOpen)
  axiom_renderer.h            # Public C API
  axiom_renderer.c            # Implementation
  axiom_pipeline.c            # Reflection-driven pipeline creation
```

**Effort estimate:** 1-2 weeks for triangle, 3-4 weeks for PBR.

### Strategy C: Use Lux's C++ Renderer Directly

**Approach:** Build lux/playground_cpp as a shared library, expose C API from its existing C++ code.

**Advantages:**
- Already feature-complete (PBR, deferred, RT, mesh shaders, splat)
- 60 source files of production-quality Vulkan code
- Metal support included

**Disadvantages:**
- Heavy C++ dependency (STL, templates, etc.)
- Complex CMake build integration
- Not designed as a library (it's a standalone app)
- Harder to maintain as a fork

**Effort estimate:** 1-2 weeks to extract library, ongoing maintenance burden.

---

## 5. Recommended Architecture

### Phase 1: Triangle in a Window (M7.6a)

```
AXIOM source (.axm)          Lux source (.lux)
       |                            |
       v                            v
  AXIOM Compiler               Lux Compiler (luxc)
       |                            |
       v                            v
  Native binary (.exe)         .vert.spv + .frag.spv + .lux.json
       |                            |
       +--- extern fn calls --------+
       |                            |
       v                            v
  axiom-renderer.dll (Rust cdylib)
       |
       v
  ash -> Vulkan -> GPU -> pixels on screen
```

**Step-by-step:**
1. Write `hello_triangle.lux` (already exists in Lux examples)
2. Compile with `luxc` -> `hello_triangle.vert.spv` + `hello_triangle.frag.spv` + `.lux.json`
3. Build `axiom-renderer` crate:
   - `VulkanContext::new_with_window()` (from Lux patterns)
   - `spv_loader::load_spirv()` + `create_shader_module()`
   - Hardcoded triangle pipeline (skip reflection for v1)
   - `begin_frame()` / `draw()` / `end_frame()` loop
4. Expose via `extern "C"` functions
5. Write AXIOM test program:
```axiom
extern fn renderer_create(w: i32, h: i32, title: ptr[i8]) -> ptr[i8];
extern fn renderer_begin_frame(ctx: ptr[i8]) -> i32;
extern fn renderer_draw_triangle(ctx: ptr[i8]);
extern fn renderer_end_frame(ctx: ptr[i8]);
extern fn renderer_should_close(ctx: ptr[i8]) -> i32;

fn main() -> i32 {
    let r: ptr[i8] = renderer_create(800, 600, "AXIOM Triangle");
    while not renderer_should_close(r) {
        renderer_begin_frame(r);
        renderer_draw_triangle(r);
        renderer_end_frame(r);
    }
    return 0;
}
```

### Phase 2: Reflection-Driven Pipelines (M7.6b)

Replace hardcoded triangle pipeline with:
1. Parse `.lux.json` reflection metadata (already implemented in Lux's `reflected_pipeline.rs`)
2. Auto-create descriptor set layouts from reflection
3. Auto-create pipeline layout from reflection
4. Load arbitrary Lux shaders by path
5. Expose `renderer_load_pipeline(ctx, "shadercache/pbr_basic")` to AXIOM

### Phase 3: Scene Rendering (M7.6c)

1. Vertex buffer upload from AXIOM arrays
2. Index buffer upload
3. Uniform buffer updates (camera, transforms)
4. Push constants for per-draw data
5. Texture loading (from AXIOM file I/O)
6. glTF scene loading (via gltf crate in renderer)

---

## 6. Vulkan-AXIOM Type Mapping

| AXIOM Type | Vulkan/GPU Type | Lux Type | SPIR-V Type |
|------------|-----------------|----------|-------------|
| `f32` | `float` | `scalar` | `OpTypeFloat 32` |
| `i32` | `int` | `int` | `OpTypeInt 32 1` |
| `u32` | `uint` | `uint` | `OpTypeInt 32 0` |
| `array[f32, 3]` | `vec3` | `vec3` | `OpTypeVector %float 3` |
| `array[f32, 4]` | `vec4` | `vec4` | `OpTypeVector %float 4` |
| `array[f32, 16]` | `mat4` | `mat4` | `OpTypeMatrix %vec4 4` |
| `ptr[f32]` | buffer pointer | buffer ref | `OpTypePointer StorageBuffer` |

---

## 7. Critical Dependencies to Add

### For axiom-renderer crate (Cargo.toml):
```toml
[package]
name = "axiom-renderer"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]  # DLL for AXIOM + rlib for Rust tests

[dependencies]
ash = "0.38"
ash-window = "0.13"
gpu-allocator = { version = "0.28", features = ["vulkan"] }
winit = "0.30"
raw-window-handle = "0.6"
glam = "0.29"
bytemuck = { version = "1", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
log = "0.4"
env_logger = "0.11"
```

### For AXIOM compiler (linking):
- The compiled AXIOM binary links against `axiom_renderer.dll` at runtime
- AXIOM codegen emits `call @renderer_create(...)` etc. as external function calls
- The DLL must be in the same directory or PATH at runtime

---

## 8. Lux Compiler Integration

### Running luxc from AXIOM build:
```bash
# Compile a Lux shader to SPIR-V
python -m luxc examples/hello_triangle.lux -o shadercache/hello_triangle

# Output files:
#   shadercache/hello_triangle.vert.spv
#   shadercache/hello_triangle.frag.spv
#   shadercache/hello_triangle.lux.json

# With validation:
python -m luxc examples/hello_triangle.lux -o shadercache/hello_triangle --validate
```

### Build-time shader compilation:
1. AXIOM build script detects `.lux` files in project
2. Invokes `luxc` to compile to SPIR-V
3. Embeds or copies `.spv` + `.lux.json` to output directory
4. Renderer loads them at runtime

### Hot reload (development):
```bash
python -m luxc examples/hello_triangle.lux -o shadercache/hello_triangle --watch
```
The renderer can watch for file changes and recreate pipelines.

---

## 9. Risk Assessment

| Risk | Mitigation |
|------|------------|
| Vulkan driver not available | Check at startup, provide clear error message |
| ash crate version mismatch | Pin exact versions in Cargo.toml |
| SPIR-V validation failures | Run `spirv-val` in debug builds, Lux compiler validates |
| Swapchain recreation on resize | Already handled in Lux's VulkanContext pattern |
| GPU memory exhaustion | gpu-allocator handles fragmentation; arena reset per frame |
| Windows-only initially | ash + winit are cross-platform; Metal path via Lux's C++ renderer |
| luxc Python dependency | Bundle Python or use compiled luxc; long-term: port luxc to Rust |

---

## 10. Action Items (Priority Order)

1. **Create `crates/axiom-renderer/`** with Cargo.toml above
2. **Port `VulkanContext`** from Lux playground_rust (instance, device, swapchain, sync)
3. **Port `spv_loader`** (trivial, ~80 lines)
4. **Write hardcoded triangle pipeline** (skip reflection for v1)
5. **Add `extern "C"` wrapper functions** for AXIOM FFI
6. **Write AXIOM test program** using `extern fn` declarations
7. **Test: triangle.axm -> triangle.exe -> colored triangle on screen**
8. **Port `reflected_pipeline`** for automatic pipeline creation from .lux.json
9. **Add vertex/index buffer upload** from AXIOM arrays
10. **Add uniform buffer updates** for camera/transform matrices

---

## Sources

- [Lux Repository](https://github.com/rudybear/lux) - Shader language + renderers
- [ash-rs/ash](https://github.com/ash-rs/ash) - Vulkan bindings for Rust
- [zeux/volk](https://github.com/zeux/volk) - Meta-loader for Vulkan API (C89, MIT)
- [KhronosGroup/SPIRV-Reflect](https://github.com/KhronosGroup/SPIRV-Reflect) - C/C++ reflection API for SPIR-V
- [Vulkan Tutorial](https://vulkan-tutorial.com/) - Comprehensive Vulkan tutorial
- [Vulkan in 30 Minutes](https://renderdoc.org/vulkan-in-30-minutes.html) - RenderDoc's condensed Vulkan overview
- [Sopyer Minimal Vulkan Sample](https://sopyer.github.io/Blog/post/minimal-vulkan-sample/) - ~500 line C clear-screen
- [elecro/vkdemos](https://github.com/elecro/vkdemos) - Single-file minimal Vulkan demos (MIT)
- [SaschaWillems/Vulkan](https://github.com/SaschaWillems/Vulkan) - C++ Vulkan examples
- [adrien-ben/vulkan-tutorial-rs](https://github.com/adrien-ben/vulkan-tutorial-rs) - Vulkan tutorial in Rust using Ash
- [Vulkan Documentation - SPIR-V](https://docs.vulkan.org/guide/latest/ways_to_provide_spirv.html)
- [Vulkan Documentation - Pipelines](https://docs.vulkan.org/spec/latest/chapters/pipelines.html)
