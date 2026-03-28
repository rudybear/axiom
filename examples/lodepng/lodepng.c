// lodepng core -- C reference: CRC32 + Adler32 + fixed-Huffman inflate
// Matches the AXIOM port's algorithm for comparison

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>

// ---------------------------------------------------------------------------
// CRC32
// ---------------------------------------------------------------------------
#define CRC32_POLY 0xEDB88320u

static uint32_t crc32_table[256];

static void build_crc32_table(void) {
    for (int i = 0; i < 256; i++) {
        uint32_t crc = (uint32_t)i;
        for (int j = 0; j < 8; j++) {
            if (crc & 1)
                crc = (crc >> 1) ^ CRC32_POLY;
            else
                crc >>= 1;
        }
        crc32_table[i] = crc;
    }
}

static inline uint32_t crc32(const uint8_t *data, int len) {
    uint32_t crc = 0xFFFFFFFF;
    for (int i = 0; i < len; i++) {
        uint8_t idx = (uint8_t)(crc ^ data[i]);
        crc = (crc >> 8) ^ crc32_table[idx];
    }
    return crc ^ 0xFFFFFFFF;
}

// ---------------------------------------------------------------------------
// Adler32
// ---------------------------------------------------------------------------
#define ADLER_MOD 65521

static inline uint32_t adler32_simple(const uint8_t *data, int len) {
    uint32_t a = 1, b = 0;
    for (int i = 0; i < len; i++) {
        a = (a + data[i]) % ADLER_MOD;
        b = (b + a) % ADLER_MOD;
    }
    return (b << 16) | a;
}

static inline uint32_t adler32_fast(const uint8_t *data, int len) {
    uint32_t a = 1, b = 0;
    int pos = 0;
    int block_size = 5552;
    while (pos < len) {
        int remaining = len - pos;
        int chunk = (remaining < block_size) ? remaining : block_size;
        for (int i = 0; i < chunk; i++) {
            a += data[pos + i];
            b += a;
        }
        a %= ADLER_MOD;
        b %= ADLER_MOD;
        pos += chunk;
    }
    return (b << 16) | a;
}

// ---------------------------------------------------------------------------
// Bit reading (LSB first)
// ---------------------------------------------------------------------------
static inline int read_bits(const uint8_t *data, int bit_pos, int num_bits) {
    int result = 0;
    for (int i = 0; i < num_bits; i++) {
        int byte_idx = (bit_pos + i) >> 3;
        int bit_idx = (bit_pos + i) & 7;
        int bit = (data[byte_idx] >> bit_idx) & 1;
        result |= (bit << i);
    }
    return result;
}

static inline int reverse_bits(int val, int num_bits) {
    int result = 0;
    int v = val;
    for (int i = 0; i < num_bits; i++) {
        result = (result << 1) | (v & 1);
        v >>= 1;
    }
    return result;
}

// ---------------------------------------------------------------------------
// Fixed Huffman decode
// ---------------------------------------------------------------------------
static inline int decode_fixed_litlen(const uint8_t *data, int bit_pos, int *sym_out) {
    int bits7 = read_bits(data, bit_pos, 7);
    int code7 = reverse_bits(bits7, 7);

    if (code7 == 0) {
        *sym_out = 256;
        return bit_pos + 7;
    }
    if (code7 >= 1 && code7 <= 23) {
        *sym_out = 256 + code7;
        return bit_pos + 7;
    }

    int bits8 = read_bits(data, bit_pos, 8);
    int code8 = reverse_bits(bits8, 8);

    if (code8 >= 0x30 && code8 <= 0xBF) {
        *sym_out = code8 - 0x30;
        return bit_pos + 8;
    }
    if (code8 >= 0xC0 && code8 <= 0xC7) {
        *sym_out = 280 + code8 - 0xC0;
        return bit_pos + 8;
    }

    int bits9 = read_bits(data, bit_pos, 9);
    int code9 = reverse_bits(bits9, 9);

    if (code9 >= 0x190 && code9 <= 0x1FF) {
        *sym_out = 144 + code9 - 0x190;
        return bit_pos + 9;
    }

    *sym_out = 256;
    return bit_pos + 7;
}

// Length/distance tables
static const int len_base[29] = {3,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258};
static const int len_extra[29] = {0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,0};
static const int dist_base[30] = {1,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,1025,1537,2049,3073,4097,6145,8193,12289,16385,24577};
static const int dist_extra_tbl[30] = {0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13};

static int inflate_fixed(const uint8_t *src, int src_bit_start,
                          uint8_t *dst, int dst_max) {
    int bit_pos = src_bit_start;
    int out_pos = 0;

    while (out_pos < dst_max) {
        int sym;
        bit_pos = decode_fixed_litlen(src, bit_pos, &sym);

        if (sym < 256) {
            dst[out_pos++] = (uint8_t)sym;
        } else if (sym == 256) {
            break;
        } else {
            int len_idx = sym - 257;
            if (len_idx < 0 || len_idx > 28) return 0;
            int length = len_base[len_idx];
            int eb = len_extra[len_idx];
            if (eb > 0) {
                length += read_bits(src, bit_pos, eb);
                bit_pos += eb;
            }

            int dist_code = reverse_bits(read_bits(src, bit_pos, 5), 5);
            bit_pos += 5;
            if (dist_code > 29) return 0;
            int distance = dist_base[dist_code];
            int deb = dist_extra_tbl[dist_code];
            if (deb > 0) {
                distance += read_bits(src, bit_pos, deb);
                bit_pos += deb;
            }

            int src_pos = out_pos - distance;
            if (src_pos < 0) return 0;
            for (int i = 0; i < length; i++) {
                if (out_pos >= dst_max) return 0;
                dst[out_pos] = dst[src_pos + i];
                out_pos++;
            }
        }
    }
    return out_pos;
}

// Simple deflate encoder (literals only)
static int write_bits_fn(uint8_t *dst, int bit_pos, int val, int num_bits) {
    for (int i = 0; i < num_bits; i++) {
        int byte_idx = (bit_pos + i) >> 3;
        int bit_idx = (bit_pos + i) & 7;
        int bit = (val >> i) & 1;
        if (bit_idx == 0) dst[byte_idx] = 0;
        dst[byte_idx] |= (uint8_t)(bit << bit_idx);
    }
    return bit_pos + num_bits;
}

static int write_fixed_literal(uint8_t *dst, int bit_pos, int lit) {
    if (lit <= 143) {
        int code = reverse_bits(0x30 + lit, 8);
        return write_bits_fn(dst, bit_pos, code, 8);
    } else if (lit <= 255) {
        int code = reverse_bits(0x190 + lit - 144, 9);
        return write_bits_fn(dst, bit_pos, code, 9);
    } else if (lit == 256) {
        int code = reverse_bits(0, 7);
        return write_bits_fn(dst, bit_pos, code, 7);
    }
    return bit_pos;
}

static int deflate_fixed_literals(const uint8_t *src, int src_len,
                                   uint8_t *dst, int dst_max_bits) {
    int bit_pos = 0;
    bit_pos = write_bits_fn(dst, bit_pos, 1, 1);
    bit_pos = write_bits_fn(dst, bit_pos, 1, 2);
    for (int i = 0; i < src_len; i++) {
        bit_pos = write_fixed_literal(dst, bit_pos, src[i]);
    }
    bit_pos = write_fixed_literal(dst, bit_pos, 256);
    return bit_pos;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
static int test_crc32_fn(void) {
    printf("--- Test: CRC32 ---\n");
    build_crc32_table();
    const uint8_t *data = (const uint8_t *)"123456789";
    uint32_t result = crc32(data, 9);
    if (result != 0xCBF43926) {
        printf("FAIL: CRC32 = 0x%08X, expected 0xCBF43926\n", result);
        return 0;
    }
    printf("PASS: CRC32(\"123456789\") correct\n");
    return 1;
}

static int test_adler32_fn(void) {
    printf("--- Test: Adler32 ---\n");
    const uint8_t *data = (const uint8_t *)"Wikipedia";
    uint32_t result = adler32_simple(data, 9);
    if (result != 0x11E60398) {
        printf("FAIL: Adler32 = 0x%08X, expected 0x11E60398\n", result);
        return 0;
    }
    printf("PASS: Adler32(\"Wikipedia\") correct\n");
    uint32_t result_fast = adler32_fast(data, 9);
    if (result_fast != 0x11E60398) {
        printf("FAIL: Adler32_fast mismatch\n");
        return 0;
    }
    printf("PASS: Adler32_fast matches\n");
    return 1;
}

static int test_inflate_fn(void) {
    printf("--- Test: Inflate round-trip ---\n");
    const uint8_t src[] = "Hello, World! Hello, World!\n";
    int src_len = 28;

    uint8_t comp[256];
    memset(comp, 0, sizeof(comp));
    int comp_bits = deflate_fixed_literals(src, src_len, comp, 256 * 8);
    int comp_bytes = (comp_bits + 7) / 8;
    printf("Original: %d bytes, Compressed: %d bytes (%d bits)\n", src_len, comp_bytes, comp_bits);

    uint8_t decomp[128];
    int decomp_len = inflate_fixed(comp, 3, decomp, 128);

    if (decomp_len != src_len) {
        printf("FAIL: Decompressed size = %d, expected %d\n", decomp_len, src_len);
        return 0;
    }
    if (memcmp(src, decomp, src_len) != 0) {
        printf("FAIL: Content mismatch\n");
        return 0;
    }
    printf("PASS: Inflate round-trip verified\n");
    return 1;
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------
static void bench_crc32_fn(void) {
    printf("\n--- Benchmark: CRC32 ---\n");
    build_crc32_table();

    int data_len = 4096;
    uint8_t *data = (uint8_t *)malloc(data_len);
    for (int i = 0; i < data_len; i++) data[i] = (uint8_t)((i * 37 + 13) & 0xFF);

    int iterations = 200000;
    uint32_t checksum = 0;

    struct timespec ts0, ts1;
    clock_gettime(CLOCK_MONOTONIC, &ts0);
    for (int iter = 0; iter < iterations; iter++) {
        checksum += crc32(data, data_len);
    }
    clock_gettime(CLOCK_MONOTONIC, &ts1);
    long elapsed_ms = (ts1.tv_sec - ts0.tv_sec) * 1000 + (ts1.tv_nsec - ts0.tv_nsec) / 1000000;

    printf("Elapsed: %ld ms\n", elapsed_ms);
    printf("Checksum (prevent DCE): %u\n", checksum);
    if (elapsed_ms > 0) {
        long total_mb = (long)iterations * data_len / 1048576;
        long throughput = total_mb * 1000 / elapsed_ms;
        printf("Throughput: %ld MB/s\n", throughput);
    }
    free(data);
}

static void bench_adler32_fn(void) {
    printf("\n--- Benchmark: Adler32 ---\n");
    int data_len = 4096;
    uint8_t *data = (uint8_t *)malloc(data_len);
    for (int i = 0; i < data_len; i++) data[i] = (uint8_t)((i * 37 + 13) & 0xFF);

    int iterations = 200000;
    uint32_t checksum = 0;

    struct timespec ts0, ts1;
    clock_gettime(CLOCK_MONOTONIC, &ts0);
    for (int iter = 0; iter < iterations; iter++) {
        checksum += adler32_fast(data, data_len);
    }
    clock_gettime(CLOCK_MONOTONIC, &ts1);
    long elapsed_ms = (ts1.tv_sec - ts0.tv_sec) * 1000 + (ts1.tv_nsec - ts0.tv_nsec) / 1000000;

    printf("Elapsed: %ld ms\n", elapsed_ms);
    printf("Checksum (prevent DCE): %u\n", checksum);
    if (elapsed_ms > 0) {
        long total_mb = (long)iterations * data_len / 1048576;
        long throughput = total_mb * 1000 / elapsed_ms;
        printf("Throughput: %ld MB/s\n", throughput);
    }
    free(data);
}

static void bench_inflate_fn(void) {
    printf("\n--- Benchmark: Inflate 4KB x 50K ---\n");
    int src_len = 4096;
    uint8_t *src = (uint8_t *)malloc(src_len);
    for (int i = 0; i < src_len; i++) src[i] = (uint8_t)((i * 37 + 13) & 0xFF);

    uint8_t *comp = (uint8_t *)calloc(src_len * 2 + 16, 1);
    int comp_bits = deflate_fixed_literals(src, src_len, comp, (src_len * 2 + 16) * 8);

    uint8_t *decomp = (uint8_t *)malloc(src_len + 64);
    int iterations = 50000;
    int checksum = 0;

    struct timespec ts0, ts1;
    clock_gettime(CLOCK_MONOTONIC, &ts0);
    for (int iter = 0; iter < iterations; iter++) {
        int dlen = inflate_fixed(comp, 3, decomp, src_len + 64);
        checksum += dlen;
    }
    clock_gettime(CLOCK_MONOTONIC, &ts1);
    long elapsed_ms = (ts1.tv_sec - ts0.tv_sec) * 1000 + (ts1.tv_nsec - ts0.tv_nsec) / 1000000;

    printf("Elapsed: %ld ms\n", elapsed_ms);
    printf("Checksum (prevent DCE): %d\n", checksum);
    if (elapsed_ms > 0) {
        long total_mb = (long)iterations * src_len / 1048576;
        long throughput = total_mb * 1000 / elapsed_ms;
        printf("Throughput: %ld MB/s\n", throughput);
    }
    free(src);
    free(comp);
    free(decomp);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
int main(void) {
    printf("=== lodepng Core: CRC32 + Adler32 + Inflate ===\n\n");

    int pass1 = test_crc32_fn();
    int pass2 = test_adler32_fn();
    int pass3 = test_inflate_fn();

    if (pass1 && pass2 && pass3) {
        printf("\nAll tests passed.\n");
    } else {
        printf("\nSome tests FAILED.\n");
        return 1;
    }

    bench_crc32_fn();
    bench_adler32_fn();
    bench_inflate_fn();

    printf("\n=== lodepng complete ===\n");
    return 0;
}
