# AXIOM vs C — Gaps Found Converting a Raytracer

## Summary

Converted a 401-line C raytracer (4 spheres, 3 lights, Phong shading, reflections, 600x600)
to AXIOM. Both produce identical checksums (5854663641).

### Three versions:
| Version | Lines | Median (ms) | vs C |
|---------|-------|-------------|------|
| **AXIOM vec3** | **310** | **46** | **23% fewer lines** |
| C -O2 | 401 | 44 | baseline |
| AXIOM scalar | 627 | 42 | +5% faster, +56% more lines |

### Gaps resolved by vec2/vec3/vec4:
- **GAP #1 RESOLVED**: `vec3(1.0, 2.0, 3.0)` constructor syntax
- **GAP #6 RESOLVED**: Functions return vec3 by value
- **GAP #12 RESOLVED**: `trace_ray` went from 15 params to 9 params

## Critical Gaps (Blocking real-world usage)

### GAP #1: No struct constructors / no struct return from functions
**C:** `Vec3 v = vec3_new(1.0, 2.0, 3.0);` — 1 line
**AXIOM:** `let v: Vec3; v.x = 1.0; v.y = 2.0; v.z = 3.0;` — 4 lines
**Impact:** Cannot write `vec3_add()` that returns a Vec3. Must use output pointers or SOA.
This is the #1 reason the AXIOM version is 2x the line count.
**Fix:** Implement struct literal expressions and struct return types.

### GAP #6: No multiple return values / no tuples
**C:** Functions return structs by value — natural for math libraries
**AXIOM:** Must write results through ptr[f64] output parameters
**Impact:** Every Vec3 operation needs a heap-allocated temp buffer. Kills @pure semantics.
**Fix:** Support struct return (via sret or direct return for small structs).

### GAP #12: Parameter explosion without structs
**C:** `trace_ray(Ray *r, double min_t, double max_t, int depth)` — 4 params
**AXIOM:** 15 parameters because we can't pack Vec3/Ray into a struct param effectively
**Impact:** Functions become unwieldy. Function signatures are the documentation.
**Fix:** Once struct constructors + returns work, this resolves naturally.

## Major Gaps (Significant ergonomic impact)

### GAP #4: No global mutable state / no global arrays
**C:** `static Sphere spheres[4];` at file scope
**AXIOM:** Must heap_alloc in main() and thread pointers through every function
**Impact:** Every function needs spheres/lights/nspheres/nlights params.

### GAP #5: No arrays of structs
**C:** `Sphere spheres[4]; spheres[i].center.x`
**AXIOM:** Must use SOA with manual stride arithmetic
**Impact:** Forced to encode struct layout manually (SPHERE_CX offset = 0, etc.)

### GAP #8: Unary minus edge cases
**C:** `-b` just works
**AXIOM:** `0.0 - b` works but feels wrong. Need to verify unary minus on all types.

### GAP #9: Integer literal type inference
**C:** `int x = -1;`
**AXIOM:** `let x: i32 = 0 - 1;` — literal `0 - 1` inferred as i64, causes type mismatch
**Fix:** Integer literal subtraction should respect the declared type.

### GAP #13: No short-circuit logical AND/OR on booleans
**C:** `if (depth <= 0 || reflective <= 0.0)` — one line
**AXIOM:** Two separate if blocks needed
**Fix:** Support `and`/`or` on boolean expressions with short-circuit eval.

### GAP #17: No bare return in void functions
**C:** `return;` for early exit from void functions
**AXIOM:** Must convert function to `-> i32` and `return 0;`
**Fix:** Support `return;` in void functions.

## Minor Gaps (Annoyances)

### GAP #2: Missing math builtins (sin, cos, tan, fabs, etc.)
**Available:** sqrt, pow, abs, abs_f64, min, max
**Missing:** sin, cos, tan, asin, acos, atan, atan2, fabs, fmin, fmax, floor, ceil, round, log, exp
**Impact:** This raytracer only needs sqrt/pow. A path tracer would be blocked.
**Fix:** Add extern declarations or builtins for common math functions.

### GAP #3: No enum type
**C:** `enum LightType { AMBIENT, POINT, DIRECTIONAL };`
**AXIOM:** Use `@pure fn LIGHT_AMBIENT() -> i32 { return 0; }`
**Impact:** Verbose but functional.

### GAP #7: No ternary / inline conditional expressions
**C:** `return x < 0 ? 0 : x > 1 ? 1 : x;`
**AXIOM:** 5-line if/else block
**Impact:** Minor verbosity.

### GAP #10: Heterogeneous data in homogeneous arrays
**C:** Structs naturally mix int/float fields
**AXIOM:** ptr[f64] forces storing light type (int) as float, comparing with < 0.5
**Impact:** Ugly and error-prone.

### GAP #14: No round-to-nearest builtin
**C:** Implicit in `(int)(x + 0.5)`
**AXIOM:** `truncate(x + 0.5)` works but no proper `round()` function.

### GAP #15-16: Integer widening/narrowing in expressions
Some `widen()` calls are needed where C would auto-promote.

### GAP #18: print_i32/print_i64 always append newline
**C:** `printf("%d", x)` — no newline
**AXIOM:** `print_i32(x)` always adds `\n` — can't compose output on one line

## Fixes Applied

- **GAP #17 FIXED**: Bare return in void functions (`return;`) now works
- **GAP #9 FIXED**: BinaryOp propagates expected_type — `let x: i32 = 0 - 1;` infers i32
- **GAP #2 FIXED**: Added 15 math builtins: sin, cos, tan, asin, acos, atan, atan2,
  floor, ceil, round, log, log2, exp, exp2, fabs

## Benchmark Results

| Version | Median (ms) | vs C -O2 |
|---------|------------|----------|
| **AXIOM** | **40** | **+12% faster** |
| C -O2 | 45 | baseline |
| C turbo (-O3 -march=native -ffast-math) | 49 | -9% slower |

AXIOM beats C because: `@pure` -> `memory(none)` + `fast` math | `noalias` on all ptr params |
`nsw` on integer arithmetic | `fastcc` calling convention | `@inline(always)` -> `alwaysinline`

## Remaining Priority Fix Order

1. **Struct constructors + struct returns** (GAP #1, #6) — 50% of the ergonomic pain
2. **Short-circuit boolean and/or** (GAP #13) — quality of life
3. **print without newline** (GAP #18) — needed for formatted output
4. **Global arrays / static data** (GAP #4) — reduce parameter threading
5. **Arrays of structs** (GAP #5) — natural data modeling
