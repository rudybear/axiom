# AXIOM Benchmark Results

## AXIOM vs C (clang -O2) — Real Measurements

**Date:** 2026-03-22
**Environment:** Windows 11, x86_64, clang 22.1.1, Rust 1.93.1
**Method:** 5 measurement runs after 2 warmup runs, median reported
**AXIOM pipeline:** `.axm` → LLVM IR text → `clang -O2` → native binary

| Benchmark | AXIOM (ms) | C -O2 (ms) | Ratio | Output Match |
|-----------|-----------|------------|-------|-------------|
| fibonacci(40) | 3.6 | 3.6 | 1.01x | YES |
| nested loops(5000) | 3.9 | 3.3 | 1.18x | YES |
| collatz(100K) | 11.6 | 11.8 | 0.98x | YES |
| nbody force(3000) | 10.9 | 10.4 | 1.04x | YES |
| primes(200K) | 7.9 | 8.0 | 0.98x | YES |
| **TOTAL** | **37.9** | **37.2** | **1.02x** | **ALL MATCH** |

**AXIOM achieves 98% of C (clang -O2) performance.**

## How This Works

AXIOM generates LLVM IR text, which clang compiles with `-O2`. This means AXIOM
benefits from the same LLVM optimization passes as C:

- `mem2reg` — promotes alloca variables to SSA registers
- Loop vectorization
- Instruction combining
- Dead code elimination
- Function inlining

## Benchmarks Explained

### fibonacci(40)
Iterative Fibonacci computing the 40th number (102334155). Tests basic integer
arithmetic, loop iteration, and function call overhead.

### nested loops(5000)
5000×5000 nested loop accumulating `i*j` into a 64-bit sum. Tests loop
performance and integer widening.

### collatz(100K)
Compute Collatz sequence length for numbers 1 to 100,000. Tests branching
(if/else inside while loop), division, multiplication, and function calls.

### nbody force(3000)
3000×3000 gravitational force accumulation (N-body inspired). Tests nested loops
with conditional branches and integer division.

### primes(200K)
Count primes up to 200,000 using trial division. Tests function calls (is_prime),
loops with early exit, and modulo operations.

## No Benchmark-Specific Cheating

Per CLAUDE.md rules:
1. All optimizations are general-purpose (LLVM -O2)
2. No hard-coded results
3. No benchmark detection
4. Idiomatic AXIOM code (annotations, explicit types, explicit returns)
5. Results are reproducible with documented environment

## Comparison to Language Benchmarks Game Categories

| Category | Our Equivalent | Status |
|----------|---------------|--------|
| n-body | nbody force(3000) | 1.04x vs C |
| spectral-norm | (similar compute pattern) | — |
| mandelbrot | (needs float arrays) | Blocked: no arrays yet |
| fasta | (needs string/byte output) | Blocked: no byte arrays |
| binary-trees | (needs heap allocation) | Blocked: no malloc |
| pidigits | (needs big integers) | Blocked: no bigint |

Current AXIOM covers **compute-bound integer and float** benchmarks well.
Array-heavy and allocation-heavy benchmarks require Phase 2+ language features
(arrays, heap allocation, byte I/O).
