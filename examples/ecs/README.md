# ECS (Entity-Component-System) Framework Demo

An ECS design pattern implemented entirely in AXIOM source code, demonstrating
that the language is expressive enough for data-oriented game architecture.

## What is ECS?

Entity-Component-System is the dominant architecture in modern game engines
(Unity DOTS, Bevy, Flecs, EnTT). It separates data (Components) from logic
(Systems) and uses integer handles (Entities) rather than OOP inheritance.

Key properties:
- **Cache-friendly**: SOA layout keeps hot data contiguous
- **Parallelizable**: Systems are pure functions over disjoint data
- **Zero-alloc game loop**: Arena allocation eliminates per-frame malloc

## Files

| File | Description |
|------|-------------|
| `ecs_demo.axm` | Full ECS demo (10K entities, 1000 ticks, chunked physics) |
| `ecs_benchmark.axm` | Benchmark version with `clock_ns()` timing |
| `ecs_benchmark.c` | C equivalent for performance comparison |

## Architecture

### Components (SOA Layout)

```
x[N]     : f64   position X
y[N]     : f64   position Y
vx[N]    : f64   velocity X
vy[N]    : f64   velocity Y
alive[N] : i32   1 = active, 0 = despawned
```

All five arrays are allocated contiguously from a single 4 MB arena. This
gives better cache utilization than AOS (Array of Structs) because each
system only touches the components it needs.

### Systems

| System | Reads | Writes | Pure |
|--------|-------|--------|------|
| `system_physics` | vx, vy, alive | x, y | Yes |
| `system_bounce` | x, y, alive | x, y, vx, vy | Yes |
| `entity_spawn` | — | x, y, vx, vy, alive | Yes |
| `entity_despawn` | — | alive | Yes |

All systems are `@pure`, meaning they have no side effects outside the arrays
they explicitly receive as arguments. This makes them trivially parallelizable.

### Memory Model

```
[ arena: 4 MB ]
  |-- x[10000]      80,000 bytes
  |-- y[10000]      80,000 bytes
  |-- vx[10000]     80,000 bytes
  |-- vy[10000]     80,000 bytes
  |-- alive[10000]  40,000 bytes
  |-- seed[1]            8 bytes
  Total: ~360 KB of 4 MB arena
```

Zero heap allocations during the game loop. Arena is created once and
destroyed once.

### Parallelism

The physics system is split into `num_chunks` independent ranges via
`physics_chunk()`. Each chunk processes a disjoint slice of the arrays and
could be dispatched to separate threads via `job_dispatch()`. The current
implementation calls them sequentially but marks where parallelism applies.

AXIOM's `job_dispatch(fn, data, n)` takes a function with signature
`(ptr, i32, i32)`. Since the physics system needs multiple array pointers,
full parallel dispatch requires packing pointers into a parameter block.
This will be supported once pointer-in-struct writing is available.

## Running

```bash
# Compile to LLVM IR (verification)
cargo run -p axiom-driver -- compile examples/ecs/ecs_demo.axm --emit=llvm-ir

# Compile and run
cargo run -p axiom-driver -- compile examples/ecs/ecs_demo.axm -o ecs_demo
./ecs_demo

# Benchmark
cargo run -p axiom-driver -- compile examples/ecs/ecs_benchmark.axm -o ecs_bench
./ecs_bench

# C comparison
gcc -O2 -o ecs_benchmark_c examples/ecs/ecs_benchmark.c -lm
./ecs_benchmark_c
```

## Performance Expectations

The AXIOM version should be within 1-2x of the C version since both compile
to native code via LLVM. The SOA layout ensures both versions get good
cache behavior on the physics and bounce inner loops.
