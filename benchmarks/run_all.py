#!/usr/bin/env python3
"""Run all AXIOM benchmarks: compile, execute, verify, and time."""

import json
import os
import subprocess
import sys
import time
import statistics

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
SUITE_DIR = os.path.join(SCRIPT_DIR, "suite")
MANIFEST_PATH = os.path.join(SUITE_DIR, "manifest.json")
RESULTS_PATH = os.path.join(SCRIPT_DIR, "results.json")

# Discover AXIOM compiler and clang
AXIOM_BIN = None
for candidate in [
    os.path.join(SCRIPT_DIR, "..", "target", "release", "axiom.exe"),
    os.path.join(SCRIPT_DIR, "..", "target", "release", "axiom"),
    os.path.join(SCRIPT_DIR, "..", "target", "debug", "axiom.exe"),
    os.path.join(SCRIPT_DIR, "..", "target", "debug", "axiom"),
]:
    if os.path.isfile(candidate):
        AXIOM_BIN = os.path.abspath(candidate)
        break

CLANG_BIN = None
for candidate in ["clang", "clang-19", "clang-18", "clang-17", "clang-16", "clang-15", "gcc", "cc"]:
    try:
        subprocess.run([candidate, "--version"], capture_output=True, check=True)
        CLANG_BIN = candidate
        break
    except (subprocess.CalledProcessError, FileNotFoundError):
        continue


def compile_axiom(axm_path, out_path):
    """Compile an .axm file to a native binary using the AXIOM compiler."""
    if not AXIOM_BIN:
        return False, "AXIOM compiler not found"
    try:
        result = subprocess.run(
            [AXIOM_BIN, "compile", axm_path, "-o", out_path],
            capture_output=True, text=True, timeout=60
        )
        if result.returncode != 0:
            return False, result.stderr.strip()
        return True, ""
    except subprocess.TimeoutExpired:
        return False, "compilation timed out"
    except Exception as e:
        return False, str(e)


def compile_c(c_path, out_path):
    """Compile a .c file using clang."""
    if not CLANG_BIN:
        return False, "clang not found"
    try:
        result = subprocess.run(
            [CLANG_BIN, "-O2", "-o", out_path, c_path, ""],
            capture_output=True, text=True, timeout=60
        )
        if result.returncode != 0:
            return False, result.stderr.strip()
        return True, ""
    except subprocess.TimeoutExpired:
        return False, "compilation timed out"
    except Exception as e:
        return False, str(e)


def run_binary(bin_path, timeout_sec=30):
    """Run a binary and capture its output."""
    try:
        result = subprocess.run(
            [bin_path],
            capture_output=True, text=True, timeout=timeout_sec
        )
        return result.stdout, result.returncode
    except subprocess.TimeoutExpired:
        return None, -1
    except Exception as e:
        return None, -1


def time_binary(bin_path, runs=5, timeout_sec=30):
    """Time a binary over multiple runs and return median time in ms."""
    times = []
    for _ in range(runs):
        start = time.perf_counter()
        try:
            subprocess.run([bin_path], capture_output=True, timeout=timeout_sec)
        except subprocess.TimeoutExpired:
            return None
        elapsed = (time.perf_counter() - start) * 1000
        times.append(elapsed)
    return statistics.median(times)


def main():
    import argparse
    parser = argparse.ArgumentParser(description="Run AXIOM benchmark suite")
    parser.add_argument("--filter", help="Only run benchmarks matching this name pattern")
    parser.add_argument("--runs", type=int, default=5, help="Number of timing runs (default: 5)")
    parser.add_argument("--no-time", action="store_true", help="Skip timing (just verify correctness)")
    parser.add_argument("--category", help="Only run benchmarks in this category")
    parser.add_argument("--verify-only", action="store_true", help="Only verify outputs, skip timing")
    args = parser.parse_args()

    if not os.path.isfile(MANIFEST_PATH):
        print(f"ERROR: Manifest not found at {MANIFEST_PATH}")
        print("Run generate_benchmarks.py first.")
        sys.exit(1)

    with open(MANIFEST_PATH) as f:
        manifest = json.load(f)

    print(f"AXIOM Benchmark Suite")
    print(f"=====================")
    print(f"AXIOM compiler: {AXIOM_BIN or 'NOT FOUND'}")
    print(f"C compiler:     {CLANG_BIN or 'NOT FOUND'}")
    print(f"Benchmarks:     {len(manifest)}")
    print(f"Timing runs:    {args.runs}")
    print()

    # Build output directory
    build_dir = os.path.join(SCRIPT_DIR, "build")
    os.makedirs(build_dir, exist_ok=True)

    results = []
    total = 0
    passed = 0
    failed = 0
    skipped = 0

    for bench in manifest:
        bid = bench["id"]
        name = bench["name"]
        category = bench["category"]
        expected = bench["expected_output"]
        axm_file = os.path.join(SUITE_DIR, bench["axm"])
        c_file = os.path.join(SUITE_DIR, bench["c"])

        # Apply filters
        if args.filter and args.filter not in name:
            continue
        if args.category and args.category != category:
            continue

        total += 1
        ext = ".exe" if sys.platform == "win32" else ""
        axm_bin = os.path.join(build_dir, f"{bid}_{name}_axm{ext}")
        c_bin = os.path.join(build_dir, f"{bid}_{name}_c{ext}")

        result_entry = {
            "id": bid,
            "name": name,
            "category": category,
            "status": "unknown",
        }

        # Compile AXIOM
        axm_ok, axm_err = compile_axiom(axm_file, axm_bin)
        if not axm_ok:
            result_entry["status"] = "compile_fail_axm"
            result_entry["error"] = axm_err
            results.append(result_entry)
            failed += 1
            print(f"  [{bid}] {name:30s} COMPILE FAIL (axm): {axm_err[:80]}")
            continue

        # Compile C
        c_ok, c_err = compile_c(c_file, c_bin)
        if not c_ok:
            result_entry["status"] = "compile_fail_c"
            result_entry["error"] = c_err
            results.append(result_entry)
            failed += 1
            print(f"  [{bid}] {name:30s} COMPILE FAIL (c): {c_err[:80]}")
            continue

        # Run and verify AXIOM
        axm_out, axm_rc = run_binary(axm_bin)
        c_out, c_rc = run_binary(c_bin)

        if axm_out is None:
            result_entry["status"] = "timeout_axm"
            results.append(result_entry)
            failed += 1
            print(f"  [{bid}] {name:30s} TIMEOUT (axm)")
            continue

        if c_out is None:
            result_entry["status"] = "timeout_c"
            results.append(result_entry)
            failed += 1
            print(f"  [{bid}] {name:30s} TIMEOUT (c)")
            continue

        # Compare outputs
        axm_match = axm_out == expected or axm_out.strip() == expected.strip()
        c_match = c_out == expected or c_out.strip() == expected.strip()
        outputs_match = axm_out.strip() == c_out.strip()

        if not outputs_match:
            result_entry["status"] = "output_mismatch"
            result_entry["axm_output"] = axm_out.strip()
            result_entry["c_output"] = c_out.strip()
            result_entry["expected"] = expected.strip()
            results.append(result_entry)
            failed += 1
            print(f"  [{bid}] {name:30s} MISMATCH axm={axm_out.strip()!r} c={c_out.strip()!r} expected={expected.strip()!r}")
            continue

        # Timing
        axm_ms = None
        c_ms = None
        if not args.no_time and not args.verify_only:
            axm_ms = time_binary(axm_bin, runs=args.runs)
            c_ms = time_binary(c_bin, runs=args.runs)

        result_entry["status"] = "pass"
        result_entry["output"] = axm_out.strip()
        if axm_ms is not None:
            result_entry["axm_time_ms"] = round(axm_ms, 3)
        if c_ms is not None:
            result_entry["c_time_ms"] = round(c_ms, 3)
        if axm_ms and c_ms:
            result_entry["ratio"] = round(axm_ms / c_ms, 3)

        results.append(result_entry)
        passed += 1

        if axm_ms and c_ms:
            ratio = axm_ms / c_ms
            print(f"  [{bid}] {name:30s} PASS  axm={axm_ms:8.1f}ms  c={c_ms:8.1f}ms  ratio={ratio:.2f}x")
        else:
            print(f"  [{bid}] {name:30s} PASS  output={axm_out.strip()!r}")

    # Summary
    print()
    print(f"Results: {passed} passed, {failed} failed, {skipped} skipped, {total} total")

    # Save results
    with open(RESULTS_PATH, "w") as f:
        json.dump({
            "summary": {"total": total, "passed": passed, "failed": failed, "skipped": skipped},
            "benchmarks": results,
        }, f, indent=2)
    print(f"Results saved to {RESULTS_PATH}")


if __name__ == "__main__":
    main()
