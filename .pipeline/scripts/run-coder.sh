#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# Run Coder Agent
# ═══════════════════════════════════════════════════════════════════════
# Launches Claude Code as the Coder agent to implement the Architect's
# specification.
#
# Usage: ./run-coder.sh <run-id> <milestone-id> <project-root>
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

RUN_ID="$1"
MILESTONE_ID="$2"
PROJECT_ROOT="${3:-.}"

PIPELINE_DIR="$PROJECT_ROOT/.pipeline"
RUN_DIR="$PIPELINE_DIR/runs/$RUN_ID"
MILESTONE_FILE="$PIPELINE_DIR/milestones/${MILESTONE_ID}.json"
TEMPLATE_FILE="$PIPELINE_DIR/templates/coder.md"
ARCHITECT_OUTPUT="$RUN_DIR/architect-output.json"
OUTPUT_FILE="$RUN_DIR/coder-output.json"
PROMPT_FILE="$RUN_DIR/coder-prompt.md"
REVIEWER_OUTPUT="$RUN_DIR/reviewer-output.json"

cd "$PROJECT_ROOT"

# ── Create/switch branch ──────────────────────────────────────────────
BRANCH="coder/$RUN_ID/$MILESTONE_ID"
ARCHITECT_BRANCH="architect/$RUN_ID/$MILESTONE_ID"

if git rev-parse --verify "$BRANCH" > /dev/null 2>&1; then
    git checkout "$BRANCH"
else
    # Branch from architect branch if it exists, otherwise from main
    if git rev-parse --verify "$ARCHITECT_BRANCH" > /dev/null 2>&1; then
        git checkout -b "$BRANCH" "$ARCHITECT_BRANCH"
    else
        git checkout -b "$BRANCH"
    fi
fi

# ── Assemble prompt ───────────────────────────────────────────────────
REVIEW_CONTEXT=""
if [ -f "$REVIEWER_OUTPUT" ]; then
    REVIEW_CONTEXT="
## Previous Review Feedback
The Reviewer found issues with your previous implementation. Fix these:

$(cat "$REVIEWER_OUTPUT")
"
fi

cat > "$PROMPT_FILE" <<PROMPTEOF
# Coder Task

## Architect Specification
$(cat "$ARCHITECT_OUTPUT")

## Milestone Definition
$(cat "$MILESTONE_FILE")

## Project Conventions (CLAUDE.md)
$(cat "$PROJECT_ROOT/CLAUDE.md")
$REVIEW_CONTEXT

## Current Codebase
$(find "$PROJECT_ROOT/crates" -name "*.rs" -exec echo "### {}" \; -exec cat {} \; 2>/dev/null || echo "No source files yet")

## Current Cargo.toml (workspace)
$(cat "$PROJECT_ROOT/Cargo.toml")

## Instructions

Implement the Architect's specification for milestone **$MILESTONE_ID**.

1. Read the specification carefully — implement exactly what's specified
2. Follow all conventions from CLAUDE.md
3. Write unit tests for every public function
4. Ensure \`cargo check\` and \`cargo clippy\` pass
5. Make atomic git commits with proper commit messages
6. Produce your coder-output.json as a \`\`\`json block at the end

IMPORTANT: Actually write the Rust source files. Do not just describe what to write.
PROMPTEOF

# ── Run Claude Code ───────────────────────────────────────────────────
echo "Launching Coder agent..."
RAW_OUTPUT="$RUN_DIR/coder-raw-output.txt"

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
    print('{}')
    sys.exit(1)
" > "$OUTPUT_FILE" 2>/dev/null || echo '{}' > "$OUTPUT_FILE"

# ── Verify code compiles ─────────────────────────────────────────────
echo "Running cargo check..."
if cargo check --workspace 2>&1 | tee "$RUN_DIR/coder-cargo-check.txt"; then
    echo "✓ cargo check passed"
    CARGO_STATUS="pass"
else
    echo "✗ cargo check failed"
    CARGO_STATUS="fail"
fi

echo "Running cargo clippy..."
AFFECTED_CRATES=$(jq -r '.crates_affected[]' "$MILESTONE_FILE" 2>/dev/null || echo "")
CLIPPY_STATUS="pass"
for crate in $AFFECTED_CRATES; do
    if ! cargo clippy -p "$crate" -- -D warnings 2>&1; then
        CLIPPY_STATUS="fail"
    fi
done

# ── Update output with status ────────────────────────────────────────
if jq empty "$OUTPUT_FILE" 2>/dev/null; then
    TEMP_FILE=$(mktemp)
    jq --arg run "$RUN_ID" --arg ms "$MILESTONE_ID" --arg sha "$(git rev-parse HEAD)" \
       --arg cargo "$CARGO_STATUS" --arg clippy "$CLIPPY_STATUS" \
        '. + {
            "pipeline_version": "1.0",
            "run_id": $run,
            "milestone_id": $ms,
            "agent": "coder",
            "timestamp": (now | todate),
            "git_sha": $sha,
            "cargo_check_status": $cargo,
            "cargo_clippy_status": $clippy,
            "status": (if $cargo == "pass" then "complete" else "failed" end)
        }' "$OUTPUT_FILE" > "$TEMP_FILE"
    mv "$TEMP_FILE" "$OUTPUT_FILE"
fi

# ── Commit results ────────────────────────────────────────────────────
git add -A
git commit -m "pipeline(coder): implementation for $MILESTONE_ID — run $RUN_ID" || true

if [ "$CARGO_STATUS" = "fail" ]; then
    echo "✗ Code does not compile — will retry"
    exit 1
fi

echo "✓ Coder implementation complete"
