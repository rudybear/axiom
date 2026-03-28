// TurboPFor-style integer compression -- C reference implementation
// Matches the AXIOM port's algorithm for comparison

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>

// ---------------------------------------------------------------------------
// Bit-width calculation
// ---------------------------------------------------------------------------

static int find_max_value(const int32_t *input, int n) {
    int max_val = 0;
    for (int i = 0; i < n; i++)
        if (input[i] > max_val) max_val = input[i];
    return max_val;
}

static int bit_width(int val) {
    if (val == 0) return 1;
    int bits = 0;
    unsigned v = (unsigned)val;
    while (v > 0) { bits++; v >>= 1; }
    return bits;
}

// ---------------------------------------------------------------------------
// Bit-pack encode
// ---------------------------------------------------------------------------

static int bitpack_encode(const int32_t *input, int n, uint8_t *output) {
    int max_val = find_max_value(input, n);
    int bits = bit_width(max_val);

    output[0] = (uint8_t)bits; // header
    int out_pos = 1;

    uint32_t bit_buf = 0;
    int bit_count = 0;

    for (int i = 0; i < n; i++) {
        uint32_t val = (uint32_t)input[i] & ((1U << bits) - 1);
        bit_buf |= val << bit_count;
        bit_count += bits;

        while (bit_count >= 8) {
            output[out_pos++] = (uint8_t)(bit_buf & 0xFF);
            bit_buf >>= 8;
            bit_count -= 8;
        }
    }

    if (bit_count > 0)
        output[out_pos++] = (uint8_t)(bit_buf & 0xFF);

    return out_pos;
}

// ---------------------------------------------------------------------------
// Bit-pack decode
// ---------------------------------------------------------------------------

static void bitpack_decode(const uint8_t *input, int n, int bits, int32_t *output) {
    int in_pos = 0;
    uint32_t bit_buf = 0;
    int bit_count = 0;
    uint32_t mask = (1U << bits) - 1;

    for (int i = 0; i < n; i++) {
        while (bit_count < bits) {
            bit_buf |= (uint32_t)input[in_pos++] << bit_count;
            bit_count += 8;
        }
        output[i] = (int32_t)(bit_buf & mask);
        bit_buf >>= bits;
        bit_count -= bits;
    }
}

// ---------------------------------------------------------------------------
// Delta encode/decode
// ---------------------------------------------------------------------------

static void delta_encode(const int32_t *input, int n, int32_t *output) {
    if (n < 1) return;
    output[0] = input[0];
    for (int i = 1; i < n; i++)
        output[i] = input[i] - input[i - 1];
}

static void delta_decode(const int32_t *input, int n, int32_t *output) {
    if (n < 1) return;
    output[0] = input[0];
    for (int i = 1; i < n; i++)
        output[i] = output[i - 1] + input[i];
}

// ---------------------------------------------------------------------------
// VByte encode/decode
// ---------------------------------------------------------------------------

static int vbyte_encode(const int32_t *input, int n, uint8_t *output) {
    int out_pos = 0;
    for (int i = 0; i < n; i++) {
        uint32_t v = (uint32_t)input[i];
        if (v == 0) {
            output[out_pos++] = 0;
            continue;
        }
        while (v > 0) {
            uint8_t byte_val = v & 0x7F;
            v >>= 7;
            if (v > 0) byte_val |= 0x80;
            output[out_pos++] = byte_val;
        }
    }
    return out_pos;
}

static int vbyte_decode(const uint8_t *input, int n, int32_t *output) {
    int in_pos = 0;
    for (int i = 0; i < n; i++) {
        uint32_t val = 0;
        int shift = 0;
        while (1) {
            uint8_t byte_val = input[in_pos++];
            val |= (uint32_t)(byte_val & 0x7F) << shift;
            shift += 7;
            if (!(byte_val & 0x80)) break;
        }
        output[i] = (int32_t)val;
    }
    return in_pos;
}

// ---------------------------------------------------------------------------
// Combined pipelines
// ---------------------------------------------------------------------------

static int delta_bitpack_encode(const int32_t *input, int n,
                                int32_t *delta_buf, uint8_t *output) {
    delta_encode(input, n, delta_buf);
    return bitpack_encode(delta_buf, n, output);
}

static void delta_bitpack_decode(const uint8_t *input, int n,
                                 int32_t *delta_buf, int32_t *output) {
    int bits = input[0];
    bitpack_decode(input + 1, n, bits, delta_buf);
    delta_decode(delta_buf, n, output);
}

// ---------------------------------------------------------------------------
// Verify
// ---------------------------------------------------------------------------

static int arrays_equal(const int32_t *a, const int32_t *b, int n) {
    for (int i = 0; i < n; i++)
        if (a[i] != b[i]) return 0;
    return 1;
}

// ---------------------------------------------------------------------------
// LCG PRNG
// ---------------------------------------------------------------------------

static int lcg_next(int *state) {
    *state = ((*state) * 1103515245 + 12345) & 0x7FFFFFFF;
    return *state;
}

static void generate_sorted_ids(int32_t *output, int n, int seed) {
    int state = seed;
    int cur = 1;
    for (int i = 0; i < n; i++) {
        int gap = (lcg_next(&state) % 20) + 1;
        cur += gap;
        output[i] = cur;
    }
}

static void generate_small_values(int32_t *output, int n, int seed, int max_val) {
    int state = seed;
    for (int i = 0; i < n; i++)
        output[i] = lcg_next(&state) % max_val;
}

// ---------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------

#ifdef _WIN32
#include <windows.h>
static int64_t clock_ns_func(void) {
    LARGE_INTEGER freq, count;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&count);
    return (int64_t)((double)count.QuadPart / freq.QuadPart * 1e9);
}
#else
static int64_t clock_ns_func(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000LL + ts.tv_nsec;
}
#endif

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

int main(void) {
    printf("=== TurboPFor Integer Compression (C reference) ===\n\n");

    int n = 10000;
    int pass_count = 0, total_tests = 0;

    int32_t *input = (int32_t *)malloc(n * sizeof(int32_t));
    int32_t *delta_buf = (int32_t *)malloc(n * sizeof(int32_t));
    int32_t *decoded = (int32_t *)malloc(n * sizeof(int32_t));
    uint8_t *packed_buf = (uint8_t *)malloc(n * 5 + 64);

    // Test 1: Delta encoding
    printf("--- Test 1: Delta encoding (10K sorted IDs) ---\n");
    total_tests++;
    generate_sorted_ids(input, n, 42);
    delta_encode(input, n, delta_buf);
    delta_decode(delta_buf, n, decoded);
    if (arrays_equal(input, decoded, n)) {
        printf("PASS: Delta round-trip verified\n");
        pass_count++;
    } else {
        printf("FAIL: Delta round-trip mismatch\n");
    }
    int max_delta = find_max_value(delta_buf, n);
    printf("Max delta: %d, Bits needed: %d\n", max_delta, bit_width(max_delta));

    // Test 2: Bit-packing
    printf("\n--- Test 2: Bit-packing (10K small values 0..255) ---\n");
    total_tests++;
    generate_small_values(input, n, 123, 256);
    int packed_size = bitpack_encode(input, n, packed_buf);
    int raw_size = n * 4;
    printf("Raw size: %d bytes\n", raw_size);
    printf("Packed size: %d bytes\n", packed_size);
    int bp_bits = packed_buf[0];
    printf("Bit width: %d bits/int\n", bp_bits);
    bitpack_decode(packed_buf + 1, n, bp_bits, decoded);
    if (arrays_equal(input, decoded, n)) {
        printf("PASS: Bitpack round-trip verified\n");
        pass_count++;
    } else {
        printf("FAIL: Bitpack round-trip mismatch\n");
    }
    printf("Compression ratio: %d%% (%dx)\n", (packed_size * 100) / raw_size, raw_size / packed_size);

    // Test 3: VByte
    printf("\n--- Test 3: VByte encoding (10K values 0..1023) ---\n");
    total_tests++;
    generate_small_values(input, n, 456, 1024);
    int vbyte_size = vbyte_encode(input, n, packed_buf);
    printf("Raw size: %d bytes\n", raw_size);
    printf("VByte size: %d bytes\n", vbyte_size);
    vbyte_decode(packed_buf, n, decoded);
    if (arrays_equal(input, decoded, n)) {
        printf("PASS: VByte round-trip verified\n");
        pass_count++;
    } else {
        printf("FAIL: VByte round-trip mismatch\n");
    }
    printf("Compression ratio: %d%% (%dx)\n", (vbyte_size * 100) / raw_size, raw_size / vbyte_size);

    // Test 4: Delta+Bitpack pipeline
    printf("\n--- Test 4: Delta+Bitpack pipeline (10K sorted IDs) ---\n");
    total_tests++;
    generate_sorted_ids(input, n, 789);
    int pipeline_size = delta_bitpack_encode(input, n, delta_buf, packed_buf);
    printf("Raw size: %d bytes\n", raw_size);
    printf("Delta+Bitpack size: %d bytes\n", pipeline_size);
    delta_bitpack_decode(packed_buf, n, delta_buf, decoded);
    if (arrays_equal(input, decoded, n)) {
        printf("PASS: Delta+Bitpack pipeline round-trip verified\n");
        pass_count++;
    } else {
        printf("FAIL: Delta+Bitpack pipeline round-trip mismatch\n");
    }
    printf("Compression ratio: %d%% (%dx)\n",
           (pipeline_size * 100) / raw_size,
           pipeline_size > 0 ? raw_size / pipeline_size : 0);

    // Test 5: Delta+VByte pipeline
    printf("\n--- Test 5: Delta+VByte pipeline (10K sorted IDs) ---\n");
    total_tests++;
    generate_sorted_ids(input, n, 321);
    delta_encode(input, n, delta_buf);
    int dv_size = vbyte_encode(delta_buf, n, packed_buf);
    printf("Raw size: %d bytes\n", raw_size);
    printf("Delta+VByte size: %d bytes\n", dv_size);
    vbyte_decode(packed_buf, n, delta_buf);
    delta_decode(delta_buf, n, decoded);
    if (arrays_equal(input, decoded, n)) {
        printf("PASS: Delta+VByte pipeline round-trip verified\n");
        pass_count++;
    } else {
        printf("FAIL: Delta+VByte pipeline round-trip mismatch\n");
    }
    printf("Compression ratio: %d%% (%dx)\n",
           (dv_size * 100) / raw_size,
           dv_size > 0 ? raw_size / dv_size : 0);

    printf("\n--- Summary ---\n");
    printf("Tests passed: %d/%d\n", pass_count, total_tests);

    // Benchmark: Delta+Bitpack
    printf("\n--- Benchmark: Delta+Bitpack 10K ints x 10K iterations ---\n");
    generate_sorted_ids(input, n, 42);
    int bench_iters = 10000;

    int64_t t0 = clock_ns_func();
    int checksum = 0;
    for (int iter = 0; iter < bench_iters; iter++) {
        int enc_sz = delta_bitpack_encode(input, n, delta_buf, packed_buf);
        delta_bitpack_decode(packed_buf, n, delta_buf, decoded);
        checksum += enc_sz + decoded[0];
    }
    int64_t t1 = clock_ns_func();
    int64_t elapsed_ms = (t1 - t0) / 1000000;

    printf("Elapsed: %lld ms\n", (long long)elapsed_ms);
    printf("Checksum (prevent DCE): %d\n", checksum);
    if (elapsed_ms > 0) {
        int64_t total_ints = (int64_t)bench_iters * n * 2;
        int64_t ints_per_sec = (total_ints * 1000) / elapsed_ms;
        printf("Throughput: %lld M ints/sec\n", (long long)(ints_per_sec / 1000000));
        int64_t total_mb = ((int64_t)bench_iters * n * 4 * 2) / 1048576;
        printf("Throughput: %lld MB/s\n", (long long)((total_mb * 1000) / elapsed_ms));
    }

    // Benchmark: VByte
    printf("\n--- Benchmark: VByte 10K ints x 10K iterations ---\n");
    generate_small_values(input, n, 42, 1024);

    int64_t t2 = clock_ns_func();
    int checksum2 = 0;
    for (int iter = 0; iter < bench_iters; iter++) {
        int vb_sz = vbyte_encode(input, n, packed_buf);
        vbyte_decode(packed_buf, n, decoded);
        checksum2 += vb_sz + decoded[0];
    }
    int64_t t3 = clock_ns_func();
    int64_t elapsed_ms2 = (t3 - t2) / 1000000;

    printf("Elapsed: %lld ms\n", (long long)elapsed_ms2);
    printf("Checksum (prevent DCE): %d\n", checksum2);
    if (elapsed_ms2 > 0) {
        int64_t total_ints2 = (int64_t)bench_iters * n * 2;
        int64_t ints_per_sec2 = (total_ints2 * 1000) / elapsed_ms2;
        printf("Throughput: %lld M ints/sec\n", (long long)(ints_per_sec2 / 1000000));
        int64_t total_mb2 = ((int64_t)bench_iters * n * 4 * 2) / 1048576;
        printf("Throughput: %lld MB/s\n", (long long)((total_mb2 * 1000) / elapsed_ms2));
    }

    free(input);
    free(delta_buf);
    free(decoded);
    free(packed_buf);

    printf("\n=== TurboPFor complete ===\n");
    return 0;
}
