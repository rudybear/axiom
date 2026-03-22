#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# Gate Check — Verify acceptance criteria for a pipeline step
# ═══════════════════════════════════════════════════════════════════════
# Usage: ./gate-check.sh <run-id> <milestone-id> <step> <project-root>
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

RUN_ID="$1"
MILESTONE_ID="$2"
STEP="$3"
PROJECT_ROOT="${4:-.}"

PIPELINE_DIR="$PROJECT_ROOT/.pipeline"
RUN_DIR="$PIPELINE_DIR/runs/$RUN_ID"
MILESTONE_FILE="$PIPELINE_DIR/milestones/${MILESTONE_ID}.json"
RESULTS_FILE="$RUN_DIR/gate-${STEP}-results.json"

cd "$PROJECT_ROOT"

echo "Gate check for step: $STEP"
echo "Milestone: $MILESTONE_ID"
echo "Run: $RUN_ID"
echo ""

ALL_PASSED=true
RESULTS="[]"
CHECKED=0
PASSED=0
FAILED=0
SKIPPED=0

# ── Step-specific pre-checks ──────────────────────────────────────────

case "$STEP" in
    "architect")
        # Verify architect output has required fields
        if [ ! -f "$RUN_DIR/architect-output.json" ]; then
            echo "  FAIL: architect-output.json not found"
            ALL_PASSED=false
        else
            for field in files_to_create technical_spec acceptance_tests; do
                if ! jq -e ".$field" "$RUN_DIR/architect-output.json" > /dev/null 2>&1; then
                    echo "  FAIL: architect-output.json missing field: $field"
                    ALL_PASSED=false
                else
                    echo "  PASS: architect-output.json has $field"
                fi
            done
        fi
        ;;

    "coder")
        # Verify code compiles
        echo "  Running cargo check..."
        if cargo check --workspace 2>&1; then
            echo "  PASS: cargo check"
            PASSED=$((PASSED + 1))
        else
            echo "  FAIL: cargo check"
            ALL_PASSED=false
            FAILED=$((FAILED + 1))
        fi
        CHECKED=$((CHECKED + 1))
        ;;

    "reviewer")
        # Verify reviewer verdict is APPROVE
        if [ -f "$RUN_DIR/reviewer-output.json" ]; then
            local_verdict=$(jq -r '.verdict' "$RUN_DIR/reviewer-output.json")
            if [ "$local_verdict" = "APPROVE" ]; then
                echo "  PASS: Reviewer approved"
                PASSED=$((PASSED + 1))
            else
                echo "  FAIL: Reviewer verdict is $local_verdict"
                ALL_PASSED=false
                FAILED=$((FAILED + 1))
            fi
        else
            echo "  FAIL: reviewer-output.json not found"
            ALL_PASSED=false
            FAILED=$((FAILED + 1))
        fi
        CHECKED=$((CHECKED + 1))
        ;;

    "tester")
        # Run all acceptance criteria from milestone definition
        echo "  Running acceptance criteria..."
        echo ""

        CRITERIA_COUNT=$(jq '.acceptance_criteria | length' "$MILESTONE_FILE")

        for i in $(seq 0 $((CRITERIA_COUNT - 1))); do
            AC_ID=$(jq -r ".acceptance_criteria[$i].id" "$MILESTONE_FILE")
            AC_DESC=$(jq -r ".acceptance_criteria[$i].description" "$MILESTONE_FILE")
            AC_CMD=$(jq -r ".acceptance_criteria[$i].check_command" "$MILESTONE_FILE")
            AC_PATTERN=$(jq -r ".acceptance_criteria[$i].check_pattern" "$MILESTONE_FILE")
            AC_TYPE=$(jq -r ".acceptance_criteria[$i].check_type" "$MILESTONE_FILE")
            AC_OPTIONAL=$(jq -r ".acceptance_criteria[$i].optional // false" "$MILESTONE_FILE")

            CHECKED=$((CHECKED + 1))
            echo -n "  [$AC_ID] $AC_DESC ... "

            # Execute the check command
            CMD_OUTPUT=$(eval "$AC_CMD" 2>&1) || true

            case "$AC_TYPE" in
                "command_output_contains")
                    if echo "$CMD_OUTPUT" | grep -q "$AC_PATTERN"; then
                        echo "PASS"
                        PASSED=$((PASSED + 1))
                    else
                        if [ "$AC_OPTIONAL" = "true" ]; then
                            echo "SKIP (optional)"
                            SKIPPED=$((SKIPPED + 1))
                        else
                            echo "FAIL"
                            echo "    Expected output to contain: $AC_PATTERN"
                            echo "    Actual output (last 5 lines):"
                            echo "$CMD_OUTPUT" | tail -5 | sed 's/^/      /'
                            ALL_PASSED=false
                            FAILED=$((FAILED + 1))
                        fi
                    fi
                    ;;

                "command_output_not_contains")
                    if echo "$CMD_OUTPUT" | grep -q "$AC_PATTERN"; then
                        if [ "$AC_OPTIONAL" = "true" ]; then
                            echo "SKIP (optional)"
                            SKIPPED=$((SKIPPED + 1))
                        else
                            echo "FAIL"
                            echo "    Expected output NOT to contain: $AC_PATTERN"
                            ALL_PASSED=false
                            FAILED=$((FAILED + 1))
                        fi
                    else
                        echo "PASS"
                        PASSED=$((PASSED + 1))
                    fi
                    ;;

                "command_exit_zero")
                    if eval "$AC_CMD" > /dev/null 2>&1; then
                        echo "PASS"
                        PASSED=$((PASSED + 1))
                    else
                        if [ "$AC_OPTIONAL" = "true" ]; then
                            echo "SKIP (optional)"
                            SKIPPED=$((SKIPPED + 1))
                        else
                            echo "FAIL"
                            ALL_PASSED=false
                            FAILED=$((FAILED + 1))
                        fi
                    fi
                    ;;

                "command_output_matches_regex")
                    if echo "$CMD_OUTPUT" | grep -qE "$AC_PATTERN"; then
                        echo "PASS"
                        PASSED=$((PASSED + 1))
                    else
                        if [ "$AC_OPTIONAL" = "true" ]; then
                            echo "SKIP (optional)"
                            SKIPPED=$((SKIPPED + 1))
                        else
                            echo "FAIL"
                            ALL_PASSED=false
                            FAILED=$((FAILED + 1))
                        fi
                    fi
                    ;;

                *)
                    echo "SKIP (unknown check type: $AC_TYPE)"
                    SKIPPED=$((SKIPPED + 1))
                    ;;
            esac
        done
        ;;

    "benchmark")
        # Check benchmark output for regressions
        if [ -f "$RUN_DIR/benchmark-output.json" ]; then
            BENCH_STATUS=$(jq -r '.status' "$RUN_DIR/benchmark-output.json")
            if [ "$BENCH_STATUS" = "PASS" ]; then
                echo "  PASS: Benchmark — no regressions"
                PASSED=$((PASSED + 1))
            else
                REGRESSION_COUNT=$(jq '.regressions | length' "$RUN_DIR/benchmark-output.json")
                echo "  FAIL: Benchmark — $REGRESSION_COUNT regressions detected"
                jq -r '.regressions[] | "    - \(.metric): \(.baseline) → \(.current) (\(.change_pct)%)"' "$RUN_DIR/benchmark-output.json"
                ALL_PASSED=false
                FAILED=$((FAILED + 1))
            fi
        else
            echo "  SKIP: No benchmark output (first run)"
            SKIPPED=$((SKIPPED + 1))
        fi
        CHECKED=$((CHECKED + 1))
        ;;
esac

# ── Write results ─────────────────────────────────────────────────────
cat > "$RESULTS_FILE" <<JSONEOF
{
  "run_id": "$RUN_ID",
  "milestone_id": "$MILESTONE_ID",
  "step": "$STEP",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "overall": $([ "$ALL_PASSED" = true ] && echo '"PASS"' || echo '"FAIL"'),
  "checked": $CHECKED,
  "passed": $PASSED,
  "failed": $FAILED,
  "skipped": $SKIPPED
}
JSONEOF

echo ""
echo "Gate results: $PASSED passed, $FAILED failed, $SKIPPED skipped (of $CHECKED checks)"

if [ "$ALL_PASSED" = true ]; then
    exit 0
else
    exit 1
fi
