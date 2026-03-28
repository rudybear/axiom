// LZ4 compression core -- C reference implementation
// Matches the AXIOM port's algorithm for comparison

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>

#define HASH_TABLE_SIZE 4096
#define HASH_LOG 12
#define MIN_MATCH 4
#define ML_BITS 4
#define ML_MASK 15
#define RUN_BITS 4
#define RUN_MASK 15

static inline uint32_t read32_le(const uint8_t *p) {
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) |
           ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

static inline void write16_le(uint8_t *p, uint16_t val) {
    p[0] = val & 0xFF;
    p[1] = (val >> 8) & 0xFF;
}

static inline uint16_t read16_le_val(const uint8_t *p) {
    return (uint16_t)p[0] | ((uint16_t)p[1] << 8);
}

static inline int lz4_hash(uint32_t val) {
    return (int)((val * 2654435761U) >> 20) & 4095;
}

static int write_extended_length(uint8_t *dst, int pos, int length) {
    int rem = length;
    while (rem >= 255) {
        dst[pos++] = 255;
        rem -= 255;
    }
    dst[pos++] = (uint8_t)rem;
    return pos;
}

int lz4_compress(const uint8_t *src, int src_len,
                 uint8_t *dst, int dst_max) {
    if (src_len < 1) return 0;

    int hash_table[HASH_TABLE_SIZE];
    memset(hash_table, 0, sizeof(hash_table));

    int src_pos = 0, dst_pos = 0, anchor = 0;
    int match_limit = src_len - 5;

    if (src_len < MIN_MATCH) {
        int lit_len = src_len;
        if (lit_len >= RUN_MASK) {
            dst[dst_pos++] = (uint8_t)(RUN_MASK << ML_BITS);
            dst_pos = write_extended_length(dst, dst_pos, lit_len - RUN_MASK);
        } else {
            dst[dst_pos++] = (uint8_t)(lit_len << ML_BITS);
        }
        memcpy(dst + dst_pos, src + anchor, lit_len);
        dst_pos += lit_len;
        return dst_pos;
    }

    while (src_pos <= match_limit) {
        uint32_t cur_val = read32_le(src + src_pos);
        int h = lz4_hash(cur_val);
        int ref_pos = hash_table[h];
        hash_table[h] = src_pos;

        int offset = src_pos - ref_pos;
        if (ref_pos > 0 && offset > 0 && offset < 65536 && src_pos > ref_pos) {
            uint32_t ref_val = read32_le(src + ref_pos);
            if (cur_val == ref_val) {
                int match_len = MIN_MATCH;
                int max_extend = src_len - src_pos - MIN_MATCH;
                while (match_len < max_extend &&
                       src[ref_pos + match_len] == src[src_pos + match_len]) {
                    match_len++;
                }

                int lit_len = src_pos - anchor;
                int ml_code = match_len - MIN_MATCH;
                int token_lit = lit_len > RUN_MASK ? RUN_MASK : lit_len;
                int token_ml = ml_code > ML_MASK ? ML_MASK : ml_code;

                if (dst_pos + 1 + lit_len + 2 + 4 > dst_max) return 0;

                dst[dst_pos++] = (uint8_t)((token_lit << ML_BITS) | token_ml);
                if (lit_len >= RUN_MASK)
                    dst_pos = write_extended_length(dst, dst_pos, lit_len - RUN_MASK);
                memcpy(dst + dst_pos, src + anchor, lit_len);
                dst_pos += lit_len;
                write16_le(dst + dst_pos, (uint16_t)offset);
                dst_pos += 2;
                if (ml_code >= ML_MASK)
                    dst_pos = write_extended_length(dst, dst_pos, ml_code - ML_MASK);

                src_pos += match_len;
                anchor = src_pos;

                if (src_pos <= match_limit) {
                    uint32_t skip_val = read32_le(src + src_pos);
                    hash_table[lz4_hash(skip_val)] = src_pos;
                }
                continue;
            }
        }
        src_pos++;
    }

    int last_lit_len = src_len - anchor;
    if (last_lit_len > 0) {
        int token_lit2 = last_lit_len > RUN_MASK ? RUN_MASK : last_lit_len;
        dst[dst_pos++] = (uint8_t)(token_lit2 << ML_BITS);
        if (last_lit_len >= RUN_MASK)
            dst_pos = write_extended_length(dst, dst_pos, last_lit_len - RUN_MASK);
        memcpy(dst + dst_pos, src + anchor, last_lit_len);
        dst_pos += last_lit_len;
    }

    return dst_pos;
}

int lz4_decompress(const uint8_t *src, int src_len,
                   uint8_t *dst, int dst_max) {
    int src_pos = 0, dst_pos = 0;

    while (src_pos < src_len) {
        int token = src[src_pos++];
        int lit_len = token >> ML_BITS;
        if (lit_len == RUN_MASK) {
            int extra;
            do {
                if (src_pos >= src_len) return 0;
                extra = src[src_pos++];
                lit_len += extra;
            } while (extra == 255);
        }

        if (dst_pos + lit_len > dst_max) return 0;
        memcpy(dst + dst_pos, src + src_pos, lit_len);
        src_pos += lit_len;
        dst_pos += lit_len;

        if (src_pos >= src_len) break;

        if (src_pos + 2 > src_len) return 0;
        int match_offset = read16_le_val(src + src_pos);
        src_pos += 2;
        if (match_offset == 0) return 0;

        int match_len = (token & ML_MASK) + MIN_MATCH;
        if ((token & ML_MASK) == ML_MASK) {
            int extra2;
            do {
                if (src_pos >= src_len) return 0;
                extra2 = src[src_pos++];
                match_len += extra2;
            } while (extra2 == 255);
        }

        int match_src = dst_pos - match_offset;
        if (match_src < 0 || dst_pos + match_len > dst_max) return 0;
        for (int i = 0; i < match_len; i++)
            dst[dst_pos + i] = dst[match_src + i];
        dst_pos += match_len;
    }
    return dst_pos;
}

void fill_repeating(uint8_t *buf, int len) {
    for (int i = 0; i < len; i++)
        buf[i] = (uint8_t)((i & 7) + 65);
}

void fill_text_like(uint8_t *buf, int len) {
    const char *pat = "the quick brown fox jumps over the lazy dog ";
    int plen = 44;
    for (int i = 0; i < len; i++)
        buf[i] = (uint8_t)pat[i % plen];
}

int main(void) {
    printf("=== LZ4 C Reference ===\n");

    // Test: text-like 4096 bytes
    int test_len = 4096;
    uint8_t *src_buf = (uint8_t *)malloc(test_len);
    fill_text_like(src_buf, test_len);
    int comp_max = test_len + test_len / 4 + 16;
    uint8_t *comp_buf = (uint8_t *)malloc(comp_max);
    uint8_t *decomp_buf = (uint8_t *)malloc(test_len + 64);

    int comp_size = lz4_compress(src_buf, test_len, comp_buf, comp_max);
    printf("Original: %d bytes\n", test_len);
    printf("Compressed: %d bytes (%d%%)\n", comp_size, comp_size * 100 / test_len);

    int decomp_size = lz4_decompress(comp_buf, comp_size, decomp_buf, test_len + 64);
    if (decomp_size == test_len && memcmp(src_buf, decomp_buf, test_len) == 0)
        printf("PASS: Round-trip verified\n");
    else
        printf("FAIL: Round-trip mismatch\n");

    // Benchmark
    int iterations = 50000;
    printf("\nBenchmark: 4KB text x 50K compress+decompress\n");

    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int checksum = 0;

    for (int i = 0; i < iterations; i++) {
        int csz = lz4_compress(src_buf, test_len, comp_buf, comp_max);
        int dsz = lz4_decompress(comp_buf, csz, decomp_buf, test_len + 64);
        checksum += csz + dsz;
    }

    clock_gettime(CLOCK_MONOTONIC, &t1);
    long elapsed_ms = (t1.tv_sec - t0.tv_sec) * 1000 +
                      (t1.tv_nsec - t0.tv_nsec) / 1000000;

    printf("Elapsed: %ld ms\n", elapsed_ms);
    printf("Checksum: %d\n", checksum);
    if (elapsed_ms > 0) {
        long total_mb = ((long)iterations * test_len * 2) / 1048576;
        printf("Throughput: %ld MB/s\n", total_mb * 1000 / elapsed_ms);
    }

    free(src_buf);
    free(comp_buf);
    free(decomp_buf);

    printf("=== LZ4 C complete ===\n");
    return 0;
}
