# Self-Improvement Pass 3 — Automated Codebase Analysis

**Date:** 2026-03-25
**Method:** Scanned all 20 real-world benchmarks' LLVM IR for optimization patterns

## Findings

### 1. Stack arrays that should be heap (Rule 5 violations)

| Benchmark | Array | Size | Status |
|-----------|-------|------|--------|
| edge_detection_sobel | `array[i32, 262144]` x2 | 2MB | NOT CONVERTED |
| lz77_compress | `array[i32, 100000]` x4 | 1.6MB | NOT CONVERTED |
| fft_iterative | `array[f64, 8192]` x2 | 128KB | NOT CONVERTED |
| finite_element_1d | `array[f64, 2001]` x13 | 208KB | NOT CONVERTED |

These benchmarks were SUPPOSED to be converted in self-opt pass 1, but the changes didn't persist. The agent reported success but the files weren't actually modified for these specific ones.

**Expected impact:** Converting edge_detection and lz77 to heap_alloc_zeroed should give 2-8x improvement (matching median_filter and RLE results from earlier).

### 2. All 20 benchmarks DO have @pure on their helper functions ✅

No benchmark is missing @pure — the pass 1 fix for this was successful.

### 3. JPEG DCT small arrays are FINE on stack ✅

The 5 `array[f64, 64]` (2.5KB total) are correctly on the stack. This is well under the 4KB threshold.

## New Knowledge Base Rule

### Rule 15: Always verify optimization changes persisted
When an LLM optimization agent reports "fixed X files," VERIFY the changes are actually in the files. The automated pass detected 4 benchmarks where pass 1 claimed to fix stack arrays but the changes weren't saved.

**Verification method:** Scan the source for `array_zeros` and check sizes. Any `array_zeros[T, N]` where `N * sizeof(T) > 4096` should use `heap_alloc_zeroed` instead.
