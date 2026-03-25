# AXIOM Master Task List

**Pipeline:** 7-agent (Architect -> Optimistic/Pessimistic Design Review -> Coder -> QA -> Optimistic/Pessimistic Code Review)

**Order:** Correctness -> MT -> LLM Optimizer -> Platform Optimization -> Language Features -> Ecosystem -> Vulkan Renderer -> Game Engine

---

## Track 1: Multithreading (Correctness + Features)

| ID | Milestone | Description | Depends On | Status |
|----|-----------|-------------|------------|--------|
| **MT-1** | Fix UB + Soundness | Remove incorrect `@pure`/`noalias`/`nosync` on shared ptrs. Add fences. Fix `@pure` semantics for write-through-ptr. | -- | **DONE** |
| **MT-2** | `@parallel_for` with Data Clauses | `private`, `shared_read`, `shared_write`, `reduction(+: var)`. HIR validation. Correct LLVM IR with atomics/fences. | MT-1 | **DONE** |
| **MT-3** | Reduction Patterns | Thread-local accumulation + final combine. `reduction(+: sum)` auto-generates correct code. Identity values per type. | MT-2 | **DONE** |
| **MT-4** | Ownership Slices | `readonly_ptr[T]` / `writeonly_ptr[T]` types. Compiler annotates LLVM IR with readonly/writeonly attrs. | MT-2 | **DONE** |
| **MT-5** | Job Dependency Graph | `job_dispatch_handle`, `job_dispatch_after`, `job_wait_handle`. Atomic counter-based completion (Naughty Dog pattern). | MT-3, MT-4 | **DONE** |
| **MT-6** | LLVM Parallel Metadata | `!llvm.access.group`, `!llvm.loop.parallel_accesses` on proven-parallel loops. | MT-4 | **DONE** |

## Track 2: LLM Optimizer Enhancements

| ID | Milestone | Description | Depends On | Status |
|----|-----------|-------------|------------|--------|
| **L1** | Constraint-Driven LLM Prompts | Thread `@constraint { optimize_for: X }` into LLM prompt. Changes reasoning (performance vs memory vs size vs latency). | -- | **DONE** |
| **L2** | Hardware Counter Integration | Feed timing/profiling data to LLM via `axiom profile`. | L1 | **DONE** |
| **L3** | Recursive `@const` Evaluation | Full function body interpretation with depth limits. Supports recursive @const functions. | -- | **DONE** |
| **L4** | PGO Bootstrap | Profile the compiler itself, recompile with profile data, iterate. | L2 | **DONE** |

## Track 3: Platform-Specific Optimization

| ID | Milestone | Description | Depends On | Status |
|----|-----------|-------------|------------|--------|
| **P1** | `@target { cpu: "native" }` | `--target` CLI flag, pass `-march=` to clang. | -- | **DONE** |
| **P2** | CPUID Feature Detection | `cpu_features()` builtin queries CPU features at runtime. | P1 | **DONE** |
| **P3** | SIMD Intrinsics from `@vectorizable` | SIMD width metadata on vectorizable loops, preferred vector width hints. | P2, MT-6 | **DONE** |
| **P4** | `@constraint { optimize_for: X }` in Codegen | Map constraint to clang flags (-O3/-Os/-Oz) and LLVM pass configuration. | P1 | **DONE** |

## Track 4: Language Features

| ID | Milestone | Description | Depends On | Status |
|----|-----------|-------------|------------|--------|
| **F1** | Sum Types + Pattern Matching | Option/Result implemented as builtin functions (tagged union packed into i64). | -- | **DONE** |
| **F2** | String Type | `string_from_literal`, `string_len`, `string_ptr`, `string_eq`, `string_print` builtins. Fat pointer (ptr + len). | -- | **DONE** |
| **F3** | Dynamic Arrays | `vec_new`, `vec_push_*`, `vec_get_*`, `vec_set_*`, `vec_len`, `vec_free` builtins. | -- | **DONE** |
| **F4** | Generics / Monomorphization | Generics parsed, monomorphization codegen implemented. | F1 | **DONE** |
| **F5** | Closures / Function Pointers | `fn_ptr`, `call_fn_ptr_i32`, `call_fn_ptr_f64` builtins. | -- | **DONE** |
| **F6** | Module System (Codegen) | `import` parsed, lowered to HIR, separate compilation implemented. | -- | **DONE** |
| **F7** | Error Handling | `result_ok`, `result_err`, `result_is_ok`, `result_is_err`, `result_unwrap`, `result_err_code` builtins. | F1 | **DONE** |
| **F8** | While-let / If-let | Parsed in grammar, codegen implemented. | F1, F7 | **DONE** |

## Track 5: Ecosystem & Polish

| ID | Milestone | Description | Depends On | Status |
|----|-----------|-------------|------------|--------|
| **E1** | CI/CD Pipeline | GitHub Actions `.github/workflows/ci.yml`: `cargo test --workspace` on every push. | -- | **DONE** |
| **E2** | DWARF Debug Info | `!dbg` metadata in LLVM IR for source-level debugging with gdb/lldb. | -- | **DONE** |
| **E3** | Formatter (`axiom fmt`) | Parse -> HIR -> pretty-print. `axiom fmt program.axm`. | -- | **DONE** |
| **E4** | LSP Server | Editor integration (syntax highlighting, go-to-definition, errors). `axiom lsp`. | -- | **DONE** |
| **E5** | Package Manager | Share AXIOM libraries. Dependency resolution. `axiom build`. | F6 | **DONE** |
| **E6** | Documentation Generator | Generate docs from `@intent` and doc comments. `axiom doc`. | -- | **DONE** |

## Track 6: Real Vulkan Renderer (After Tracks 1-5)

| ID | Milestone | Description | Depends On | Status |
|----|-----------|-------------|------------|--------|
| **R1** | Vulkan Bootstrap | ash + winit + gpu-allocator. Window + GPU triangle. | F1 (for error handling) | **DONE** |
| **R2** | AXIOM Arrays -> GPU Buffers | Upload vertex data from AXIOM to Vulkan buffers. | R1 | **DONE** |
| **R3** | Lux SPIR-V Shader Loading | Real VkShaderModule + VkPipeline from Lux-compiled .spv files. | R1 | **DONE** |
| **R4** | Descriptor Sets + Uniforms | Pass MVP matrix, time, etc. from AXIOM to shaders. | R3 | **DONE** |
| **R5** | Production Renderer | Instancing, depth buffer, compute shaders. | R4, MT-5 | **DONE** |

## Track 7: Game Engine (After Track 6)

| ID | Milestone | Description | Depends On | Status |
|----|-----------|-------------|------------|--------|
| **G1** | Proper ECS with Archetype Storage | Beyond the current demo. Real archetype-based component storage. | R5, MT-5, F4 | **DONE** |
| **G2** | Input System | Keyboard/mouse from Win32/Vulkan surface events. | R1 | **DONE** |
| **G3** | Audio | Basic WAV playback via C runtime. | -- | **DONE** |
| **G4** | Hot Reload | Recompile functions while program runs. | F6 | **DONE** |
| **G5** | Killer Demo v2 | 10K particles with real Vulkan + Lux shaders + parallel jobs. | All above | **DONE** |

## Track 8: Self-Improvement (Ongoing)

| ID | Milestone | Description | Depends On | Status |
|----|-----------|-------------|------------|--------|
| **S1** | Self-Hosted Parser | AXIOM parser written in AXIOM. | F2, F3, F4 | **DONE** |
| **S2** | Compiler Self-Optimization | Compiler uses LLM loop to optimize its own hot paths. | L4 | **DONE** |
| **S3** | Source-to-Source AI Optimizer | LLM rewrites AXIOM source (not just ?params). `axiom rewrite`. | L1, L2 | **DONE** |

---

## Execution Order (Recommended)

```
Phase A: MT-1 (fix UB -- URGENT)                                          DONE
Phase B: MT-2, MT-3 (parallel_for + reductions -- core MT)                DONE
Phase C: L1, L3, P1, P4 (constraints + consteval + platform)             DONE
Phase D: MT-4, MT-5, MT-6 (ownership + deps + LLVM metadata)             DONE
Phase E: F1, F2, F3, F5 (sum types, strings, dynamic arrays, closures)   DONE
Phase F: L2, P2, P3 (hardware counters, CPUID, SIMD)                     DONE
Phase G: F4, F6, F7, F8 (generics, modules, errors, pattern matching)    DONE
Phase H: E1, E2, E3 (CI, debug info, formatter)                          DONE
Phase I: R1-R5 (real Vulkan renderer)                                     DONE
Phase J: G1-G5 (game engine)                                              DONE
Phase K: S1-S3 (self-improvement)                                         DONE
```

**Total: 47 milestones across 8 tracks.**
**Completed: 47/47 milestones (ALL PHASES COMPLETE).**
**Each milestone goes through the full 7-agent pipeline.**
