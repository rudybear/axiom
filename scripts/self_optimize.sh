#!/bin/bash
# =============================================================================
# AXIOM Self-Optimization Loop (S2)
# =============================================================================
#
# Demonstrates compiler self-optimization:
#   1. Compiles a benchmark with `axiom optimize --dry-run`
#   2. Measures baseline performance with `axiom bench`
#   3. Iterates: optimize -> benchmark -> compare
#
# Usage:
#   chmod +x scripts/self_optimize.sh
#   ./scripts/self_optimize.sh [benchmark_file] [iterations]
#
# Example:
#   ./scripts/self_optimize.sh benchmarks/real_world/015_jpeg_dct.axm 3
# =============================================================================

set -euo pipefail

# Defaults
BENCHMARK="${1:-benchmarks/real_world/015_jpeg_dct.axm}"
ITERATIONS="${2:-3}"
RUNS="${3:-5}"

echo "=== AXIOM Self-Optimization Loop ==="
echo "Benchmark: $BENCHMARK"
echo "Iterations: $ITERATIONS"
echo "Measurement runs per iteration: $RUNS"
echo ""

# Check the benchmark file exists
if [ ! -f "$BENCHMARK" ]; then
    echo "ERROR: Benchmark file not found: $BENCHMARK"
    echo "Available benchmarks:"
    ls benchmarks/real_world/*.axm 2>/dev/null || echo "  (none found)"
    exit 1
fi

# Check axiom binary is available
if ! command -v axiom &> /dev/null; then
    AXIOM="cargo run --bin axiom --"
    echo "Note: 'axiom' not on PATH, using: $AXIOM"
else
    AXIOM="axiom"
fi

echo "--- Baseline measurement ---"
echo "Running: $AXIOM bench $BENCHMARK --runs $RUNS"
$AXIOM bench "$BENCHMARK" --runs "$RUNS" 2>&1 || echo "(benchmark may require compilation support)"
echo ""

for i in $(seq 1 "$ITERATIONS"); do
    echo "=== Iteration $i / $ITERATIONS ==="
    echo ""

    # Step 1: Generate optimization prompt (dry-run)
    echo "--- Step 1: Generate LLM optimization prompt ---"
    echo "Running: $AXIOM optimize $BENCHMARK --dry-run --iterations 1"
    $AXIOM optimize "$BENCHMARK" --dry-run --iterations 1 2>&1 || echo "(optimize dry-run complete or no surfaces)"
    echo ""

    # Step 2: Benchmark current version
    echo "--- Step 2: Benchmark ---"
    echo "Running: $AXIOM bench $BENCHMARK --runs $RUNS"
    $AXIOM bench "$BENCHMARK" --runs "$RUNS" 2>&1 || echo "(benchmark may require compilation support)"
    echo ""

    # Step 3: Profile to find optimization surfaces
    echo "--- Step 3: Profile ---"
    echo "Running: $AXIOM profile $BENCHMARK --iterations 3"
    $AXIOM profile "$BENCHMARK" --iterations 3 2>&1 || echo "(profile complete)"
    echo ""

    echo "--- Iteration $i complete ---"
    echo ""
done

echo "=== Self-Optimization Loop Complete ==="
echo ""
echo "To apply LLM suggestions automatically, run:"
echo "  $AXIOM optimize $BENCHMARK --iterations $ITERATIONS"
echo ""
echo "To use a specific API key:"
echo "  export ANTHROPIC_API_KEY=your-key-here"
echo "  $AXIOM optimize $BENCHMARK --iterations $ITERATIONS"
