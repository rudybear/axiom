# AXIOM Generics — Type-Suffixed Builtins Approach

## Design Decision

AXIOM deliberately avoids parametric polymorphism (e.g., `fn push<T>(vec: Vec<T>, val: T)`)
in favor of **explicit type-suffixed builtins**. This is a pragmatic design choice that
trades some surface-level convenience for significant implementation simplicity and
predictable performance.

## Rationale

### Why not full generics with monomorphization?

Full generics (as in Rust or C++) require:

1. **Type inference** — resolving generic type parameters at call sites.
2. **Monomorphization** — generating specialized code for each concrete type instantiation.
3. **Trait/concept bounds** — constraining type parameters to ensure operations are valid.
4. **Lifetime/ownership interactions** — generic types interacting with ownership rules.
5. **Error reporting** — producing useful error messages when generic constraints fail.

Each of these is a major compiler subsystem. For AXIOM's domain (numerical computing,
game engines, AI optimization), the type universe is small and well-known: `i32`, `i64`,
`f64`, and `ptr`. Adding a generic system would add thousands of lines of compiler code
for marginal benefit.

### Type-suffixed builtins: the AXIOM approach

Instead, AXIOM provides explicitly typed variants of polymorphic operations:

```
// Pointer read/write — one variant per element type
ptr_read_i32(ptr, offset) -> i32
ptr_read_i64(ptr, offset) -> i64
ptr_read_f64(ptr, offset) -> f64
ptr_write_i32(ptr, offset, value)
ptr_write_i64(ptr, offset, value)
ptr_write_f64(ptr, offset, value)

// Vec (dynamic array) — one variant per element type
vec_push_i32(vec, value)
vec_push_f64(vec, value)
vec_get_i32(vec, index) -> i32
vec_get_f64(vec, index) -> f64
vec_set_i32(vec, index, value)
vec_set_f64(vec, index, value)

// Function pointers — one variant per return type
call_fn_ptr_i32(fn_ptr, arg) -> i32
call_fn_ptr_f64(fn_ptr, arg) -> f64

// Printing — one variant per type
print(msg)          // string
print_i32(value)    // 32-bit integer
print_i64(value)    // 64-bit integer
print_f64(value)    // 64-bit float

// Math — separate integer and float variants
abs(x)     -> i32   // integer absolute value
abs_f64(x) -> f64   // float absolute value
min(a, b)     -> i32
min_f64(a, b) -> f64
max(a, b)     -> i32
max_f64(a, b) -> f64
```

## Advantages

1. **Zero-cost abstraction** — no monomorphization pass; each call compiles directly to
   the correct LLVM IR instruction with the correct type.

2. **Explicit types in code** — reading `vec_push_i32(v, 42)` immediately tells you the
   element type. No need to trace type inference to understand what code is generated.

3. **Simple codegen** — each builtin maps to a single `emit_builtin_*` function in the
   codegen backend. No template instantiation, no type substitution.

4. **Predictable performance** — no risk of unexpected code bloat from monomorphization.
   What you write is what you get.

5. **Fast compilation** — no generic resolution phase. The compiler remains fast even on
   large codebases.

## When would full generics be added?

Full generics may be considered if:

- User-defined container types become common (beyond the built-in `Vec`).
- Higher-order abstractions (map/filter/reduce over generic collections) are needed.
- The compiler team has bandwidth for a type inference + monomorphization subsystem.

Until then, the type-suffixed approach serves AXIOM's numerical/systems domain well.

## Adding new type variants

To add a new type suffix (e.g., `vec_push_i64`):

1. Add the C runtime function in `axiom_rt.c`.
2. Add the builtin match arm in `axiom-codegen/src/llvm.rs` (the `emit_builtin_*` dispatch).
3. Add the LLVM IR `declare` in the `needs_vec` / `needs_runtime` section.
4. Update the `needs_runtime()` detection function.

This is intentionally mechanical and low-risk.
