# AXIOM Master Task List

**Pipeline:** 7-agent (Architect → Optimistic/Pessimistic Design Review → Coder → QA → Optimistic/Pessimistic Code Review)

**Order:** Correctness → MT → LLM Optimizer → Platform Optimization → Language Features → Ecosystem → Vulkan Renderer → Game Engine

---

## Track 1: Multithreading (Correctness + Features)

| ID | Milestone | Description | Depends On |
|----|-----------|-------------|------------|
| **MT-1** | Fix UB + Soundness | Remove incorrect `@pure`/`noalias`/`nosync` on shared ptrs. Add fences. Fix `@pure` semantics for write-through-ptr. | — |
| **MT-2** | `@parallel_for` with Data Clauses | `private`, `shared_read`, `shared_write`, `reduction(+: var)`. HIR validation. Correct LLVM IR with atomics/fences. | MT-1 |
| **MT-3** | Reduction Patterns | Thread-local accumulation + final combine. `reduction(+: sum)` auto-generates correct code. Identity values per type. | MT-2 |
| **MT-4** | Ownership Slices | `slice[T, readonly]` / `slice[T, exclusive]` types. Compiler proves non-aliasing at type level. Enables `noalias` correctly. | MT-2 |
| **MT-5** | Job Dependency Graph | `JobHandle`, `depends_on`, `job_wait(handle)`. Atomic counter-based completion (Naughty Dog pattern). | MT-3, MT-4 |
| **MT-6** | LLVM Parallel Metadata | `!llvm.access.group`, `!llvm.loop.parallel_accesses` on proven-parallel loops. | MT-4 |

## Track 2: LLM Optimizer Enhancements

| ID | Milestone | Description | Depends On |
|----|-----------|-------------|------------|
| **L1** | Constraint-Driven LLM Prompts | Thread `@constraint { optimize_for: X }` into LLM prompt. Changes reasoning (performance vs memory vs size vs latency). | — |
| **L2** | Hardware Counter Integration | Feed `perf` data (cache misses, branch mispredictions) to LLM. | L1 |
| **L3** | Recursive `@const` Evaluation | Fix compile-time evaluator to handle recursive functions (currently only basic arithmetic). | — |
| **L4** | PGO Bootstrap | Profile the compiler itself, recompile with profile data, iterate. | L2 |

## Track 3: Platform-Specific Optimization

| ID | Milestone | Description | Depends On |
|----|-----------|-------------|------------|
| **P1** | `@target { cpu: "native" }` | Pass `-march=native` to clang. Detect available CPU features. | — |
| **P2** | CPUID Feature Detection | Query CPU features at compile time, expose to `@constraint` system. | P1 |
| **P3** | SIMD Intrinsics from `@vectorizable` | Emit explicit SIMD instructions for annotated loops (beyond auto-vec). | P2, MT-6 |
| **P4** | `@constraint { optimize_for: X }` in Codegen | Map constraint to clang flags (-O3/-Os/-Oz) and LLVM pass configuration. | P1 |

## Track 4: Language Features

| ID | Milestone | Description | Depends On |
|----|-----------|-------------|------------|
| **F1** | Sum Types + Pattern Matching | `type Result = Ok(T) \| Err(E)`, `match` statement with exhaustive checking. | — |
| **F2** | String Type | `string` as byte array + length. String literals, concatenation, comparison. | — |
| **F3** | Dynamic Arrays | Growable heap-backed `vec[T]` type with push/pop/len. | — |
| **F4** | Generics / Monomorphization | `fn sort[T](arr: array[T, N])` → specialized per type at compile time. | F1 |
| **F5** | Closures / Function Pointers | `fn(i32) -> i32` as values. Needed for callbacks, higher-order functions. | — |
| **F6** | Module System (Codegen) | Multi-file programs. `import` resolves to separate compilation units. | — |
| **F7** | Error Handling | `Result` type + propagation operator (like Rust's `?`). | F1 |
| **F8** | While-let / If-let | Pattern matching sugar for option/result types. | F1, F7 |

## Track 5: Ecosystem & Polish

| ID | Milestone | Description | Depends On |
|----|-----------|-------------|------------|
| **E1** | CI/CD Pipeline | GitHub Actions: run tests + benchmarks on every push. | — |
| **E2** | DWARF Debug Info | Emit debug info in LLVM IR so compiled programs work with debuggers. | — |
| **E3** | Formatter (`axiom fmt`) | Auto-format AXIOM source code. | — |
| **E4** | LSP Server | Editor integration (syntax highlighting, go-to-definition, errors). | — |
| **E5** | Package Manager | Share AXIOM libraries. Dependency resolution. | F6 |
| **E6** | Documentation Generator | Generate docs from `@intent` and doc comments. | — |

## Track 6: Real Vulkan Renderer (After Tracks 1-5)

| ID | Milestone | Description | Depends On |
|----|-----------|-------------|------------|
| **R1** | Vulkan Bootstrap | ash + winit + gpu-allocator. Window + GPU triangle. | F1 (for error handling) |
| **R2** | AXIOM Arrays → GPU Buffers | Upload vertex data from AXIOM to Vulkan buffers. | R1 |
| **R3** | Lux SPIR-V Shader Loading | Real VkShaderModule + VkPipeline from Lux-compiled .spv files. | R1 |
| **R4** | Descriptor Sets + Uniforms | Pass MVP matrix, time, etc. from AXIOM to shaders. | R3 |
| **R5** | Production Renderer | Instancing, depth buffer, compute shaders. | R4, MT-5 |

## Track 7: Game Engine (After Track 6)

| ID | Milestone | Description | Depends On |
|----|-----------|-------------|------------|
| **G1** | Proper ECS with Archetype Storage | Beyond the current demo. Real archetype-based component storage. | R5, MT-5, F4 |
| **G2** | Input System | Keyboard/mouse from Win32/Vulkan surface events. | R1 |
| **G3** | Audio | Basic WAV playback via C runtime. | — |
| **G4** | Hot Reload | Recompile functions while program runs. | F6 |
| **G5** | Killer Demo v2 | 10K particles with real Vulkan + Lux shaders + parallel jobs. | All above |

## Track 8: Self-Improvement (Ongoing)

| ID | Milestone | Description | Depends On |
|----|-----------|-------------|------------|
| **S1** | Self-Hosted Parser | AXIOM parser written in AXIOM. | F2, F3, F4 |
| **S2** | Compiler Self-Optimization | Compiler uses LLM loop to optimize its own hot paths. | L4 |
| **S3** | Source-to-Source AI Optimizer | LLM rewrites AXIOM source (not just ?params). | L1, L2 |

---

## Execution Order (Recommended)

```
Phase A: MT-1 (fix UB — URGENT)
Phase B: MT-2, MT-3 (parallel_for + reductions — core MT)
Phase C: L1, L3, P1, P4 (constraints + consteval + platform — quick wins, parallel)
Phase D: MT-4, MT-5, MT-6 (ownership + deps + LLVM metadata — advanced MT)
Phase E: F1, F2, F3, F5 (sum types, strings, dynamic arrays, closures — language gaps)
Phase F: L2, P2, P3 (hardware counters, CPUID, SIMD — platform depth)
Phase G: F4, F6, F7, F8 (generics, modules, errors — language maturity)
Phase H: E1, E2, E3 (CI, debug info, formatter — ecosystem basics)
Phase I: R1-R5 (real Vulkan renderer)
Phase J: G1-G5 (game engine)
Phase K: S1-S3 (self-improvement)
```

**Total: 47 milestones across 8 tracks.**
**Each milestone goes through the full 7-agent pipeline.**
