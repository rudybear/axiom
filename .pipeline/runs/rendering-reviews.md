# Rendering Architecture Reviews: Approach C (Hybrid Layered)

## Date: 2026-03-24
## Reviewed document: rendering-architecture.md

---

# PART 1: OPTIMISTIC REVIEW

## Verdict: Approach C is the right call. It is realistic and well-aligned with AXIOM's thesis.

### 1. The layering is correct and mirrors industry practice

The document's comparison to Unreal (C++ RHI + C++ logic + HLSL) and Unity (C++ native + C# scripting + shaders) is not a rhetorical flourish -- it is exactly how successful engines are structured. The "Rust = RHI, AXIOM = game logic, Lux = shaders" split puts each language where it is strongest. Vulkan's API is a poor fit for any language that lacks mature struct nesting, generics, and error handling. Rust is purpose-built for that layer. Rendering *logic* -- transform math, culling, sorting, batching, light management -- is stateless numeric computation, which is exactly where AXIOM's `@pure`, `@parallel_for`, and `@strategy` annotations provide real, measurable value.

### 2. The ~40 C ABI functions are a well-scoped contract

Forty functions is a manageable API. It is smaller than SDL's renderer API, smaller than sokol_gfx, and dramatically smaller than raw Vulkan (200+ entry points, 500+ struct types). The proposed API has a clean "resource creation + command recording" pattern that maps directly to how Vulkan works internally, without exposing Vulkan's structural complexity. This is not a novel design -- it is essentially what WebGPU or Metal's command encoder pattern looks like when flattened to C ABI. The fact that existing precedent validates this shape is a strength.

### 3. The optimization surface in Layer 2 is real and significant

The pessimistic reviewer will ask "how much rendering code is actually `@pure`?" The answer: most of it. The Layer 2 modules identified in the document -- transform math, camera math, frustum culling, AABB tests, draw call sorting, light culling, material parameter packing -- are all pure CPU-side computation with no side effects. This is not a hypothetical. Consider:

- **Frustum culling of 10K objects**: 6 plane tests x 8 AABB corners x 10K objects = 480K dot products. This is embarrassingly parallel and `@pure`. The job system + `@parallel_for` will absolutely speed this up, and `@strategy` on batch size is a real optimization knob.
- **Draw call sorting**: Sorting 1K-10K draw keys by material/depth is a classic `@strategy` target where radix sort vs. merge sort vs. insertion sort genuinely matters depending on the distribution.
- **Matrix math**: Every `mat4_mul`, `vec3_normalize`, `vec3_cross` benefits from `@pure` -> `memory(none)` + fast-math. LLVM will FMA-contract these aggressively.

This is not AXIOM pretending to be a GPU language. This is AXIOM doing what it does best: optimizing CPU-side numeric computation that feeds the GPU.

### 4. The phased rollout is honest about effort

The timeline (PBR sphere in 2 weeks, full deferred in 8 weeks, RT in 14 weeks) is realistic *because* it leverages 26K LOC of existing Lux playground Rust code. Phase 1 is essentially: extract a library crate, write 40 C ABI wrappers, write 500 lines of AXIOM math, and connect them. This is 2 weeks of focused work, not a moonshot. The document does not hide the harder parts (multi-file compilation, import system, generics) and correctly classifies them as "nice to have" for later phases.

### 5. It aligns with AXIOM's core thesis

AXIOM's thesis is: "AI agents read, write, and optimize this code." For Approach A, the renderer is an opaque Rust DLL -- AI agents cannot optimize it. For Approach B, the renderer is raw Vulkan calls in a language missing half the features needed to write them safely. Approach C puts the *decision-making* code (what to draw, in what order, with what parameters) in AXIOM where AI agents can see it, while keeping the *mechanical plumbing* (Vulkan structs, GPU memory, synchronization) in Rust where it belongs. An AI agent optimizing `cull_batch_size` from 512 to 2048 based on profiling data is a concrete, demonstrable capability that showcases AXIOM's value proposition.

### 6. The GPGPU path is credible

`gpu_cmd_dispatch_compute(ctx, gx, gy, gz)` plus `gpu_create_storage_buffer` plus `gpu_readback_buffer` is a three-function API for GPGPU. Combined with Lux compute shaders (which already compile to SPIR-V), AXIOM programs can dispatch GPU compute workloads. The AXIOM side orchestrates: prepare data, upload, dispatch, readback. The Lux side does the GPU math. This is a clean separation that actually works.

### 7. The "who writes Lux shaders" question has a good answer

Lux already has 60+ shaders, a stdlib with 15 modules (brdf, lighting, shadow, ibl, pbr_pipeline, noise), and 1,462 tests. The shader ecosystem is not hypothetical -- it exists today. Layer 0 simply loads them. Users who want custom shaders write Lux (which is designed for this), not AXIOM. This is the correct separation of concerns.

---

# PART 2: PESSIMISTIC REVIEW

## Verdict: Approach C has real structural problems that the document glosses over. Several claims do not survive contact with implementation details.

### Problem 1: "Layer 2 AXIOM rendering library" is writing an engine in a language missing essential abstractions

The document lists 11 `.axm` modules for Layer 2: transform, camera, culling, sort, batch, lights, materials, scene, render_loop, aabb, math. Let us examine what these actually require.

**Material system (`materials.axm`)**. A material system needs polymorphism. A PBR metallic-roughness material has different parameters than an unlit material, a glass material, or a subsurface scattering material. In Rust, you use enums or trait objects. In C++, you use virtual methods or variant types. In AXIOM, you have: no enums (sum types are "builtin i64-packed hacks"), no dynamic dispatch ("fn_ptr builtins are limited to single-arg calls"), no generics codegen, no method syntax. So how does `materials.axm` represent "this mesh uses material variant X with these specific parameters"? The answer is: flat arrays of floats with integer type tags and manual offset computation. This is C circa 1985. It works, but it is not "rendering logic in AXIOM" -- it is "manual memory layout in AXIOM with no type safety." An AI agent reading this code will see `ptr_read_f64(mat_data, base + 7)` and have no idea that offset 7 is "roughness". The `@pure` annotation on material parameter packing is meaningless -- the bottleneck in material systems is not computation, it is data organization and polymorphic dispatch.

**Scene graph (`scene.axm`)**. A scene graph is a tree. Trees require recursive data structures (nodes pointing to children). AXIOM has no nested structs, no struct-in-arrays, no struct pointer fields. You would have to represent the scene graph as parallel flat arrays (parent indices, transform arrays, dirty flag arrays) with manual index arithmetic. This is doable -- ECS engines do it -- but it is a sophisticated data-oriented design pattern that requires careful thought, not a weekend project. The document's estimate of "tree traversal with `@parallel_for` on sibling groups" assumes the tree is already laid out in a cache-friendly, parallelizable format. Who designs this layout? Who validates it? AXIOM has no tooling for this.

**Draw call batching (`batch.axm`)**. Batching requires grouping draws by pipeline state (shader, blend mode, depth state) and material, then emitting instanced draw calls or indirect draw buffers. This requires: sorting heterogeneous data (different key types for different sort criteria), building indirect draw command buffers (structs with specific GPU-expected layouts), and managing instance data buffers. None of these are particularly `@pure` -- they involve mutable state accumulation. The "rendering logic" that benefits from AXIOM annotations is actually a thin slice of the total rendering logic.

### Problem 2: The 40 C ABI functions -- API design is the hardest part, and it is hand-waved

The document lists ~40 functions and says "Key design decision: The draw commands record into a Vulkan command buffer managed by Rust." But:

**Who designs the render pass abstraction?** `gpu_cmd_begin_render_pass(ctx, 0)` takes an integer (0=shadow, 1=gbuffer, 2=forward, 3=lighting). This means the set of render passes is hardcoded in Rust. What happens when the user wants a custom render pass (e.g., a velocity buffer for motion blur, or a light pre-pass for clustered forward)? They cannot create one from AXIOM. They need to modify Rust code, recompile the DLL, and hope the new integer ID does not collide. This is exactly the "black box" problem that Approach C was supposed to solve.

**GPU memory management is absent.** The API has `gpu_create_buffer` and `gpu_upload_buffer`, but no concept of GPU memory pressure, buffer reuse, staging buffer management, or frame-in-flight resource lifetime. When AXIOM calls `gpu_create_buffer` 10,000 times per frame for dynamic data, who handles the ring buffer allocation? Rust? Then AXIOM does not actually control resource management. AXIOM? Then it needs to understand Vulkan's frame synchronization model, which is not exposed through this API.

**Descriptor set management is hidden but critical.** `gpu_set_uniform(ctx, pipeline, set, binding, data, size)` and `gpu_bind_texture(ctx, pipeline, set, binding, image)` hide the fact that descriptor sets in Vulkan must be allocated from pools, updated before use, and cannot be updated while in flight. If AXIOM calls `gpu_set_uniform` between draw calls, who ensures the descriptor set is not still in use by the previous frame? This requires either per-frame descriptor set duplication or a ring buffer of descriptor sets -- complex Vulkan machinery that is invisible to the AXIOM programmer but will cause validation errors and GPU crashes if mismanaged.

**The API will grow uncontrollably.** Forty functions today. But to support deferred rendering, you need G-buffer attachment configuration. For shadow maps, you need depth bias control. For RT, you need acceleration structure builds. For compute, you need buffer barriers. Each new feature adds 3-5 new C ABI functions. By Phase 5, you will have 80-100 functions, and the API will be a leaky abstraction over Vulkan with no coherent design philosophy.

### Problem 3: "Move more rendering control into AXIOM" -- is this actually desirable?

The document frames this as obvious: more AXIOM = more optimization = better. But consider:

**Debugging.** When the deferred rendering pass produces black output, where is the bug? In `render_loop.axm` (wrong render pass sequencing)? In `lights.axm` (wrong shadow matrix)? In the Rust layer (wrong descriptor set binding)? In Lux shaders (wrong G-buffer format)? You now have four languages (AXIOM, Rust, Lux, SPIR-V) across four layers to debug. AXIOM has no debugger, no printf-style debugging (only `print_i32`/`print_f64`), and no way to inspect GPU state. Rust has `log`, `tracing`, Vulkan validation layers, RenderDoc capture. Every line of rendering logic moved from Rust to AXIOM is a line that becomes harder to debug.

**How much rendering code is actually `@pure`?** Let us audit Layer 2 honestly:
- `math.axm`: 100% `@pure`. This is the sweet spot.
- `camera.axm`: 90% `@pure` (math functions), 10% state mutation (camera position update).
- `culling.axm`: 90% `@pure` (plane tests), but the `@parallel_for` dispatch is not pure.
- `sort.axm`: 0% `@pure`. Sorting is inherently about mutable state reordering.
- `batch.axm`: 0% `@pure`. Batching is mutable accumulation.
- `lights.axm`: 50% `@pure` (shadow matrix math), 50% state management (light list mutation).
- `materials.axm`: 0% `@pure`. Material binding is descriptor set management.
- `scene.axm`: 0% `@pure`. Scene graph traversal modifies transforms.
- `render_loop.axm`: 0% `@pure`. It is a sequence of side-effecting GPU commands.

So roughly 30-40% of Layer 2 code benefits from `@pure`. The `@strategy` use cases (sort algorithm, batch size, render path selection) are real but narrow -- they affect maybe 5-10 tunable parameters. The `@parallel_for` use case is real for culling and potentially light assignment. But the majority of rendering logic is imperative state management that gains nothing from AXIOM's annotations. It would be equally readable and far more debuggable in Rust.

### Problem 4: glTF loading is massive and under-estimated

The document says "Reuse `gltf_loader.rs` (1288 lines)" and moves on. But glTF loading for a real renderer involves:

- Mesh loading: vertex positions, normals, tangents, UVs, joint weights, joint indices. Multiple vertex formats, interleaved vs. separate buffers. Index buffers in u16 and u32 variants.
- Material loading: PBR metallic-roughness parameters, normal maps, occlusion maps, emissive maps, alpha modes (opaque, mask, blend), double-sided flag, texture transform extensions.
- Texture loading: PNG, JPEG, KTX2 decoding, mipmap generation, GPU format conversion (sRGB vs linear), compressed texture formats (BC7, ASTC). Each texture needs a VkImage, VkImageView, VkSampler, descriptor binding.
- Animation: skeletal animation requires joint hierarchies, inverse bind matrices, keyframe interpolation (linear, step, cubic spline), multiple animation channels (translation, rotation, scale).
- Scene hierarchy: nodes with parent-child relationships, local transforms, world transform computation.

The existing `gltf_loader.rs` (1288 lines) probably handles a subset of this. But the document claims full Lux playground parity in 8 weeks. The Lux playground has a `scene_manager.rs` and `scene.rs` that together are likely 2000+ lines handling all of this. This is not "reuse a file" -- this is "ensure the extracted library crate correctly exposes all scene data to the C ABI layer, with handle management, lifetime tracking, and GPU resource cleanup."

### Problem 5: Textures, images, and descriptor sets -- the elephant in the room

The document mentions textures only in passing (`gpu_bind_texture`, `gpu_upload_image`). But textures in a real PBR renderer are the hardest part of the resource management story:

- A single glTF material can reference 5 textures (albedo, normal, metallic-roughness, occlusion, emissive).
- Sponza has ~25 materials, so ~100+ textures.
- Each texture needs: staging buffer upload, transfer queue copy, layout transition, mipmap generation (either blit chain or compute), VkImageView creation, VkSampler creation or reuse, descriptor set binding.
- Descriptor indexing (bindless textures) is the modern approach but requires `VK_EXT_descriptor_indexing` and careful descriptor pool sizing.
- Non-bindless approaches require per-material descriptor sets, which means N descriptor set allocations, N descriptor writes, and N `vkCmdBindDescriptorSets` calls.

None of this machinery is described. `gpu_upload_image(ctx, img, pixels, size)` hand-waves over the staging buffer, transfer command buffer, pipeline barrier, and layout transition that a texture upload actually requires. Who generates mipmaps? Who manages sampler deduplication? Who allocates the descriptor pool? The Rust layer must do all of this invisibly, which means the Rust layer is not a "thin backend" -- it is a full resource management engine.

### Problem 6: Multi-file compilation is not "nice to have" -- it is a hard requirement

The document lists multi-file compilation as "Nice to have (Phase 3+)" with a 1-week estimate. But Layer 2 is 11 `.axm` files that must all be compiled and linked together with user code. Without multi-file compilation, the "pragmatic approach" is file concatenation -- literally `cat math.axm camera.axm culling.axm ... user.axm > combined.axm && axiom compile combined.axm`. This means:

- No module namespaces (all function names are global, collision risk).
- No separate compilation (change one file, recompile everything).
- No dependency tracking.
- Compilation time scales linearly with total codebase size.
- IDE/editor support is nonexistent for concatenated files.

For Phase 1 (single demo file), concatenation works. For Phase 2+ (11 library files + user code), it is untenable. Multi-file compilation should be Phase 1 or early Phase 2, not Phase 3+. The 1-week estimate is also optimistic -- a real module system with name resolution, separate compilation, and linking typically takes 2-4 weeks.

---

# SHARED QUESTION: Should AXIOM be a general-purpose GPU programming language?

## Optimistic answer

No, and Approach C correctly avoids this. AXIOM is a CPU-side systems language with AI optimization. Lux is the GPU language. Approach C draws the right line: AXIOM optimizes CPU rendering logic (culling, sorting, batching, scene management), Lux handles GPU shaders (PBR, lighting, shadows), and Rust manages Vulkan plumbing. AXIOM does not need to talk to the GPU directly. It needs to *orchestrate* GPU work efficiently, which is exactly what Layer 2 does. The GPGPU story (Phase 4) is AXIOM dispatching Lux compute shaders -- AXIOM prepares data and issues dispatch commands, Lux does the GPU math. This is the correct division of labor.

AXIOM should double down on what makes it unique: `@pure` optimization of CPU compute, `@strategy` for AI-tunable parameters, `@parallel_for` for multi-core workloads, and LLM-readable source code. These capabilities are genuinely novel and valuable. Trying to become a GPU programming language would require years of compiler work (generics, traits, closures, pattern matching, async) and would still produce an inferior experience compared to Lux (which is purpose-built for GPU shading) or CUDA/HLSL/WGSL (which have decades of tooling).

## Pessimistic answer

The document's GPGPU ambitions (Phase 4: "enabling GPGPU programming," "compute shader dispatch from AXIOM") are misleading. AXIOM does not dispatch compute shaders -- it calls `gpu_cmd_dispatch_compute(ctx, 64, 64, 1)`, which tells Rust to call `vkCmdDispatch`. The actual compute kernel is a Lux shader. AXIOM's role is writing three integers. This is not "GPGPU programming in AXIOM" -- it is "calling a function that takes three integers." The real GPGPU programming happens in Lux.

If the goal is to make AXIOM a language that AI agents can optimize rendering with, the honest answer is: **most rendering optimization happens in shaders (GPU) or in Vulkan resource management (Rust), not in CPU-side draw call management (AXIOM Layer 2)**. The biggest performance wins in modern rendering are:
1. Shader optimization (Lux's domain)
2. GPU memory layout and bandwidth (Rust's domain via gpu-allocator)
3. Reducing CPU overhead via indirect draws and GPU-driven rendering (requires compute shaders, which is Lux's domain)
4. CPU-side culling and sorting (AXIOM's domain -- but this is a shrinking slice of the pie as GPU-driven rendering matures)

AXIOM should not try to be a general-purpose GPU programming language. But it should also be honest that its optimization surface in rendering is narrower than the document implies. The killer demo is not "AXIOM renders Sponza" -- it is "AXIOM + Lux + Rust render Sponza, and an AI agent tuned the culling batch size, sort algorithm, and shadow resolution to get 40% more FPS." That is a real demo, and it is achievable. But it requires admitting that AXIOM is one piece of a three-language stack, not the star of the show.

---

# SUMMARY

| Aspect | Optimistic take | Pessimistic take |
|---|---|---|
| Architecture | Correct layering, mirrors industry practice | Layer 2 scope is overambitious for a language without generics/enums/closures |
| 40 C ABI functions | Well-scoped, manageable contract | Will grow to 80-100, render pass abstraction is too rigid, descriptor management is hidden complexity |
| `@pure` on rendering code | 100% of math, 90% of culling, significant slice | Only ~30-40% of Layer 2 code is actually `@pure` |
| Timeline | Realistic because it leverages existing 26K LOC Rust code | Multi-file compilation is under-estimated and should be Phase 1, not Phase 3 |
| glTF / textures | Reuse existing Rust code, just add C ABI wrappers | Texture pipeline (staging, mipmaps, descriptors) is a massive hidden iceberg |
| AXIOM as GPU language | Correctly avoids it -- AXIOM orchestrates, Lux computes | GPGPU claims are marketing; AXIOM writes three integers, Lux does the work |
| Overall | Ship it. Approach C is the pragmatic choice. | Ship it, but with eyes open about what Layer 2 can realistically contain in AXIOM vs. what will quietly stay in Rust. |
