# Vulkan Rendering + Lux Shader Integration

This directory contains examples demonstrating AXIOM's rendering API with
Lux shader loading infrastructure.

## Architecture

AXIOM does **not** expose raw Vulkan to user code. Instead, the runtime
(`axiom_rt.c`) provides a thin C wrapper with ~10 functions that hide the
complexity of Vulkan setup (instance, device, swapchain, command buffers,
synchronization). AXIOM programs call these builtins directly.

### Data flow

```
AXIOM source (.axm)
  |
  v
AXIOM compiler --> LLVM IR --> native executable
  |                               |
  | calls renderer builtins       | links axiom_rt.c
  v                               v
axiom_renderer_create()     axiom_renderer_draw_triangles()
axiom_shader_load()         axiom_renderer_end_frame()
  |                               |
  | loads SPIR-V                  | submits draw commands
  v                               v
Lux shader (.lux)           Vulkan GPU pipeline
  |
  | compiled by luxc
  v
SPIR-V bytecode (.spv)
```

### How AXIOM calls the renderer API

AXIOM programs use built-in functions that map 1:1 to C functions in the
runtime:

| AXIOM builtin                          | C function in axiom_rt.c                    |
|----------------------------------------|---------------------------------------------|
| `renderer_create(w, h, title)`         | `axiom_renderer_create(int, int, char*)`    |
| `renderer_destroy(r)`                  | `axiom_renderer_destroy(void*)`             |
| `renderer_begin_frame(r)`              | `axiom_renderer_begin_frame(void*)`         |
| `renderer_end_frame(r)`                | `axiom_renderer_end_frame(void*)`           |
| `renderer_should_close(r)`             | `axiom_renderer_should_close(void*)`        |
| `renderer_draw_triangles(r, p, c, n)`  | `axiom_renderer_draw_triangles(...)`        |
| `renderer_get_time(r)`                 | `axiom_renderer_get_time(void*)`            |
| `shader_load(r, path, stage)`          | `axiom_shader_load(void*, char*, int)`      |
| `pipeline_create(r, vert, frag)`       | `axiom_pipeline_create(void*, void*, void*)`|
| `renderer_bind_pipeline(r, pipeline)`  | `axiom_renderer_bind_pipeline(void*, void*)`|

### How Lux shaders are loaded

1. Write shaders in Lux (`.lux` files) using Lux's shader syntax
2. Compile with `luxc` to produce SPIR-V bytecode (`.spv` files)
3. At runtime, call `shader_load(renderer, "path/to/shader.spv", stage)`:
   - Stage `0` = vertex shader
   - Stage `1` = fragment shader
4. Create a pipeline: `pipeline_create(renderer, vert_module, frag_module)`
5. Bind before drawing: `renderer_bind_pipeline(renderer, pipeline)`

### The data flow: AXIOM arrays to pixels

1. **AXIOM arrays**: Vertex data (positions, colors, normals) lives in AXIOM
   `array[f64, N]` values allocated on the stack or heap.
2. **Vulkan buffers**: The renderer copies array data into GPU-visible vertex
   buffers (VkBuffer with VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT).
3. **Lux shaders**: The vertex shader reads per-vertex attributes; the
   fragment shader computes per-pixel color.
4. **Pixels**: Vulkan rasterizes triangles through the pipeline and presents
   the result to the swapchain (window).

## Current status (Phase 7 MVP)

The renderer is a **stub** that prints lifecycle events:
- `renderer_create` logs the window dimensions
- `renderer_end_frame` logs frame counts
- `renderer_draw_triangles` logs vertex data on the first frame
- No actual window is opened; no GPU work is performed

This validates the API contract end-to-end. The stub can be replaced with
a real Vulkan implementation without changing any AXIOM source code.

## Running the triangle example

```bash
cargo run -- examples/vulkan/triangle.axm
```

Expected output (stub mode):
```
[AXIOM Renderer] Created 800x600 window: "AXIOM Triangle" (stub)
[AXIOM Renderer] Backend: headless stub (Vulkan planned)
[AXIOM Renderer] draw_triangles: 3 vertices (0.00,0.50,0.00)...
[AXIOM Renderer] Frame 1 complete
[AXIOM Renderer] Frame 2 complete
[AXIOM Renderer] Frame 3 complete
[AXIOM Renderer] Frame 10 complete
...
[AXIOM Renderer] Frame 100 complete
100
[AXIOM Renderer] Destroyed after 100 frames: "AXIOM Triangle"
```
