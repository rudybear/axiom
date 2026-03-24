# Frame Allocator Demo

Demonstrates a **triple-buffered arena** pattern for zero-malloc game loops,
a standard technique in AAA game engines.

## Concept

In real-time games, per-frame allocations (malloc/free) cause:
- Unpredictable latency spikes from heap fragmentation
- Cache thrashing from non-contiguous allocations
- Memory leaks if allocations are not correctly paired

The solution: pre-allocate three arenas and rotate them each frame.

```
Frame 0  ->  arena0  (allocate scratch data)
Frame 1  ->  arena1  (allocate scratch data)
Frame 2  ->  arena2  (allocate scratch data)
Frame 3  ->  arena0  (reset + reuse -- frames 1,2 are done reading)
Frame 4  ->  arena1  (reset + reuse)
...
```

Each `arena_reset()` is O(1) -- it just resets the bump pointer. All
per-frame allocations (`arena_alloc`) are O(1) pointer bumps. Zero heap
traffic during the game loop.

## Architecture

```
[ arena0: 1 MB ]   [ arena1: 1 MB ]   [ arena2: 1 MB ]
  |-- positions[1000]    (8000 bytes)
  |-- velocities[1000]   (8000 bytes)
  |-- commands[500]       (2000 bytes)
  Total per frame: 18 KB of 1 MB arena
```

### Systems

| Function | Description | Pure |
|----------|-------------|------|
| `simulate_physics` | Write positions and velocities | Yes |
| `process_input` | Fill command buffer | Yes |
| `frame_checksum` | Verify data integrity | Yes |

## Running

```bash
# Compile to LLVM IR (verification)
cargo run -p axiom-driver -- compile examples/game_loop/frame_alloc_demo.axm --emit=llvm-ir

# Compile and run
cargo run -p axiom-driver -- compile examples/game_loop/frame_alloc_demo.axm -o frame_alloc_demo
./frame_alloc_demo
```

## Key Metrics

- **300 frames** simulated
- **900 arena allocs** (3 per frame), **0 heap allocs**
- **1000 entities** with position + velocity per frame
- **500 input commands** per frame
- All scratch memory recycled via arena reset
