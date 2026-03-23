# N-Body Simulation

A 5-body gravitational simulation implemented in AXIOM using 35 scalar `f64`
variables (x, y, z, vx, vy, vz, mass for each body).

## nbody.axm

Simulates gravitational interactions between 5 bodies for 1000 time steps
using a simple Euler integration scheme. All 10 unique pairs are computed
explicitly (no arrays of bodies — pure scalar computation).

### Bodies
- Body 0: Sun-like central mass (m=1000)
- Body 1: Inner planet (m=1)
- Body 2: Outer planet (m=2)
- Body 3: Inclined orbit body (m=0.5)
- Body 4: Distant body (m=1.5)

### Output
Prints the total kinetic energy at the end of the simulation as a checksum.

## Running

```bash
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/nbody/nbody.axm
```

## Language Features Demonstrated

- Heavy floating-point computation with `f64`
- `sqrt` builtin for distance calculations
- `@pure` annotation on the simulation function
- Large function with many local variables
- `for ... in range()` loop for time stepping
