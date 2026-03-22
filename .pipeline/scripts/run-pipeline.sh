#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# AXIOM Pipeline — Quick Runner
# ═══════════════════════════════════════════════════════════════════════
# Convenience wrapper for the orchestrator.
#
# Usage:
#   ./run-pipeline.sh M1.1-lexer       # Run full pipeline for lexer
#   ./run-pipeline.sh M1.2-parser      # Run full pipeline for parser
#   ./run-pipeline.sh --status         # Show pipeline status
#   ./run-pipeline.sh --gate M1.1      # Run gate check only
#   ./run-pipeline.sh --list           # List milestones
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PIPELINE_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(dirname "$PIPELINE_DIR")"

case "${1:-}" in
    "--list"|"-l")
        echo "Available milestones:"
        echo ""
        for f in "$PIPELINE_DIR/milestones/"*.json; do
            ID=$(jq -r '.id' "$f")
            NAME=$(jq -r '.name' "$f")
            STATUS=$(jq -r '.status' "$f")
            DEPS=$(jq -r '.depends_on | join(", ")' "$f")
            AC_COUNT=$(jq '.acceptance_criteria | length' "$f")
            echo "  $ID ($NAME)"
            echo "    Status: $STATUS"
            echo "    Depends on: ${DEPS:-none}"
            echo "    Acceptance criteria: $AC_COUNT"
            echo ""
        done
        ;;

    "--status"|"-s")
        echo "Pipeline Status"
        echo "═══════════════"
        echo ""

        # Show recent runs
        echo "Recent runs:"
        if ls -d "$PIPELINE_DIR/runs/run-"* 2>/dev/null | tail -5 | while read run_dir; do
            RUN_ID=$(basename "$run_dir")
            if [ -f "$run_dir/state.json" ]; then
                STEP=$(jq -r '.current_step' "$run_dir/state.json")
                STATUS=$(jq -r '.status' "$run_dir/state.json")
                MS=$(jq -r '.milestone_id' "$run_dir/state.json")
                echo "  $RUN_ID: $MS — $STEP ($STATUS)"
            fi
        done; then true; else
            echo "  No runs yet."
        fi

        echo ""
        echo "Baselines:"
        if [ -f "$PIPELINE_DIR/benchmarks/baselines.json" ]; then
            jq -r 'to_entries[] | "  \(.key): build=\(.value.cargo_build_time_ms)ms tests=\(.value.test_count) loc=\(.value.total_loc)"' "$PIPELINE_DIR/benchmarks/baselines.json" 2>/dev/null || echo "  (none)"
        else
            echo "  No baselines established yet."
        fi

        echo ""
        echo "Git branches:"
        git -C "$PROJECT_ROOT" branch --list "architect/*" "coder/*" "tester/*" 2>/dev/null | sed 's/^/  /' || echo "  (none)"
        ;;

    "--gate"|"-g")
        if [ $# -lt 2 ]; then
            echo "Usage: $0 --gate <milestone-id>"
            exit 1
        fi
        # Find the most recent run for this milestone
        MILESTONE_ID="$2"
        LATEST_RUN=$(ls -d "$PIPELINE_DIR/runs/run-"* 2>/dev/null | tail -1 | xargs basename || echo "")
        if [ -z "$LATEST_RUN" ]; then
            echo "No runs found. Run the pipeline first."
            exit 1
        fi
        bash "$SCRIPT_DIR/gate-check.sh" "$LATEST_RUN" "$MILESTONE_ID" "tester" "$PROJECT_ROOT"
        ;;

    "--help"|"-h"|"")
        echo "AXIOM Development Pipeline"
        echo ""
        echo "Usage: $0 <command>"
        echo ""
        echo "Commands:"
        echo "  <milestone-id>    Run full pipeline for a milestone"
        echo "  --list, -l        List available milestones"
        echo "  --status, -s      Show pipeline status and recent runs"
        echo "  --gate, -g <ms>   Run gate check for a milestone"
        echo "  --help, -h        Show this help"
        echo ""
        echo "Examples:"
        echo "  $0 M1.1-lexer"
        echo "  $0 M1.2-parser"
        echo "  $0 --status"
        ;;

    *)
        # Run the orchestrator
        bash "$SCRIPT_DIR/orchestrator.sh" "$1"
        ;;
esac
