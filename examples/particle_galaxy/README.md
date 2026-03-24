# Particle Galaxy -- Killer Demo

The crowning demonstration of AXIOM's game engine phase, combining every
capability developed across Milestones 7.1 through 7.9.

## What It Does

Simulates a **10,000-particle spiral galaxy** with gravitational physics,
a rendering pipeline, zero per-frame allocations, and parallel job
infrastructure -- all in pure AXIOM source code.

## Features Demonstrated

| Feature | Milestone | How It's Used |
|---------|-----------|---------------|
| Arena allocation | M7.1 | 8 MB arena for particle SOA data |
| SOA layout | M7.2 | x[], y[], vx[], vy[], mass[], alive[] arrays |
| Pure systems | M7.3 | `@pure` physics, integration, damping functions |
| Parallel jobs | M7.4 | `jobs_init(4)` / `jobs_shutdown()` worker pool |
| Renderer API | M7.5 | `renderer_create`, `begin_frame`, `end_frame` lifecycle |
| Frame allocator | M7.7 | 1 MB frame arena, reset each frame for scratch data |
| Clock timing | M7.6 | `clock_ns()` for precise elapsed-time measurement |

## Architecture

### Memory Layout

```
[ Persistent Arena: 8 MB ]
  |-- x[10000]       80,000 bytes
  |-- y[10000]       80,000 bytes
  |-- vx[10000]      80,000 bytes
  |-- vy[10000]      80,000 bytes
  |-- mass[10000]    80,000 bytes
  |-- alive[10000]   40,000 bytes
  |-- seed[1]             8 bytes
  Total: ~440 KB of 8 MB arena

[ Frame Arena: 1 MB ] (reset each frame)
  |-- fx[10000]      80,000 bytes
  |-- fy[10000]      80,000 bytes
  Total: 160 KB of 1 MB arena
```

### Systems Pipeline (per frame)

```
1. arena_reset(frame_arena)           -- O(1)
2. allocate fx[], fy[]                -- O(1) bump alloc
3. compute_center_of_mass_x/y()       -- O(n)
4. system_gravity()                   -- O(n) (COM approximation)
5. system_damping()                   -- O(n)
6. system_integrate()                 -- O(n)
7. renderer_begin_frame / end_frame   -- O(1)
8. despawn out-of-bounds particles    -- O(n)
```

Total per-frame complexity: O(n) where n = 10,000.
Total heap allocations in game loop: **0**.

### Physics Model

Uses a center-of-mass (COM) approximation for O(n) per-frame cost instead
of the O(n^2) all-pairs approach. Each particle is attracted toward the
galaxy's center of mass with:

```
force = G * mass / (distance^2 + softening)
```

Velocity damping (factor 0.9999) prevents energy buildup.

### Initialization

Particles are placed in a spiral galaxy pattern using:
- LCG pseudo-random angle and radius
- Spiral arm modulation: `r = radius * (1 + angle / 2*pi)`
- Taylor-series sin/cos approximation for position
- Tangential velocity for orbital motion

## Running

```bash
# Compile to LLVM IR (verification)
cargo run -p axiom-driver -- compile examples/particle_galaxy/particle_galaxy.axm --emit=llvm-ir

# Compile and run
cargo run -p axiom-driver -- compile examples/particle_galaxy/particle_galaxy.axm -o particle_galaxy
./particle_galaxy

# Profile performance
cargo run -p axiom-driver -- profile examples/particle_galaxy/particle_galaxy.axm --iterations=5
```

## Expected Output

```
Particle Galaxy: 10000 particles, 100 frames
Initial alive particles:
10000
[renderer lifecycle messages from stub backend]
Final alive particles:
<count>
Position checksum:
<f64 value>
Elapsed (ns):
<timing>
Frames rendered:
100
Particle galaxy complete.
```

The checksum and alive count are deterministic given the fixed LCG seed (42).
