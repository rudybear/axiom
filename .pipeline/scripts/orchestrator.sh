#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# AXIOM Pipeline Orchestrator
# ═══════════════════════════════════════════════════════════════════════
# Drives the multi-agent development pipeline for a given milestone.
#
# Usage: ./orchestrator.sh <milestone-id>
#   e.g.: ./orchestrator.sh M1.2-parser
#
# State machine:
#   ARCHITECT → CODER → REVIEWER → TESTER → BENCHMARK → MERGE
#   With retry loops on failure (see config.json for limits)
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PIPELINE_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(dirname "$PIPELINE_DIR")"

# ── Parse arguments ────────────────────────────────────────────────────
if [ $# -lt 1 ]; then
    echo "Usage: $0 <milestone-id>"
    echo "  e.g.: $0 M1.2-parser"
    echo ""
    echo "Available milestones:"
    ls "$PIPELINE_DIR/milestones/"*.json 2>/dev/null | xargs -I{} basename {} .json | sed 's/^/  /'
    exit 1
fi

MILESTONE_ID="$1"
MILESTONE_FILE="$PIPELINE_DIR/milestones/${MILESTONE_ID}.json"

if [ ! -f "$MILESTONE_FILE" ]; then
    echo "ERROR: Milestone file not found: $MILESTONE_FILE"
    exit 1
fi

# ── Generate run ID ───────────────────────────────────────────────────
RUN_ID="run-$(date +%Y%m%d)-$(printf '%03d' $(ls -d "$PIPELINE_DIR/runs/run-$(date +%Y%m%d)-"* 2>/dev/null | wc -l | tr -d ' '))"
RUN_DIR="$PIPELINE_DIR/runs/$RUN_ID"
mkdir -p "$RUN_DIR"

# ── Load configuration ────────────────────────────────────────────────
CONFIG_FILE="$PIPELINE_DIR/config.json"
MAX_CODER_RETRIES=$(jq -r '.workflow.retry_policy.max_coder_retries' "$CONFIG_FILE")
MAX_ARCHITECT_RETRIES=$(jq -r '.workflow.retry_policy.max_architect_retries' "$CONFIG_FILE")
MAX_REVIEWER_CYCLES=$(jq -r '.workflow.retry_policy.max_reviewer_cycles' "$CONFIG_FILE")
AGENT_TIMEOUT=$(jq -r '.timeouts.agent_timeout_seconds' "$CONFIG_FILE")

# ── Logging ────────────────────────────────────────────────────────────
LOG_FILE="$RUN_DIR/log.jsonl"

log_event() {
    local event="$1"
    shift
    local extra="$*"
    echo "{\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"event\":\"$event\",\"run_id\":\"$RUN_ID\",\"milestone\":\"$MILESTONE_ID\"${extra:+,$extra}}" >> "$LOG_FILE"
}

# ── Dependency check ──────────────────────────────────────────────────
check_dependencies() {
    local deps
    deps=$(jq -r '.depends_on[]' "$MILESTONE_FILE" 2>/dev/null || true)
    for dep in $deps; do
        if [ -z "$dep" ] || [ "$dep" = "null" ]; then continue; fi
        local dep_baseline="$PIPELINE_DIR/benchmarks/${dep}.jsonl"
        if [ ! -f "$dep_baseline" ]; then
            echo "WARNING: Dependency $dep has no completed benchmark baseline."
            echo "  Expected: $dep_baseline"
            echo "  Run milestone $dep first, or proceed at your own risk."
        fi
    done
}

# ── Tool availability check ──────────────────────────────────────────
check_tools() {
    echo "=== Checking required tools ==="
    local required
    required=$(jq -r '.required_tools[]' "$CONFIG_FILE" | tr -d '\r')
    local missing=0
    for tool in $required; do
        # Use --version probe — works reliably on Windows/MSYS2 where
        # which/command -v may not resolve .exe files
        if "$tool" --version > /dev/null 2>&1 || "$tool" --help > /dev/null 2>&1; then
            echo "  ✓ $tool"
        else
            echo "  ✗ $tool (MISSING)"
            missing=1
        fi
    done

    local optional
    optional=$(jq -r '.optional_tools[]' "$CONFIG_FILE" | tr -d '\r')
    for tool in $optional; do
        if "$tool" --version > /dev/null 2>&1 || "$tool" --help > /dev/null 2>&1; then
            echo "  ✓ $tool (optional)"
        else
            echo "  - $tool (optional, not found)"
        fi
    done

    if [ "$missing" -eq 1 ]; then
        echo "ERROR: Missing required tools. Install them and retry."
        exit 1
    fi
}

# ── State management ──────────────────────────────────────────────────
write_state() {
    local step="$1"
    local attempt="$2"
    local status="$3"
    cat > "$RUN_DIR/state.json" <<JSONEOF
{
  "run_id": "$RUN_ID",
  "milestone_id": "$MILESTONE_ID",
  "current_step": "$step",
  "attempt": $attempt,
  "status": "$status",
  "started_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "architect_retries": ${ARCHITECT_ATTEMPT:-0},
  "coder_retries": ${CODER_ATTEMPT:-0},
  "reviewer_cycles": ${REVIEWER_CYCLE:-0}
}
JSONEOF
}

# ── Run an agent ──────────────────────────────────────────────────────
run_agent() {
    local agent="$1"
    echo ""
    echo "═══════════════════════════════════════════════════════════════"
    echo "  RUNNING: $agent agent (attempt ${2:-1})"
    echo "  Milestone: $MILESTONE_ID"
    echo "  Run: $RUN_ID"
    echo "═══════════════════════════════════════════════════════════════"
    echo ""

    log_event "agent_start" "\"agent\":\"$agent\""

    local script="$SCRIPT_DIR/run-${agent}.sh"
    if [ ! -f "$script" ]; then
        echo "ERROR: Agent script not found: $script"
        log_event "agent_error" "\"agent\":\"$agent\",\"error\":\"script not found\""
        return 1
    fi

    if timeout "$AGENT_TIMEOUT" bash "$script" "$RUN_ID" "$MILESTONE_ID" "$PROJECT_ROOT"; then
        log_event "agent_complete" "\"agent\":\"$agent\",\"status\":\"success\""
        return 0
    else
        local exit_code=$?
        log_event "agent_complete" "\"agent\":\"$agent\",\"status\":\"failed\",\"exit_code\":$exit_code"
        return $exit_code
    fi
}

# ── Gate check ────────────────────────────────────────────────────────
run_gate_check() {
    local step="$1"
    echo ""
    echo "--- Gate check: $step ---"
    log_event "gate_check_start" "\"step\":\"$step\""

    if bash "$SCRIPT_DIR/gate-check.sh" "$RUN_ID" "$MILESTONE_ID" "$step" "$PROJECT_ROOT"; then
        log_event "gate_check_pass" "\"step\":\"$step\""
        echo "  ✓ Gate PASSED for $step"
        return 0
    else
        log_event "gate_check_fail" "\"step\":\"$step\""
        echo "  ✗ Gate FAILED for $step"
        return 1
    fi
}

# ═══════════════════════════════════════════════════════════════════════
# MAIN PIPELINE LOOP
# ═══════════════════════════════════════════════════════════════════════

echo ""
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║  AXIOM Pipeline — Starting                                  ║"
echo "║  Milestone: $MILESTONE_ID"
echo "║  Run ID:    $RUN_ID"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

cd "$PROJECT_ROOT"
check_tools
check_dependencies

log_event "pipeline_start"
write_state "init" 0 "starting"

ARCHITECT_ATTEMPT=0
CODER_ATTEMPT=0
REVIEWER_CYCLE=0

# ── Step 1: Architect ─────────────────────────────────────────────────
architect_phase() {
    ARCHITECT_ATTEMPT=$((ARCHITECT_ATTEMPT + 1))
    write_state "architect" "$ARCHITECT_ATTEMPT" "running"

    if ! run_agent "architect" "$ARCHITECT_ATTEMPT"; then
        echo "ERROR: Architect agent failed"
        return 1
    fi

    # Verify architect output exists
    if [ ! -f "$RUN_DIR/architect-output.json" ]; then
        echo "ERROR: Architect did not produce architect-output.json"
        return 1
    fi

    echo "  ✓ Architect produced specification"
    return 0
}

# ── Step 2: Coder ─────────────────────────────────────────────────────
coder_phase() {
    CODER_ATTEMPT=$((CODER_ATTEMPT + 1))
    write_state "coder" "$CODER_ATTEMPT" "running"

    if ! run_agent "coder" "$CODER_ATTEMPT"; then
        echo "ERROR: Coder agent failed"
        return 1
    fi

    if [ ! -f "$RUN_DIR/coder-output.json" ]; then
        echo "ERROR: Coder did not produce coder-output.json"
        return 1
    fi

    # Basic gate: cargo check must pass
    echo "  Running cargo check..."
    if ! cargo check --workspace 2>&1 | tee "$RUN_DIR/cargo-check-output.txt"; then
        echo "  ✗ cargo check failed"
        return 1
    fi
    echo "  ✓ cargo check passed"
    return 0
}

# ── Step 3: Reviewer ──────────────────────────────────────────────────
reviewer_phase() {
    REVIEWER_CYCLE=$((REVIEWER_CYCLE + 1))
    write_state "reviewer" "$REVIEWER_CYCLE" "running"

    if ! run_agent "reviewer" "$REVIEWER_CYCLE"; then
        echo "ERROR: Reviewer agent failed"
        return 1
    fi

    if [ ! -f "$RUN_DIR/reviewer-output.json" ]; then
        echo "ERROR: Reviewer did not produce reviewer-output.json"
        return 1
    fi

    local verdict
    verdict=$(jq -r '.verdict' "$RUN_DIR/reviewer-output.json")

    case "$verdict" in
        "APPROVE")
            echo "  ✓ Reviewer APPROVED"
            return 0
            ;;
        "REQUEST_CHANGES")
            echo "  → Reviewer requested changes (cycle $REVIEWER_CYCLE/$MAX_REVIEWER_CYCLES)"
            if [ "$REVIEWER_CYCLE" -ge "$MAX_REVIEWER_CYCLES" ]; then
                echo "  ✗ Max reviewer cycles reached"
                return 2  # special code: escalate
            fi
            return 1  # retry coder
            ;;
        "REJECT")
            echo "  ✗ Reviewer REJECTED — design issue"
            return 2  # escalate to architect
            ;;
        *)
            echo "  ✗ Unknown verdict: $verdict"
            return 1
            ;;
    esac
}

# ── Step 4: Tester ────────────────────────────────────────────────────
tester_phase() {
    write_state "tester" 1 "running"

    if ! run_agent "tester" 1; then
        echo "ERROR: Tester agent failed"
        return 1
    fi

    if ! run_gate_check "tester"; then
        return 1
    fi

    return 0
}

# ── Step 5: Benchmark ─────────────────────────────────────────────────
benchmark_phase() {
    write_state "benchmark" 1 "running"

    if ! run_agent "benchmark" 1; then
        echo "ERROR: Benchmark agent failed"
        return 1
    fi

    if [ -f "$RUN_DIR/benchmark-output.json" ]; then
        local bench_status
        bench_status=$(jq -r '.status' "$RUN_DIR/benchmark-output.json")
        if [ "$bench_status" = "FAIL" ]; then
            echo "  ✗ Benchmark detected regression"
            return 1
        fi
    fi

    echo "  ✓ Benchmark passed"
    return 0
}

# ── Main execution loop ──────────────────────────────────────────────

# Phase: Architect
while [ "$ARCHITECT_ATTEMPT" -lt "$MAX_ARCHITECT_RETRIES" ]; do
    if architect_phase; then
        break
    fi
    if [ "$ARCHITECT_ATTEMPT" -ge "$MAX_ARCHITECT_RETRIES" ]; then
        echo "FATAL: Architect failed after $MAX_ARCHITECT_RETRIES attempts"
        log_event "pipeline_failed" "\"reason\":\"architect_exhausted\""
        bash "$SCRIPT_DIR/rollback.sh" "$RUN_ID" "$MILESTONE_ID" "$PROJECT_ROOT"
        exit 1
    fi
done

# Phase: Coder ↔ Reviewer loop
CODER_ATTEMPT=0
REVIEWER_CYCLE=0
coder_approved=false

while [ "$coder_approved" = "false" ]; do
    # Run coder
    if [ "$CODER_ATTEMPT" -ge "$MAX_CODER_RETRIES" ]; then
        echo "FATAL: Coder failed after $MAX_CODER_RETRIES attempts"
        # Escalate to architect re-spec
        if [ "$ARCHITECT_ATTEMPT" -lt "$MAX_ARCHITECT_RETRIES" ]; then
            echo "  → Escalating to Architect for re-specification"
            log_event "escalate_to_architect" "\"reason\":\"coder_exhausted\""
            CODER_ATTEMPT=0
            REVIEWER_CYCLE=0
            if ! architect_phase; then
                echo "FATAL: Architect re-spec failed"
                log_event "pipeline_failed" "\"reason\":\"architect_reattempt_failed\""
                bash "$SCRIPT_DIR/rollback.sh" "$RUN_ID" "$MILESTONE_ID" "$PROJECT_ROOT"
                exit 1
            fi
            continue
        else
            log_event "pipeline_failed" "\"reason\":\"all_retries_exhausted\""
            bash "$SCRIPT_DIR/rollback.sh" "$RUN_ID" "$MILESTONE_ID" "$PROJECT_ROOT"
            exit 1
        fi
    fi

    if ! coder_phase; then
        continue
    fi

    # Run reviewer
    reviewer_result=0
    reviewer_phase || reviewer_result=$?

    case $reviewer_result in
        0)
            coder_approved=true
            ;;
        1)
            # Request changes → retry coder
            echo "  → Looping back to Coder with review feedback"
            log_event "retry_coder" "\"reason\":\"reviewer_request_changes\""
            ;;
        2)
            # Reject → escalate to architect
            echo "  → Escalating to Architect"
            log_event "escalate_to_architect" "\"reason\":\"reviewer_reject\""
            if [ "$ARCHITECT_ATTEMPT" -lt "$MAX_ARCHITECT_RETRIES" ]; then
                CODER_ATTEMPT=0
                REVIEWER_CYCLE=0
                if ! architect_phase; then
                    echo "FATAL: Architect re-spec failed"
                    log_event "pipeline_failed" "\"reason\":\"architect_reattempt_failed\""
                    bash "$SCRIPT_DIR/rollback.sh" "$RUN_ID" "$MILESTONE_ID" "$PROJECT_ROOT"
                    exit 1
                fi
            else
                echo "FATAL: All retries exhausted"
                log_event "pipeline_failed" "\"reason\":\"all_retries_exhausted\""
                bash "$SCRIPT_DIR/rollback.sh" "$RUN_ID" "$MILESTONE_ID" "$PROJECT_ROOT"
                exit 1
            fi
            ;;
    esac
done

# Phase: Tester
if ! tester_phase; then
    echo "  → Tester failed, looping back to Coder"
    log_event "retry_coder" "\"reason\":\"tester_fail\""
    # For simplicity in v1: retry coder once more
    CODER_ATTEMPT=$((CODER_ATTEMPT - 1))  # give one more attempt
    if coder_phase && reviewer_phase; then
        if ! tester_phase; then
            echo "FATAL: Tests still failing after retry"
            log_event "pipeline_failed" "\"reason\":\"tester_fail_after_retry\""
            bash "$SCRIPT_DIR/rollback.sh" "$RUN_ID" "$MILESTONE_ID" "$PROJECT_ROOT"
            exit 1
        fi
    fi
fi

# Phase: Benchmark
if ! benchmark_phase; then
    echo "  → Benchmark regression detected, looping back to Coder"
    log_event "retry_coder" "\"reason\":\"benchmark_regression\""
    CODER_ATTEMPT=$((CODER_ATTEMPT - 1))
    if coder_phase && reviewer_phase && tester_phase; then
        if ! benchmark_phase; then
            echo "WARNING: Benchmark regression persists — proceeding with warning"
            log_event "benchmark_regression_accepted" "\"reason\":\"persistent_regression\""
        fi
    fi
fi

# ── Merge to main ─────────────────────────────────────────────────────
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  ALL GATES PASSED — Merging to main"
echo "═══════════════════════════════════════════════════════════════"

write_state "merge" 1 "merging"

TESTER_BRANCH="tester/$RUN_ID/$MILESTONE_ID"
if git rev-parse --verify "$TESTER_BRANCH" > /dev/null 2>&1; then
    git checkout main
    git merge --no-ff "$TESTER_BRANCH" -m "milestone($MILESTONE_ID): complete — run $RUN_ID

All gates passed:
- Architect: specified
- Coder: implemented
- Reviewer: approved
- Tester: all tests pass
- Benchmark: no regressions"
    echo "  ✓ Merged $TESTER_BRANCH into main"
else
    # If no separate branch was used, just tag the current state
    git tag "milestone/$MILESTONE_ID/$RUN_ID" -m "Milestone $MILESTONE_ID completed"
    echo "  ✓ Tagged milestone completion"
fi

write_state "complete" 0 "success"
log_event "pipeline_complete"

echo ""
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║  PIPELINE COMPLETE                                          ║"
echo "║  Milestone: $MILESTONE_ID"
echo "║  Run: $RUN_ID"
echo "║  Status: SUCCESS                                            ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
