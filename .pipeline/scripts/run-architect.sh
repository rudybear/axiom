#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# Run Architect Agent
# ═══════════════════════════════════════════════════════════════════════
# Launches Claude Code as the Architect agent to produce a specification
# for the given milestone.
#
# Usage: ./run-architect.sh <run-id> <milestone-id> <project-root>
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

RUN_ID="$1"
MILESTONE_ID="$2"
PROJECT_ROOT="${3:-.}"

PIPELINE_DIR="$PROJECT_ROOT/.pipeline"
RUN_DIR="$PIPELINE_DIR/runs/$RUN_ID"
MILESTONE_FILE="$PIPELINE_DIR/milestones/${MILESTONE_ID}.json"
TEMPLATE_FILE="$PIPELINE_DIR/templates/architect.md"
OUTPUT_FILE="$RUN_DIR/architect-output.json"
PROMPT_FILE="$RUN_DIR/architect-prompt.md"

cd "$PROJECT_ROOT"

# ── Create branch ─────────────────────────────────────────────────────
BRANCH="architect/$RUN_ID/$MILESTONE_ID"
git checkout -b "$BRANCH" 2>/dev/null || git checkout "$BRANCH"

# ── Assemble prompt ───────────────────────────────────────────────────
cat > "$PROMPT_FILE" <<PROMPTEOF
# Architect Task

## Milestone
$(cat "$MILESTONE_FILE")

## Project Specification (CLAUDE.md)
$(cat "$PROJECT_ROOT/CLAUDE.md")

## Design Document (DESIGN.md)
$(cat "$PROJECT_ROOT/DESIGN.md")

## Current Codebase State
$(find "$PROJECT_ROOT/crates" -name "*.rs" -exec echo "### {}" \; -exec cat {} \; 2>/dev/null || echo "No source files yet")

## Recent Git History
$(git log --oneline -20 2>/dev/null || echo "No git history")

## Instructions

Read all the context above carefully. Then produce a detailed architectural specification for milestone **$MILESTONE_ID**.

Your output MUST be a single JSON object inside a \`\`\`json fenced code block, following the schema in your system prompt.

Key requirements:
1. List every file to create and every file to modify, with exact paths
2. Provide complete Rust type signatures for all public APIs
3. Specify detailed acceptance tests that the Tester can verify
4. Consider error handling, edge cases, and the anti-patterns list
5. Follow existing code patterns from the codebase state above
PROMPTEOF

# ── Run Claude Code ───────────────────────────────────────────────────
echo "Launching Architect agent..."
RAW_OUTPUT="$RUN_DIR/architect-raw-output.txt"

claude --print \
    --system-prompt "$(cat "$TEMPLATE_FILE")" \
    < "$PROMPT_FILE" \
    > "$RAW_OUTPUT" 2>&1 || true

# ── Extract JSON from output ─────────────────────────────────────────
# Find the last ```json ... ``` block in the output
python3 -c "
import re, sys

text = open('$RAW_OUTPUT').read()
matches = re.findall(r'\`\`\`json\s*\n(.*?)\`\`\`', text, re.DOTALL)
if matches:
    print(matches[-1])
else:
    # Try to find any JSON object
    import json
    for line in text.split('\n'):
        line = line.strip()
        if line.startswith('{'):
            try:
                json.loads(line)
                print(line)
                sys.exit(0)
            except:
                pass
    print('{}')
    sys.exit(1)
" > "$OUTPUT_FILE" 2>/dev/null || {
    # Fallback: try to extract with sed/grep if python3 isn't available
    sed -n '/```json/,/```/p' "$RAW_OUTPUT" | sed '1d;$d' > "$OUTPUT_FILE" 2>/dev/null || echo '{}' > "$OUTPUT_FILE"
}

# ── Validate output ───────────────────────────────────────────────────
if jq empty "$OUTPUT_FILE" 2>/dev/null; then
    echo "✓ Architect output is valid JSON"

    # Add envelope
    TEMP_FILE=$(mktemp)
    jq --arg run "$RUN_ID" --arg ms "$MILESTONE_ID" --arg sha "$(git rev-parse HEAD)" \
        '. + {
            "pipeline_version": "1.0",
            "run_id": $run,
            "milestone_id": $ms,
            "agent": "architect",
            "timestamp": (now | todate),
            "git_sha": $sha,
            "status": "complete"
        }' "$OUTPUT_FILE" > "$TEMP_FILE"
    mv "$TEMP_FILE" "$OUTPUT_FILE"

    # Commit
    git add "$OUTPUT_FILE" "$PROMPT_FILE"
    git commit -m "pipeline(architect): specification for $MILESTONE_ID — run $RUN_ID" || true
    echo "✓ Architect specification committed"
else
    echo "✗ Architect output is not valid JSON"
    echo "  Raw output saved to: $RAW_OUTPUT"
    exit 1
fi
