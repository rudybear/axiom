// minimp3 core -- C reference: IMDCT-36 + Huffman decode
// Matches the AXIOM port's algorithm for comparison

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>
#include <time.h>

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

#define IMDCT_N    36
#define IMDCT_HALF 18

// ---------------------------------------------------------------------------
// Build IMDCT cosine table: cos_table[n*18 + k] for n=0..35, k=0..17
// ---------------------------------------------------------------------------
static void build_imdct_cos_table(double *table) {
    for (int n = 0; n < 36; n++) {
        for (int k = 0; k < 18; k++) {
            double angle = M_PI / 36.0 * (2.0 * n + 19.0) * (2.0 * k + 1.0);
            table[n * 18 + k] = cos(angle);
        }
    }
}

// ---------------------------------------------------------------------------
// IMDCT-36
// ---------------------------------------------------------------------------
static inline void imdct36(const double *input, double *output, const double *cos_tab) {
    for (int n = 0; n < 36; n++) {
        double sum = 0.0;
        int base = n * 18;
        for (int k = 0; k < 18; k++) {
            sum += input[k] * cos_tab[base + k];
        }
        output[n] = sum;
    }
}

// ---------------------------------------------------------------------------
// Build IMDCT window
// ---------------------------------------------------------------------------
static void build_imdct_window(double *win) {
    for (int n = 0; n < 36; n++) {
        win[n] = sin(M_PI / 36.0 * ((double)n + 0.5));
    }
}

// ---------------------------------------------------------------------------
// Windowed IMDCT-36
// ---------------------------------------------------------------------------
static inline void imdct36_windowed(const double *input, double *output,
                                     const double *cos_tab, const double *window,
                                     double *overlap) {
    imdct36(input, output, cos_tab);

    for (int n = 0; n < 18; n++) {
        double raw = output[n];
        output[n] = raw * window[n] + overlap[n];
    }
    for (int n = 0; n < 18; n++) {
        overlap[n] = output[18 + n] * window[18 + n];
    }
}

// ---------------------------------------------------------------------------
// Huffman table
// ---------------------------------------------------------------------------
static void build_huffman_table(int *huff_val, int *huff_bits) {
    for (int i = 0; i < 256; i++) {
        if ((i & 0x80) == 0) {
            huff_val[i] = 0; huff_bits[i] = 1;
        } else if ((i & 0xC0) == 0x80) {
            huff_val[i] = 1; huff_bits[i] = 2;
        } else if ((i & 0xE0) == 0xC0) {
            huff_val[i] = 2; huff_bits[i] = 3;
        } else if ((i & 0xF0) == 0xE0) {
            huff_val[i] = 3; huff_bits[i] = 4;
        } else if ((i & 0xF8) == 0xF0) {
            huff_val[i] = 4; huff_bits[i] = 5;
        } else if ((i & 0xFC) == 0xF8) {
            huff_val[i] = 5; huff_bits[i] = 6;
        } else if ((i & 0xFE) == 0xFC) {
            huff_val[i] = 6; huff_bits[i] = 7;
        } else if (i == 0xFE) {
            huff_val[i] = 7; huff_bits[i] = 8;
        } else {
            huff_val[i] = 8; huff_bits[i] = 8;
        }
    }
}

// ---------------------------------------------------------------------------
// Huffman decode one symbol
// ---------------------------------------------------------------------------
static inline int huff_decode_one(const uint8_t *bitstream, int bit_pos,
                                   const int *huff_val, const int *huff_bits,
                                   int *out_val) {
    int byte_idx = bit_pos >> 3;
    int bit_off = bit_pos & 7;

    int combined = ((int)bitstream[byte_idx] << 8) | (int)bitstream[byte_idx + 1];
    int shift_amt = 8 - bit_off;
    int lookup = (combined >> shift_amt) & 0xFF;

    *out_val = huff_val[lookup];
    return bit_pos + huff_bits[lookup];
}

// ---------------------------------------------------------------------------
// Huffman decode block
// ---------------------------------------------------------------------------
static inline int huff_decode_block(const uint8_t *bitstream, int bit_pos, int count,
                                     int *output,
                                     const int *huff_val, const int *huff_bits) {
    int pos = bit_pos;
    for (int i = 0; i < count; i++) {
        pos = huff_decode_one(bitstream, pos, huff_val, huff_bits, &output[i]);
    }
    return pos;
}

// ---------------------------------------------------------------------------
// Test IMDCT
// ---------------------------------------------------------------------------
static int test_imdct(void) {
    printf("--- Test: IMDCT-36 ---\n");

    double cos_tab[648];
    build_imdct_cos_table(cos_tab);

    double input[18] = {0};
    double output[36];

    input[0] = 1.0;
    imdct36(input, output, cos_tab);

    double expected_0 = cos(M_PI / 36.0 * 19.0);
    double diff_0 = fabs(output[0] - expected_0);

    int pass = 1;
    if (diff_0 > 0.0000001) {
        printf("FAIL: IMDCT output[0] mismatch\n");
        printf("  expected: %f\n  actual:   %f\n", expected_0, output[0]);
        pass = 0;
    }

    double expected_17 = cos(M_PI / 36.0 * 53.0);
    if (fabs(output[17] - expected_17) > 0.0000001) {
        printf("FAIL: IMDCT output[17] mismatch\n");
        pass = 0;
    }

    for (int k = 0; k < 18; k++) input[k] = 1.0;
    imdct36(input, output, cos_tab);

    double ref_sum = 0.0;
    for (int k = 0; k < 18; k++) {
        ref_sum += cos(M_PI / 36.0 * 19.0 * (2.0 * k + 1.0));
    }
    if (fabs(output[0] - ref_sum) > 0.0001) {
        printf("FAIL: IMDCT all-ones sum mismatch\n");
        pass = 0;
    }

    if (pass) printf("PASS: IMDCT-36 correctness verified\n");
    return pass;
}

// ---------------------------------------------------------------------------
// Test Huffman
// ---------------------------------------------------------------------------
static int test_huffman(void) {
    printf("--- Test: Huffman decode ---\n");

    int huff_val[256], huff_bits[256];
    build_huffman_table(huff_val, huff_bits);

    uint8_t bitstream[4] = {0x5B, 0x88, 0x00, 0x00};
    int decoded[7];
    int final_pos = huff_decode_block(bitstream, 0, 7, decoded, huff_val, huff_bits);

    int expected[] = {0, 1, 2, 3, 0, 0, 1};
    int pass = 1;

    for (int i = 0; i < 7; i++) {
        if (decoded[i] != expected[i]) {
            printf("FAIL: Huffman decode[%d] = %d, expected %d\n", i, decoded[i], expected[i]);
            pass = 0;
        }
    }
    if (final_pos != 14) {
        printf("FAIL: Huffman bit position = %d, expected 14\n", final_pos);
        pass = 0;
    }

    if (pass) printf("PASS: Huffman decode verified\n");
    return pass;
}

// ---------------------------------------------------------------------------
// Benchmark: 1M IMDCT-36
// ---------------------------------------------------------------------------
static void bench_imdct(void) {
    printf("\n--- Benchmark: 1M IMDCT-36 transforms ---\n");

    double cos_tab[648];
    build_imdct_cos_table(cos_tab);

    double window[36];
    build_imdct_window(window);

    double input[18], output[36], overlap[18] = {0};

    for (int k = 0; k < 18; k++) {
        input[k] = sin((double)k * 0.3) * 0.5;
    }

    int iterations = 1000000;
    double checksum = 0.0;

    struct timespec ts0, ts1;
    clock_gettime(CLOCK_MONOTONIC, &ts0);

    for (int iter = 0; iter < iterations; iter++) {
        imdct36_windowed(input, output, cos_tab, window, overlap);
        double tmp = input[0];
        input[0] = output[0] * 0.001 + tmp * 0.999;
        checksum += output[0];
    }

    clock_gettime(CLOCK_MONOTONIC, &ts1);
    long elapsed_ms = (ts1.tv_sec - ts0.tv_sec) * 1000 + (ts1.tv_nsec - ts0.tv_nsec) / 1000000;

    printf("Elapsed: %ld ms\n", elapsed_ms);
    printf("Checksum (prevent DCE): %f\n", checksum);

    if (elapsed_ms > 0) {
        long transforms_per_sec = (long)iterations * 1000 / elapsed_ms;
        printf("Transforms/sec: %ld\n", transforms_per_sec);
    }
}

// ---------------------------------------------------------------------------
// Benchmark: Huffman decode
// ---------------------------------------------------------------------------
static void bench_huffman(void) {
    printf("\n--- Benchmark: Huffman decode 10M symbols ---\n");

    int huff_val[256], huff_bits[256];
    build_huffman_table(huff_val, huff_bits);

    int stream_bytes = 8192;
    uint8_t *bitstream = (uint8_t *)malloc(stream_bytes);
    for (int i = 0; i < stream_bytes; i++) {
        bitstream[i] = (uint8_t)((i * 137 + 43) & 0xFF);
    }

    int decoded[1024];
    int iterations = 10000;
    int symbols_per_iter = 1024;

    struct timespec ts0, ts1;
    clock_gettime(CLOCK_MONOTONIC, &ts0);
    int checksum = 0;

    for (int iter = 0; iter < iterations; iter++) {
        int final_pos = huff_decode_block(bitstream, 0, symbols_per_iter,
                                           decoded, huff_val, huff_bits);
        checksum += decoded[0] + final_pos;
    }

    clock_gettime(CLOCK_MONOTONIC, &ts1);
    long elapsed_ms = (ts1.tv_sec - ts0.tv_sec) * 1000 + (ts1.tv_nsec - ts0.tv_nsec) / 1000000;

    long total_symbols = (long)iterations * symbols_per_iter;
    printf("Total symbols decoded: %ld\n", total_symbols);
    printf("Elapsed: %ld ms\n", elapsed_ms);
    printf("Checksum (prevent DCE): %d\n", checksum);

    if (elapsed_ms > 0) {
        long msyms = total_symbols * 1000 / (elapsed_ms * 1000000);
        printf("M symbols/sec: %ld\n", msyms);
    }

    free(bitstream);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
int main(void) {
    printf("=== minimp3 Core: IMDCT-36 + Huffman Decode ===\n\n");

    int pass1 = test_imdct();
    int pass2 = test_huffman();

    if (pass1 && pass2) {
        printf("\nAll tests passed.\n");
    } else {
        printf("\nSome tests FAILED.\n");
        return 1;
    }

    bench_imdct();
    bench_huffman();

    printf("\n=== minimp3 complete ===\n");
    return 0;
}
