# Plan: More Examples and Benchmarks

## Current State
- 197 benchmarks (115 simple, 30 complex, 20 real-world, 30 memory, 2 GitHub repos)
- 27 example programs across 12 categories
- 24 test samples

## What's Missing

### 1. Compiler Self-Benchmarks
AXIOM should benchmark its OWN compiler. These measure the compiler's performance, not the generated code.

| Benchmark | What it measures |
|-----------|-----------------|
| `bench_lexer_throughput` | Tokens/second on a large .axm file |
| `bench_parser_throughput` | AST nodes/second for a complex program |
| `bench_hir_lowering` | HIR lowering time for 1000+ line program |
| `bench_codegen_throughput` | LLVM IR lines/second |
| `bench_full_pipeline` | End-to-end .axm → binary time vs program size |

Create large .axm test inputs (1K, 5K, 10K lines) by generating repetitive valid AXIOM code.

### 2. Classic Algorithm Benchmarks (from Project Euler / Rosetta Code)
With arrays and heap, we can now implement many more:

| Category | Benchmarks | Count |
|----------|-----------|-------|
| Sorting | quicksort, mergesort, heapsort, radix sort (all with heap arrays) | 4 |
| Graph | BFS, DFS, Dijkstra, Floyd-Warshall, topological sort | 5 |
| String processing | string search, edit distance, longest common subsequence | 3 |
| Dynamic programming | knapsack, longest increasing subsequence, matrix chain | 3 |
| Numerical | FFT (improved), SVD, QR decomposition | 3 |
| Crypto (with native bitwise) | AES round, SHA-512, BLAKE2b | 3 |

### 3. Real-World Application Examples
Complete programs that demonstrate AXIOM for real use cases:

| Example | Description | Features Used |
|---------|-------------|---------------|
| `examples/json_parser/` | JSON parser in AXIOM | strings, vec, recursion |
| `examples/http_client/` | Simple HTTP GET via sockets (extern fn) | FFI, strings, I/O |
| `examples/image_filter/` | Image processing (blur, sharpen, edge detect) | arrays, @pure, @parallel_for |
| `examples/physics_sim/` | 2D physics with collision detection | structs, @pure, arena |
| `examples/pathfinder/` | A* pathfinding on a grid | arrays, heap, @strategy |
| `examples/compiler_demo/` | A tiny language interpreter in AXIOM | self-referential, showcases expressiveness |

### 4. Lux Shader Integration Examples
Now that we have the wgpu renderer with PBR:

| Example | Description |
|---------|-------------|
| `examples/vulkan/rotating_cube/` | Animated rotating cube with PBR materials |
| `examples/vulkan/multiple_objects/` | Load and render multiple glTF models |
| `examples/vulkan/camera_orbit/` | Interactive camera with mouse orbit |
| `examples/vulkan/particles_gpu/` | Particle galaxy with GPU-rendered points |

### 5. Benchmark Game Recreations
With arrays, we can now implement several Benchmarks Game programs:

| Benchmark | Feasible? | Notes |
|-----------|-----------|-------|
| n-body | YES | Use heap arrays for body state |
| spectral-norm | YES | Matrix operations with heap arrays |
| mandelbrot | YES | Pixel grid with heap array output |
| binary-trees | DONE (memory benchmarks) | Arena-based, already 80% faster than C |
| fannkuch-redux | YES | Permutation arrays |

### 6. @strategy-Enabled Optimization Demos
Programs specifically designed for the LLM optimizer:

| Demo | What the LLM tunes |
|------|-------------------|
| `matmul_tunable` | Tile sizes, loop order, unroll factor |
| `nbody_tunable` | Particle batch size, force computation method |
| `sort_tunable` | Threshold for switching from quicksort to insertion sort |
| `stencil_tunable` | Tile dimensions, prefetch distance |

## Implementation Priority

1. **Compiler self-benchmarks** (unique to AXIOM, no other language does this)
2. **Benchmarks Game recreations** (credibility — recognized by the community)
3. **@strategy optimization demos** (showcases AXIOM's unique value)
4. **Real-world application examples** (proves AXIOM is practical)
5. **Lux shader integration examples** (proves the rendering pipeline)
6. **Classic algorithm benchmarks** (breadth of coverage)

## Target: 300+ total benchmarks, 40+ examples
