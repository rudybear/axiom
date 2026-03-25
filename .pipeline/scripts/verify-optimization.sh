#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# AXIOM Post-Optimization Verifier
# ═══════════════════════════════════════════════════════════════════════
# Runs AFTER the Coder agent claims to have applied optimizations.
# Does NOT trust the agent's report — checks the actual files.
#
# Usage: ./verify-optimization.sh [directory]
#   e.g.: ./verify-optimization.sh benchmarks/real_world/
#
# Exit code 0 = all checks pass. Non-zero = issues found.
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

DIR="${1:-.}"
ISSUES=0
WARNINGS=0

echo "═══════════════════════════════════════════════════════════════"
echo "  AXIOM Post-Optimization Verifier"
echo "  Scanning: $DIR"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# ── Check 1: Large stack arrays (Rule 5) ──────────────────────────────
echo "--- Check 1: Large stack arrays (>4KB should use heap_alloc_zeroed) ---"
for f in "$DIR"/*.axm; do
    [ -f "$f" ] || continue
    name=$(basename "$f" .axm)

    # Find array_zeros declarations and extract sizes
    while IFS= read -r line; do
        # Extract type and size from array_zeros[T, N] or array[T, N]
        size=$(echo "$line" | grep -oP 'array_zeros?\[\w+,\s*\K[0-9]+' || true)
        type=$(echo "$line" | grep -oP 'array_zeros?\[\K\w+' || true)

        if [ -n "$size" ] && [ -n "$type" ]; then
            # Calculate byte size
            case "$type" in
                i8|u8) elem_size=1 ;;
                i16|u16) elem_size=2 ;;
                i32|u32|f32) elem_size=4 ;;
                i64|u64|f64) elem_size=8 ;;
                i128|u128) elem_size=16 ;;
                *) elem_size=8 ;;
            esac

            total_bytes=$((size * elem_size))

            if [ "$total_bytes" -gt 4096 ]; then
                total_kb=$((total_bytes / 1024))
                echo "  ISSUE: $name — array[$type, $size] = ${total_kb}KB on stack (should use heap_alloc_zeroed)"
                ISSUES=$((ISSUES + 1))
            fi
        fi
    done < <(grep -n "array_zeros\|: array\[" "$f" 2>/dev/null || true)
done
echo ""

# ── Check 2: Functions missing @pure (Rule 1) ────────────────────────
echo "--- Check 2: Helper functions missing @pure ---"
for f in "$DIR"/*.axm; do
    [ -f "$f" ] || continue
    name=$(basename "$f" .axm)

    # Count functions and @pure annotations
    total_funcs=$(grep -c "^fn \|^@pure" "$f" 2>/dev/null || echo 0)
    pure_count=$(grep -c "@pure" "$f" 2>/dev/null || echo 0)
    func_count=$(grep -c "^fn " "$f" 2>/dev/null || echo 0)
    main_count=$(grep -c "^fn main" "$f" 2>/dev/null || echo 0)

    # Helper functions = total - main
    helpers=$((func_count - main_count))

    if [ "$helpers" -gt 0 ] && [ "$pure_count" -eq 0 ]; then
        echo "  ISSUE: $name — $helpers helper function(s) with NO @pure"
        ISSUES=$((ISSUES + 1))
    elif [ "$helpers" -gt "$pure_count" ]; then
        missing=$((helpers - pure_count))
        echo "  WARNING: $name — $missing of $helpers helpers without @pure"
        WARNINGS=$((WARNINGS + 1))
    fi
done
echo ""

# ── Check 3: Missing @constraint (Rule 2) ────────────────────────────
echo "--- Check 3: Missing @constraint { optimize_for: \"performance\" } ---"
for f in "$DIR"/*.axm; do
    [ -f "$f" ] || continue
    name=$(basename "$f" .axm)

    if ! grep -q "@constraint" "$f" 2>/dev/null; then
        echo "  WARNING: $name — no @constraint annotation"
        WARNINGS=$((WARNINGS + 1))
    fi
done
echo ""

# ── Check 4: Hot helpers missing @inline(always) (Rule 5b/14) ────────
echo "--- Check 4: @pure functions without @inline(always) ---"
for f in "$DIR"/*.axm; do
    [ -f "$f" ] || continue
    name=$(basename "$f" .axm)

    pure_no_inline=0
    prev_line=""
    while IFS= read -r line; do
        if echo "$line" | grep -q "^@pure" 2>/dev/null; then
            if ! echo "$prev_line" | grep -q "@inline" 2>/dev/null; then
                pure_no_inline=$((pure_no_inline + 1))
            fi
        fi
        prev_line="$line"
    done < "$f"

    if [ "$pure_no_inline" -gt 0 ]; then
        echo "  INFO: $name — $pure_no_inline @pure function(s) without @inline(always)"
    fi
done
echo ""

# ── Check 5: Verify LLVM IR attributes (if compilable) ──────────────
echo "--- Check 5: LLVM IR attribute verification ---"
if command -v cargo > /dev/null 2>&1; then
    for f in "$DIR"/*.axm; do
        [ -f "$f" ] || continue
        name=$(basename "$f" .axm)

        ir=$(cargo run -q -p axiom-driver -- compile --emit=llvm-ir "$f" 2>/dev/null || echo "COMPILE_FAIL")

        if [ "$ir" = "COMPILE_FAIL" ]; then
            echo "  ERROR: $name — fails to compile!"
            ISSUES=$((ISSUES + 1))
            continue
        fi

        # Check for memory attributes on @pure functions
        attr_count=$(echo "$ir" | grep -c "memory(none)\|memory(argmem" || echo 0)
        fast_count=$(echo "$ir" | grep -c "fadd fast\|fmul fast" || echo 0)
        noalias_count=$(echo "$ir" | grep -c "noalias" || echo 0)

        if [ "$attr_count" -eq 0 ] && [ "$(grep -c '@pure' "$f" 2>/dev/null)" -gt 0 ]; then
            echo "  ISSUE: $name — has @pure but LLVM IR has NO memory attributes!"
            ISSUES=$((ISSUES + 1))
        fi
    done
else
    echo "  SKIP: cargo not available"
fi
echo ""

# ── Summary ───────────────────────────────────────────────────────────
echo "═══════════════════════════════════════════════════════════════"
echo "  Results: $ISSUES issues, $WARNINGS warnings"
if [ "$ISSUES" -gt 0 ]; then
    echo "  STATUS: FAIL — $ISSUES issue(s) must be fixed"
    echo "═══════════════════════════════════════════════════════════════"
    exit 1
else
    echo "  STATUS: PASS"
    echo "═══════════════════════════════════════════════════════════════"
    exit 0
fi
