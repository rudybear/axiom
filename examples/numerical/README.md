# Numerical Methods Examples

This directory contains implementations of classic numerical methods in AXIOM.

## Programs

### pi.axm
Computes Pi using three different series:
- **Leibniz series**: Pi/4 = 1 - 1/3 + 1/5 - 1/7 + ... (slow convergence)
- **Wallis product**: Pi/2 = (2/1)(2/3)(4/3)(4/5)... (product formula)
- **BBP formula**: Bailey-Borwein-Plouffe digit extraction (fast convergence)

### roots.axm
Root-finding algorithms:
- **Newton-Raphson**: Quadratic convergence for smooth functions.
  Finds sqrt(2) and the root of x^3 - x - 2.
- **Bisection**: Linear convergence but guaranteed on bracketed intervals.
  Finds sqrt(2) on [1, 2].

### integration.axm
Numerical integration (quadrature):
- **Midpoint rule**: First-order accurate.
- **Trapezoidal rule**: Second-order accurate.
- **Simpson's rule**: Fourth-order accurate.
Integrates x^2 on [0,1] (exact = 1/3) and 4/(1+x^2) on [0,1] (exact = Pi).

## Running

```bash
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/numerical/pi.axm
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/numerical/roots.axm
cargo run -p axiom-driver -- compile --emit=llvm-ir examples/numerical/integration.axm
```

## Language Features Demonstrated

- `f64` floating-point arithmetic
- `to_f64` builtin for int-to-float conversion
- `abs_f64`, `sqrt`, `pow` math builtins
- `@pure` and `@complexity` annotations
- Iterative algorithms with convergence checks
