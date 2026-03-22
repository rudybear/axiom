#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# Run Tester Agent
# ═══════════════════════════════════════════════════════════════════════
# Runs tests and verifies acceptance criteria. Optionally launches
# Claude Code to write additional tests.
#
# Usage: ./run-tester.sh <run-id> <milestone-id> <project-root>
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

RUN_ID="$1"
MILESTONE_ID="$2"
PROJECT_ROOT="${3:-.}"

PIPELINE_DIR="$PROJECT_ROOT/.pipeline"
RUN_DIR="$PIPELINE_DIR/runs/$RUN_ID"
MILESTONE_FILE="$PIPELINE_DIR/milestones/${MILESTONE_ID}.json"
TEMPLATE_FILE="$PIPELINE_DIR/templates/tester.md"
ARCHITECT_OUTPUT="$RUN_DIR/architect-output.json"
OUTPUT_FILE="$RUN_DIR/tester-output.json"

cd "$PROJECT_ROOT"

# ── Create/switch branch ──────────────────────────────────────────────
BRANCH="tester/$RUN_ID/$MILESTONE_ID"
CODER_BRANCH="coder/$RUN_ID/$MILESTONE_ID"

if git rev-parse --verify "$BRANCH" > /dev/null 2>&1; then
    git checkout "$BRANCH"
else
    if git rev-parse --verify "$CODER_BRANCH" > /dev/null 2>&1; then
        git checkout -b "$BRANCH" "$CODER_BRANCH"
    else
        git checkout -b "$BRANCH"
    fi
fi

# ── Run cargo test ────────────────────────────────────────────────────
echo "Running workspace tests..."
TEST_OUTPUT_FILE="$RUN_DIR/tester-cargo-test.txt"

TOTAL=0
PASSED_TESTS=0
FAILED_TESTS=0
TEST_STATUS="PASS"

# Run tests for each affected crate
AFFECTED_CRATES=$(jq -r '.crates_affected[]' "$MILESTONE_FILE" 2>/dev/null || echo "")

declare -A CRATE_RESULTS 2>/dev/null || true  # bash 4+ associative arrays

for crate in $AFFECTED_CRATES; do
    echo ""
    echo "=== Testing $crate ==="
    CRATE_OUTPUT="$RUN_DIR/tester-${crate}.txt"

    if cargo test -p "$crate" 2>&1 | tee "$CRATE_OUTPUT"; then
        echo "  ✓ $crate tests passed"
        # Parse test count from output
        CRATE_TOTAL=$(grep -oP 'running \K\d+' "$CRATE_OUTPUT" | tail -1 || echo "0")
        CRATE_PASSED=$CRATE_TOTAL  # if exit 0, all passed
        TOTAL=$((TOTAL + CRATE_TOTAL))
        PASSED_TESTS=$((PASSED_TESTS + CRATE_PASSED))
    else
        echo "  ✗ $crate tests FAILED"
        TEST_STATUS="FAIL"
        CRATE_TOTAL=$(grep -oP 'running \K\d+' "$CRATE_OUTPUT" | tail -1 || echo "0")
        CRATE_FAILED=$(grep -oP '\d+ failed' "$CRATE_OUTPUT" | grep -oP '\d+' || echo "0")
        CRATE_PASSED=$((CRATE_TOTAL - CRATE_FAILED))
        TOTAL=$((TOTAL + CRATE_TOTAL))
        PASSED_TESTS=$((PASSED_TESTS + CRATE_PASSED))
        FAILED_TESTS=$((FAILED_TESTS + CRATE_FAILED))
    fi
done

# ── Run full workspace tests ──────────────────────────────────────────
echo ""
echo "=== Running full workspace tests ==="
if cargo test --workspace 2>&1 | tee "$TEST_OUTPUT_FILE"; then
    echo "  ✓ Full workspace tests passed"
else
    echo "  ✗ Full workspace tests FAILED"
    TEST_STATUS="FAIL"
fi

# ── Run acceptance criteria checks ────────────────────────────────────
echo ""
echo "=== Checking acceptance criteria ==="

AC_RESULTS="[]"
CRITERIA_COUNT=$(jq '.acceptance_criteria | length' "$MILESTONE_FILE")

for i in $(seq 0 $((CRITERIA_COUNT - 1))); do
    AC_ID=$(jq -r ".acceptance_criteria[$i].id" "$MILESTONE_FILE")
    AC_DESC=$(jq -r ".acceptance_criteria[$i].description" "$MILESTONE_FILE")
    AC_CMD=$(jq -r ".acceptance_criteria[$i].check_command" "$MILESTONE_FILE")
    AC_PATTERN=$(jq -r ".acceptance_criteria[$i].check_pattern" "$MILESTONE_FILE")
    AC_TYPE=$(jq -r ".acceptance_criteria[$i].check_type" "$MILESTONE_FILE")

    CMD_OUTPUT=$(eval "$AC_CMD" 2>&1) || true
    AC_STATUS="PASS"

    case "$AC_TYPE" in
        "command_output_contains")
            if ! echo "$CMD_OUTPUT" | grep -q "$AC_PATTERN"; then
                AC_STATUS="FAIL"
                TEST_STATUS="FAIL"
            fi
            ;;
        "command_output_not_contains")
            if echo "$CMD_OUTPUT" | grep -q "$AC_PATTERN"; then
                AC_STATUS="FAIL"
                TEST_STATUS="FAIL"
            fi
            ;;
        *)
            AC_STATUS="SKIP"
            ;;
    esac

    echo "  [$AC_ID] $AC_DESC ... $AC_STATUS"

    AC_RESULTS=$(echo "$AC_RESULTS" | jq --arg id "$AC_ID" --arg desc "$AC_DESC" --arg status "$AC_STATUS" \
        '. + [{"id": $id, "description": $desc, "status": $status}]')
done

# ── Write output ──────────────────────────────────────────────────────
cat > "$OUTPUT_FILE" <<JSONEOF
{
  "pipeline_version": "1.0",
  "run_id": "$RUN_ID",
  "milestone_id": "$MILESTONE_ID",
  "agent": "tester",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "status": "$TEST_STATUS",
  "test_results": {
    "total": $TOTAL,
    "passed": $PASSED_TESTS,
    "failed": $FAILED_TESTS,
    "skipped": 0
  },
  "acceptance_criteria_results": $(echo "$AC_RESULTS"),
  "tests_added": [],
  "coverage_notes": "Automated test run — see individual crate outputs for details"
}
JSONEOF

# ── Commit test additions if any ──────────────────────────────────────
if ! git diff --quiet 2>/dev/null; then
    git add -A
    git commit -m "pipeline(tester): test verification for $MILESTONE_ID — run $RUN_ID" || true
fi

echo ""
echo "Test results: $PASSED_TESTS passed, $FAILED_TESTS failed (of $TOTAL)"
echo "Overall status: $TEST_STATUS"

if [ "$TEST_STATUS" = "PASS" ]; then
    exit 0
else
    exit 1
fi
