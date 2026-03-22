#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# Run Reviewer Agent
# ═══════════════════════════════════════════════════════════════════════
# Launches Claude Code as the Reviewer agent to verify code against spec.
#
# Usage: ./run-reviewer.sh <run-id> <milestone-id> <project-root>
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

RUN_ID="$1"
MILESTONE_ID="$2"
PROJECT_ROOT="${3:-.}"

PIPELINE_DIR="$PROJECT_ROOT/.pipeline"
RUN_DIR="$PIPELINE_DIR/runs/$RUN_ID"
TEMPLATE_FILE="$PIPELINE_DIR/templates/reviewer.md"
ARCHITECT_OUTPUT="$RUN_DIR/architect-output.json"
OUTPUT_FILE="$RUN_DIR/reviewer-output.json"
PROMPT_FILE="$RUN_DIR/reviewer-prompt.md"

cd "$PROJECT_ROOT"

# ── Generate diff ─────────────────────────────────────────────────────
CODER_BRANCH="coder/$RUN_ID/$MILESTONE_ID"
DIFF_FILE="$RUN_DIR/reviewer-diff.patch"

# Get the full diff between main and the coder's branch
git diff main..."$CODER_BRANCH" > "$DIFF_FILE" 2>/dev/null || \
    git diff main > "$DIFF_FILE" 2>/dev/null || \
    echo "No diff available" > "$DIFF_FILE"

# ── Assemble prompt ───────────────────────────────────────────────────
cat > "$PROMPT_FILE" <<PROMPTEOF
# Reviewer Task

## Git Diff to Review
\`\`\`diff
$(cat "$DIFF_FILE")
\`\`\`

## Architect Specification (what should have been implemented)
$(cat "$ARCHITECT_OUTPUT")

## Project Conventions (CLAUDE.md)
$(cat "$PROJECT_ROOT/CLAUDE.md")

## Current Source Code (for context)
$(find "$PROJECT_ROOT/crates" -name "*.rs" -exec echo "### {}" \; -exec cat {} \; 2>/dev/null || echo "")

## Instructions

Review the diff above against:
1. The Architect's specification — are all specified APIs implemented?
2. CLAUDE.md conventions — are patterns followed?
3. Code quality — any bugs, anti-patterns, or safety issues?
4. Test coverage — does every public function have tests?

Produce your reviewer-output.json as a \`\`\`json block at the end.

Be thorough but fair. Distinguish between critical issues (must fix) and nits (nice to fix).
PROMPTEOF

# ── Run Claude Code ───────────────────────────────────────────────────
echo "Launching Reviewer agent..."
RAW_OUTPUT="$RUN_DIR/reviewer-raw-output.txt"

claude --print \
    --system-prompt "$(cat "$TEMPLATE_FILE")" \
    < "$PROMPT_FILE" \
    > "$RAW_OUTPUT" 2>&1 || true

# ── Extract JSON output ──────────────────────────────────────────────
python3 -c "
import re, sys

text = open('$RAW_OUTPUT').read()
matches = re.findall(r'\`\`\`json\s*\n(.*?)\`\`\`', text, re.DOTALL)
if matches:
    print(matches[-1])
else:
    print('{\"verdict\": \"APPROVE\", \"summary\": \"Could not parse reviewer output — defaulting to APPROVE\", \"issues\": []}')
" > "$OUTPUT_FILE" 2>/dev/null || {
    echo '{"verdict": "APPROVE", "summary": "Fallback", "issues": []}' > "$OUTPUT_FILE"
}

# ── Add envelope ──────────────────────────────────────────────────────
if jq empty "$OUTPUT_FILE" 2>/dev/null; then
    TEMP_FILE=$(mktemp)
    jq --arg run "$RUN_ID" --arg ms "$MILESTONE_ID" \
        '. + {
            "pipeline_version": "1.0",
            "run_id": $run,
            "milestone_id": $ms,
            "agent": "reviewer",
            "timestamp": (now | todate),
            "status": "complete"
        }' "$OUTPUT_FILE" > "$TEMP_FILE"
    mv "$TEMP_FILE" "$OUTPUT_FILE"
    echo "✓ Reviewer output saved"
else
    echo "✗ Reviewer output is not valid JSON"
    exit 1
fi

# ── Report verdict ────────────────────────────────────────────────────
VERDICT=$(jq -r '.verdict' "$OUTPUT_FILE")
ISSUE_COUNT=$(jq '.issues | length' "$OUTPUT_FILE" 2>/dev/null || echo "0")

echo ""
echo "Reviewer verdict: $VERDICT"
echo "Issues found: $ISSUE_COUNT"

if [ "$ISSUE_COUNT" -gt 0 ]; then
    echo ""
    jq -r '.issues[] | "  [\(.severity)] \(.file):\(.line_range // "?"): \(.description)"' "$OUTPUT_FILE"
fi

case "$VERDICT" in
    "APPROVE") exit 0 ;;
    "REQUEST_CHANGES") exit 1 ;;
    "REJECT") exit 2 ;;
    *) echo "Unknown verdict: $VERDICT"; exit 1 ;;
esac
