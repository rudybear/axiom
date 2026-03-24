# Self-Improving Optimization Demo

Demonstrates the AXIOM profiling and self-optimization workflow.

## Concept

AXIOM programs can contain `@strategy` blocks with `?holes` -- tunable
parameters that an AI agent or the built-in profiler can optimize. The
`axiom profile` command automates the feedback loop:

1. **Compile** the program
2. **Run** it N times, collecting timing data
3. **Analyze** the results (min, max, mean, median, stddev)
4. **Extract** optimization surfaces from `@strategy` blocks
5. **Suggest** which `?params` to tune and in what direction

## Files

| File | Description |
|------|-------------|
| `optimize_demo.axm` | Tunable matrix-vector multiply with timing |

## Running

```bash
# Compile to LLVM IR (verification)
cargo run -p axiom-driver -- compile examples/self_opt/optimize_demo.axm --emit=llvm-ir

# Profile the program (compile + run + analyze)
cargo run -p axiom-driver -- profile examples/self_opt/optimize_demo.axm --iterations=10

# Full optimization loop (extracts surfaces, proposes values, benchmarks)
cargo run -p axiom-driver -- optimize examples/self_opt/optimize_demo.axm --iterations=3
```

## Profiler Output

The `axiom profile` command prints:

```
Profiling examples/self_opt/optimize_demo.axm (10 iterations)...

  Compilation: OK (0.42s)

  Timing (10 runs):
    min:    12.3 ms
    max:    14.1 ms
    mean:   12.8 ms
    median: 12.6 ms
    stddev:  0.5 ms

  Optimization surfaces:
    (none in current source -- add @strategy blocks to enable tuning)

  Suggestions:
    - Program is deterministic (checksum stable across runs)
    - Consider adding @strategy { block_size: ?bs } to matvec() for tiling
```

## Architecture

The demo exercises:
- Arena allocation for matrix storage (zero heap traffic in hot loop)
- `clock_ns()` for precise nanosecond timing
- `@pure` functions for optimizer-friendly code
- Deterministic output for regression testing
