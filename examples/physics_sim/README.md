# 2D Circle Collision Simulation

Simulates 500 circles bouncing inside a 1000x1000 box with elastic circle-circle collision detection and response.

## How It Works

1. **Initialization** -- 500 circles with random positions, velocities, and radii (2-10 units)
2. **Integration** -- Euler integration of positions from velocities
3. **Collision Detection** -- O(n^2) pairwise check for overlapping circles
4. **Collision Response** -- Elastic collision impulse exchange along collision normal, with mass proportional to radius squared
5. **Wall Bounce** -- Reflect velocity when circles touch the bounding box

## Features Used

- `@module`, `@intent`, `@pure` annotations
- Arena allocation (single 1 MB block, zero per-frame mallocs)
- SOA layout for cache-friendly iteration
- `sqrt` for distance calculation
- LCG pseudo-random number generator

## Physics

The collision response uses the elastic collision formula:
- Impulse = `2 * relative_velocity_along_normal / (mass_i + mass_j)`
- Overlapping circles are separated by pushing them apart along the collision normal

## Run

```bash
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/physics_sim/collision_2d.axm
```
