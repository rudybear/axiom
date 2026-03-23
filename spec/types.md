# AXIOM Type System Specification

Version 0.1 -- matches implemented HIR as of 2026-03-23.
Source of truth: `crates/axiom-hir/src/hir.rs` (`HirType`, `PrimitiveType`).

## Guiding Principles

1. **Every type is explicit.** There is no type inference. Every variable
   binding, function parameter, and return type carries a written type.
2. **No implicit conversions.** Widening, narrowing, and truncation are named
   operations (`widen`, `narrow`, `truncate`). The compiler never silently
   converts between types.
3. **No operator overloading.** The `+` operator always means numeric
   addition on primitive types. Compound operations use named methods
   (e.g., `tensor.add(a, b)`).

## Primitive Types

All primitive types are value types. They have a fixed size, well-defined
representation, and no heap allocation.

### Signed integers

| Type   | Size    | Range |
|--------|---------|-------|
| `i8`   | 8 bits  | -128 to 127 |
| `i16`  | 16 bits | -32768 to 32767 |
| `i32`  | 32 bits | -2^31 to 2^31 - 1 |
| `i64`  | 64 bits | -2^63 to 2^63 - 1 |
| `i128` | 128 bits| -2^127 to 2^127 - 1 |

### Unsigned integers

| Type   | Size    | Range |
|--------|---------|-------|
| `u8`   | 8 bits  | 0 to 255 |
| `u16`  | 16 bits | 0 to 65535 |
| `u32`  | 32 bits | 0 to 2^32 - 1 |
| `u64`  | 64 bits | 0 to 2^64 - 1 |
| `u128` | 128 bits| 0 to 2^128 - 1 |

### Floating-point

| Type   | Size    | Standard |
|--------|---------|----------|
| `f16`  | 16 bits | IEEE 754 half-precision |
| `bf16` | 16 bits | Brain floating-point (Google BFloat16) |
| `f32`  | 32 bits | IEEE 754 single-precision |
| `f64`  | 64 bits | IEEE 754 double-precision |

### Boolean

| Type   | Size   | Values |
|--------|--------|--------|
| `bool` | 1 byte | `true`, `false` |

## Compound Types

### Tensor

Syntax: `tensor[ElementType, Dim0, Dim1, ...]`

Multi-dimensional dense array. Each dimension is one of:
- A constant integer: `tensor[f32, 1024, 1024]`
- A named generic dimension: `tensor[f32, M, N]`
- A dynamic (runtime) dimension: `tensor[f32, ?]` (using `?` / `?name` hole)

Element type must be a primitive type. Tensors carry layout metadata via
`@layout` and alignment via `@align`.

### Array

Syntax: `array[ElementType, Length]`

Fixed-size, stack-allocated array. The length must be a compile-time constant
expression. Element type may be any type (primitive or user-defined).

A zero-initialized array literal is written `array_zeros[ElementType, Length]`.

### Slice

Syntax: `slice[ElementType]`

Fat pointer consisting of a pointer and a length. Provides bounds-checked
access to a contiguous region of memory. Element type may be any type.

### Pointer

Syntax: `ptr[ElementType]`

Raw pointer. Intended for use inside `unsafe` blocks. No bounds checking.
Element type may be any type.

### Tuple

Syntax: `(T1, T2, T3)`

Fixed-size heterogeneous product type. The parser distinguishes tuples from
parenthesized grouping by the presence of a comma after the first element.

### Function Type

Syntax: `fn(ParamType1, ParamType2) -> ReturnType`

First-class function type. Used for function pointers and closures (when
those are supported). Parameter types and return type may be any type.

## User-Defined Types

### Structs

Defined with `struct Name { field: Type, ... }`. Fields may carry `@layout`
and `@align` annotations. Struct names are registered during the first pass
of HIR lowering and can be used as types throughout the module.

### Type Aliases

Defined with `type Name = TypeExpr;`. The alias name is interchangeable with
the aliased type. Lowering resolves the alias to the underlying type.

## Type Validation

During AST-to-HIR lowering, every type reference is validated:

1. Primitive type names are resolved to `PrimitiveType` enum variants.
2. User-defined type names (structs and type aliases collected in the first
   pass) are resolved to `HirType::UserDefined`.
3. Unknown type names produce a `LowerError::UnknownType` but lowering
   continues using `HirType::Unknown` for error recovery.
4. Compound types (tensor, array, slice, ptr, tuple, fn) recursively
   validate their inner types.

## Explicit Conversion Functions

AXIOM provides three built-in conversion functions that are parsed as
keywords. They are the only way to convert between numeric types.

| Function     | Purpose |
|-------------|---------|
| `widen(x)`  | Convert to a wider type (e.g., i32 to i64). Always lossless. |
| `narrow(x)` | Convert to a narrower type (e.g., i64 to i32). May lose precision. |
| `truncate(x)` | Truncate floating-point to integer, or discard high bits. |

These functions are parsed as call expressions with a single argument. The
target type is determined by the context (the expected type from the
enclosing `let` binding or function parameter).

## Integer Literal Suffixes

Integer literals may carry a type suffix to specify their width:
`42i32`, `0xFFu8`, `0b1010i16`. Without a suffix, the type is inferred
from context (this is the one place where type context matters, but the
binding or parameter still has an explicit type annotation).

Supported suffixes: `i8`, `i16`, `i32`, `i64`, `i128`, `u8`, `u16`, `u32`,
`u64`, `u128`.

## Float Literal Suffixes

Float literals may carry a type suffix: `3.14f32`, `2.0f64`.

Supported suffixes: `f16`, `bf16`, `f32`, `f64`.

## Wrapping and Saturating Arithmetic

AXIOM provides explicit operators for wrapping and saturating arithmetic to
avoid undefined behavior on overflow:

| Operator | Meaning |
|----------|---------|
| `+%`     | Wrapping add |
| `+\|`    | Saturating add |
| `-%`     | Wrapping subtract |
| `-\|`    | Saturating subtract |
| `*%`     | Wrapping multiply |

Standard `+`, `-`, `*` follow target-defined overflow behavior (typically
two's complement wrapping for integers, IEEE 754 for floats).
