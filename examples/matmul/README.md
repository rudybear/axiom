# Matrix Multiply Optimization Demo

This example demonstrates the AXIOM optimization protocol applied to a
matrix-multiplication kernel.  The protocol has four stages:

1. **Surface extraction** -- parse an AXIOM source file and discover every
   tunable `?hole` inside `@strategy` blocks.
2. **Proposal** -- an AI agent (or human) fills the holes with concrete values.
3. **Validation** -- the compiler checks types and ranges before applying
   the proposal.
4. **History** -- each optimization attempt is recorded so the next iteration
   can learn from previous results.

## Source files

| File | Purpose |
|------|---------|
| `tests/samples/matmul_naive.axm` | Full tensor-based matmul with 7 optimisation holes (tiling, loop order, parallelism, unrolling, prefetching). |
| `tests/samples/matmul_simple.axm` | Simplified scalar loop nest with a single `?unroll_factor` hole; compiles end-to-end through the AXIOM codegen pipeline. |

## Running the integration tests

```bash
cargo test -p axiom-optimize -- matmul --nocapture
```

This runs three integration tests:

- **`test_matmul_naive_surfaces`** -- extracts surfaces from the full
  `matmul_naive.axm` and verifies that all 7 `?holes` are discovered with
  correct types and ranges.
- **`test_matmul_simple_surfaces`** -- extracts the single `?unroll_factor`
  hole from `matmul_simple.axm`.
- **`test_optimization_flow`** -- exercises the complete optimisation protocol:
  surface extraction, proposal creation, proposal validation, and history
  recording.

## Optimization holes in `matmul_naive.axm`

The `@strategy` block exposes the following holes:

| Hole | Type | Range | Description |
|------|------|-------|-------------|
| `?tile_m` | u32 | [1, 512] | Tile size along the M dimension |
| `?tile_n` | u32 | [1, 512] | Tile size along the N dimension |
| `?tile_k` | u32 | [1, 512] | Tile size along the K dimension |
| `?loop_order` | array[ident] | -- | Permutation of loop indices |
| `?parallel_dims` | array[ident] | -- | Which dimensions to parallelise |
| `?unroll_factor` | u32 | [1, 32] | Inner-loop unroll factor |
| `?prefetch_distance` | u32 | [0, 16] | Cache prefetch distance |

An AI agent proposes values for every hole. The compiler validates the
proposal, compiles the specialised kernel, benchmarks it, and records
the result in the optimisation history. The agent inspects the history
to choose better values in the next iteration.
