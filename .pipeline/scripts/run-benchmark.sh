#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# Run Benchmark Agent
# ═══════════════════════════════════════════════════════════════════════
# Measures performance metrics and detects regressions against baselines.
#
# Usage: ./run-benchmark.sh <run-id> <milestone-id> <project-root>
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

RUN_ID="$1"
MILESTONE_ID="$2"
PROJECT_ROOT="${3:-.}"

PIPELINE_DIR="$PROJECT_ROOT/.pipeline"
RUN_DIR="$PIPELINE_DIR/runs/$RUN_ID"
MILESTONE_FILE="$PIPELINE_DIR/milestones/${MILESTONE_ID}.json"
BENCHMARKS_DIR="$PIPELINE_DIR/benchmarks"
BASELINE_FILE="$BENCHMARKS_DIR/baselines.json"
HISTORY_FILE="$BENCHMARKS_DIR/${MILESTONE_ID}.jsonl"
REGRESSION_CONFIG="$BENCHMARKS_DIR/regression-config.json"
OUTPUT_FILE="$RUN_DIR/benchmark-output.json"

cd "$PROJECT_ROOT"

# ── Gather environment info ───────────────────────────────────────────
RUST_VERSION=$(rustc --version 2>/dev/null | head -1 || echo "unknown")
OS_INFO=$(uname -s 2>/dev/null || echo "Windows")
ARCH_INFO=$(uname -m 2>/dev/null || echo "x86_64")

# ── Measure compilation time ──────────────────────────────────────────
echo "=== Benchmark: Compilation Time ==="

# Clean build to get accurate timing
cargo clean 2>/dev/null || true

BUILD_START=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
cargo build --workspace 2>&1 | tail -3
BUILD_END=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
BUILD_TIME_MS=$(( (BUILD_END - BUILD_START) / 1000000 ))
echo "  Build time: ${BUILD_TIME_MS}ms"

# ── Measure test execution time ───────────────────────────────────────
echo ""
echo "=== Benchmark: Test Execution Time ==="

TEST_START=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
TEST_OUTPUT=$(cargo test --workspace 2>&1) || true
TEST_END=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
TEST_TIME_MS=$(( (TEST_END - TEST_START) / 1000000 ))
echo "  Test time: ${TEST_TIME_MS}ms"

# Count tests
TEST_COUNT=$(echo "$TEST_OUTPUT" | grep -oP '(\d+) passed' | grep -oP '\d+' | head -1 || echo "0")
echo "  Test count: $TEST_COUNT"

# ── Measure code metrics ──────────────────────────────────────────────
echo ""
echo "=== Benchmark: Code Metrics ==="

TOTAL_LOC=$(find crates -name "*.rs" -exec cat {} + 2>/dev/null | wc -l | tr -d ' ')
echo "  Total LOC: $TOTAL_LOC"

# ── Measure per-sample-file metrics ───────────────────────────────────
echo ""
echo "=== Benchmark: Sample File Processing ==="

# Lexer timing (if driver exists)
LEX_FIBONACCI_MS="null"
LEX_MATMUL_MS="null"

if cargo build -p axiom-driver 2>/dev/null; then
    for sample in fibonacci hello matmul_naive; do
        SAMPLE_FILE="tests/samples/${sample}.axm"
        if [ -f "$SAMPLE_FILE" ]; then
            SAMPLE_START=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
            cargo run -p axiom-driver -- lex "$SAMPLE_FILE" > /dev/null 2>&1 || true
            SAMPLE_END=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
            SAMPLE_MS=$(( (SAMPLE_END - SAMPLE_START) / 1000000 ))
            echo "  Lex $sample: ${SAMPLE_MS}ms"

            case "$sample" in
                "fibonacci") LEX_FIBONACCI_MS=$SAMPLE_MS ;;
                "matmul_naive") LEX_MATMUL_MS=$SAMPLE_MS ;;
            esac
        fi
    done
fi

# ── Compare against baselines ─────────────────────────────────────────
echo ""
echo "=== Regression Check ==="

STATUS="PASS"
REGRESSIONS="[]"
IMPROVEMENTS="[]"
BASELINE_ACTION="unchanged"

if [ -f "$BASELINE_FILE" ]; then
    # Load baseline for this milestone (or nearest prior milestone)
    BASELINE_BUILD=$(jq -r ".\"$MILESTONE_ID\".cargo_build_time_ms // empty" "$BASELINE_FILE" 2>/dev/null || echo "")
    BASELINE_TEST=$(jq -r ".\"$MILESTONE_ID\".cargo_test_time_ms // empty" "$BASELINE_FILE" 2>/dev/null || echo "")

    COMPILE_THRESHOLD=$(jq -r '.thresholds.compile_time_pct' "$REGRESSION_CONFIG")

    if [ -n "$BASELINE_BUILD" ] && [ "$BASELINE_BUILD" != "null" ]; then
        CHANGE_PCT=$(python3 -c "print(round(($BUILD_TIME_MS - $BASELINE_BUILD) / $BASELINE_BUILD * 100, 1))" 2>/dev/null || echo "0")
        echo "  Build time: ${BASELINE_BUILD}ms → ${BUILD_TIME_MS}ms (${CHANGE_PCT}%)"

        IS_REGRESSION=$(python3 -c "print('yes' if $CHANGE_PCT > $COMPILE_THRESHOLD else 'no')" 2>/dev/null || echo "no")
        if [ "$IS_REGRESSION" = "yes" ]; then
            STATUS="FAIL"
            REGRESSIONS=$(echo "$REGRESSIONS" | jq --arg metric "cargo_build_time_ms" \
                --argjson baseline "$BASELINE_BUILD" --argjson current "$BUILD_TIME_MS" \
                --argjson change "$CHANGE_PCT" --argjson threshold "$COMPILE_THRESHOLD" \
                '. + [{"metric": $metric, "baseline": $baseline, "current": $current, "change_pct": $change, "threshold_pct": $threshold, "verdict": "FAIL"}]')
        fi
    else
        echo "  No baseline for build time — establishing"
        BASELINE_ACTION="established"
    fi
else
    echo "  No baselines file — establishing initial baselines"
    BASELINE_ACTION="established"
fi

# ── Update baselines ──────────────────────────────────────────────────
if [ "$STATUS" = "PASS" ]; then
    if [ ! -f "$BASELINE_FILE" ]; then
        echo '{}' > "$BASELINE_FILE"
    fi

    TEMP_FILE=$(mktemp)
    jq --arg ms "$MILESTONE_ID" --argjson build "$BUILD_TIME_MS" --argjson test "$TEST_TIME_MS" \
       --argjson loc "$TOTAL_LOC" --argjson tests "$TEST_COUNT" \
        '.[$ms] = {
            "cargo_build_time_ms": $build,
            "cargo_test_time_ms": $test,
            "total_loc": $loc,
            "test_count": $tests
        }' "$BASELINE_FILE" > "$TEMP_FILE"
    mv "$TEMP_FILE" "$BASELINE_FILE"

    if [ "$BASELINE_ACTION" = "unchanged" ]; then
        BASELINE_ACTION="updated"
    fi
fi

# ── Append to history ─────────────────────────────────────────────────
cat >> "$HISTORY_FILE" <<HISTEOF
{"run_id":"$RUN_ID","milestone":"$MILESTONE_ID","timestamp":"$(date -u +%Y-%m-%dT%H:%M:%SZ)","git_sha":"$(git rev-parse HEAD 2>/dev/null || echo 'unknown')","metrics":{"cargo_build_time_ms":$BUILD_TIME_MS,"cargo_test_time_ms":$TEST_TIME_MS,"total_loc":$TOTAL_LOC,"test_count":$TEST_COUNT,"lex_fibonacci_ms":$LEX_FIBONACCI_MS,"lex_matmul_ms":$LEX_MATMUL_MS}}
HISTEOF

# ── Write output ──────────────────────────────────────────────────────
cat > "$OUTPUT_FILE" <<JSONEOF
{
  "pipeline_version": "1.0",
  "run_id": "$RUN_ID",
  "milestone_id": "$MILESTONE_ID",
  "agent": "benchmark",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "status": "$STATUS",
  "environment": {
    "os": "$OS_INFO",
    "arch": "$ARCH_INFO",
    "rust_version": "$RUST_VERSION"
  },
  "metrics": {
    "cargo_build_time_ms": $BUILD_TIME_MS,
    "cargo_test_time_ms": $TEST_TIME_MS,
    "total_loc": $TOTAL_LOC,
    "test_count": $TEST_COUNT,
    "lex_fibonacci_ms": $LEX_FIBONACCI_MS,
    "lex_matmul_ms": $LEX_MATMUL_MS
  },
  "regressions": $REGRESSIONS,
  "improvements": $IMPROVEMENTS,
  "baseline_action": "$BASELINE_ACTION"
}
JSONEOF

echo ""
echo "Benchmark status: $STATUS"
echo "Baseline action: $BASELINE_ACTION"

if [ "$STATUS" = "PASS" ]; then
    exit 0
else
    exit 1
fi
