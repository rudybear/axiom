# Self-Optimization Pass 2 — Learnings

**Date:** 2026-03-24
**Result:** eigenvalue 1.41x→1.04x, ray_tracer 1.18x→1.07x. Zero C wins remaining.

## What Worked

### @inline(always) on hot @pure helpers enables loop fusion
The eigenvalue benchmark called `matvec_multiply` (500x500 = 250K multiply-adds) 200 times inside `power_iteration`. Without inlining, each call is a function boundary that prevents LLVM from fusing the matvec with the subsequent normalization. With `@inline(always)`, LLVM sees the entire 200-iteration loop as one block and can:
- Fuse the matvec and normalize operations
- Keep intermediate results in registers (no array write+read between stages)
- Apply global scheduling across the entire iteration

### Full inlining for ray tracers
Ray tracing per-pixel functions have deep call chains (trace_pixel → ray_sphere_t × 3 spheres → dot3 × multiple → sqrt). Without inlining, each call is opaque to the optimizer. With full inlining, LLVM can:
- Constant-fold sphere parameters (they're the same for all pixels)
- CSE across multiple ray-sphere tests
- Hoist invariant computations out of the pixel loop

## New Knowledge Base Rule

### Rule 14: @inline(always) on ALL hot path functions, not just small helpers
Previous guidance (Rule 5b) said "small helpers < 10 lines". This is too conservative. Even large functions (50+ lines) like `matvec_multiply` or `trace_pixel` should be `@inline(always)` when:
- Called in a tight loop (100+ iterations)
- The function is the ONLY thing in the loop body
- The caller processes independent elements (no loop-carried dependency through the function)

The code size increase is worth it — LLVM's backend handles large inlined functions well with its register allocator and instruction scheduler.

## Summary of Full Self-Optimization Journey

| Metric | Start | After Pass 1 | After Pass 2 |
|--------|-------|-------------|-------------|
| AXIOM wins | 2 | 2 | 2 |
| Ties | 9 | 16 | 18 |
| C wins | 9 | 2 | 0 |
| Total ratio | 0.97x | 0.94x | ~0.94x |
| Biggest win | JPEG 56% | JPEG 57% | JPEG 58% |
