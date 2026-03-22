#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# Rollback — Handle pipeline failure gracefully
# ═══════════════════════════════════════════════════════════════════════
# Preserves all artifacts for post-mortem analysis, returns to main.
# Does NOT delete branches — they remain for human inspection.
#
# Usage: ./rollback.sh <run-id> <milestone-id> <project-root>
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

RUN_ID="$1"
MILESTONE_ID="$2"
PROJECT_ROOT="${3:-.}"

PIPELINE_DIR="$PROJECT_ROOT/.pipeline"
RUN_DIR="$PIPELINE_DIR/runs/$RUN_ID"

cd "$PROJECT_ROOT"

echo ""
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║  PIPELINE ROLLBACK                                          ║"
echo "║  Run: $RUN_ID"
echo "║  Milestone: $MILESTONE_ID"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

# ── Generate failure report ───────────────────────────────────────────
FAILURE_REPORT="$RUN_DIR/failure-report.json"

# Collect all output files
ARCHITECT_STATUS="none"
CODER_STATUS="none"
REVIEWER_STATUS="none"
TESTER_STATUS="none"
BENCHMARK_STATUS="none"

[ -f "$RUN_DIR/architect-output.json" ] && ARCHITECT_STATUS=$(jq -r '.status // "unknown"' "$RUN_DIR/architect-output.json" 2>/dev/null || echo "error")
[ -f "$RUN_DIR/coder-output.json" ] && CODER_STATUS=$(jq -r '.status // "unknown"' "$RUN_DIR/coder-output.json" 2>/dev/null || echo "error")
[ -f "$RUN_DIR/reviewer-output.json" ] && REVIEWER_STATUS=$(jq -r '.verdict // "unknown"' "$RUN_DIR/reviewer-output.json" 2>/dev/null || echo "error")
[ -f "$RUN_DIR/tester-output.json" ] && TESTER_STATUS=$(jq -r '.status // "unknown"' "$RUN_DIR/tester-output.json" 2>/dev/null || echo "error")
[ -f "$RUN_DIR/benchmark-output.json" ] && BENCHMARK_STATUS=$(jq -r '.status // "unknown"' "$RUN_DIR/benchmark-output.json" 2>/dev/null || echo "error")

# Pipeline state
FINAL_STEP="unknown"
FINAL_ATTEMPT=0
if [ -f "$RUN_DIR/state.json" ]; then
    FINAL_STEP=$(jq -r '.current_step' "$RUN_DIR/state.json" 2>/dev/null || echo "unknown")
    FINAL_ATTEMPT=$(jq -r '.attempt' "$RUN_DIR/state.json" 2>/dev/null || echo "0")
fi

cat > "$FAILURE_REPORT" <<JSONEOF
{
  "run_id": "$RUN_ID",
  "milestone_id": "$MILESTONE_ID",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "failed_at_step": "$FINAL_STEP",
  "failed_at_attempt": $FINAL_ATTEMPT,
  "agent_statuses": {
    "architect": "$ARCHITECT_STATUS",
    "coder": "$CODER_STATUS",
    "reviewer": "$REVIEWER_STATUS",
    "tester": "$TESTER_STATUS",
    "benchmark": "$BENCHMARK_STATUS"
  },
  "branches_preserved": [
    "architect/$RUN_ID/$MILESTONE_ID",
    "coder/$RUN_ID/$MILESTONE_ID",
    "tester/$RUN_ID/$MILESTONE_ID"
  ],
  "artifacts_dir": "$RUN_DIR",
  "action_required": "Review failure-report.json and agent outputs in $RUN_DIR. Branches are preserved for inspection."
}
JSONEOF

# ── List preserved branches ───────────────────────────────────────────
echo "Preserved branches:"
git branch --list "*/$RUN_ID/*" 2>/dev/null | sed 's/^/  /' || echo "  (none)"

echo ""
echo "Preserved artifacts:"
ls -la "$RUN_DIR/" 2>/dev/null | sed 's/^/  /'

# ── Return to main ───────────────────────────────────────────────────
echo ""
echo "Returning to main branch..."
git checkout main 2>/dev/null || git checkout master 2>/dev/null || true

# ── Append to log ─────────────────────────────────────────────────────
LOG_FILE="$RUN_DIR/log.jsonl"
echo "{\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"event\":\"rollback_complete\",\"run_id\":\"$RUN_ID\",\"milestone\":\"$MILESTONE_ID\",\"failed_at\":\"$FINAL_STEP\"}" >> "$LOG_FILE"

echo ""
echo "Rollback complete. Review: $FAILURE_REPORT"
echo "To inspect a failed branch:"
echo "  git checkout coder/$RUN_ID/$MILESTONE_ID"
echo "  git diff main...HEAD"
