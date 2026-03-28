// FastLZ compression core -- C reference implementation
// Matches the AXIOM port's algorithm for comparison

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>

#define HASH_SIZE 8192
#define HASH_MASK 8191
#define MAX_DISTANCE 8191
#define MIN_MATCH 3
#define MAX_SHORT_MATCH 8
#define MAX_LIT_RUN 32
#define MAX_MATCH 264

static inline int fastlz_hash(uint8_t b0, uint8_t b1, uint8_t b2) {
    uint32_t h = (b0 * 2654435761U) + (b1 * 340573321U) + (b2 * 1262308561U);
    return (int)((h >> 19) & HASH_MASK);
}

static int emit_literals(const uint8_t *src, int src_start, int count,
                         uint8_t *dst, int dst_pos) {
    int remaining = count;
    int sp = src_start;
    int p = dst_pos;
    while (remaining > 0) {
        int run = remaining > MAX_LIT_RUN ? MAX_LIT_RUN : remaining;
        dst[p++] = (uint8_t)(run - 1);
        memcpy(dst + p, src + sp, run);
        p += run;
        sp += run;
        remaining -= run;
    }
    return p;
}

static int emit_backref(uint8_t *dst, int dst_pos, int distance, int match_len) {
    int p = dst_pos;
    int dist = distance - 1;
    int dist_hi = (dist >> 8) & 31;
    int dist_lo = dist & 255;

    if (match_len <= MAX_SHORT_MATCH) {
        int len_code = match_len - 2;
        dst[p++] = (uint8_t)((len_code << 5) | dist_hi);
        dst[p++] = (uint8_t)dist_lo;
    } else {
        dst[p++] = (uint8_t)((7 << 5) | dist_hi);
        int ext = match_len - 9;
        while (ext >= 255) { dst[p++] = 255; ext -= 255; }
        dst[p++] = (uint8_t)ext;
        dst[p++] = (uint8_t)dist_lo;
    }
    return p;
}

int fastlz_compress(const uint8_t *src, int src_len,
                    uint8_t *dst, int dst_max) {
    if (src_len < 4) {
        if (src_len == 0) return 0;
        return emit_literals(src, 0, src_len, dst, 0);
    }

    int htab[HASH_SIZE];
    memset(htab, 0, sizeof(htab));

    int src_pos = 0, dst_pos = 0, anchor = 0;

    while (src_pos < src_len - 2) {
        int h = fastlz_hash(src[src_pos], src[src_pos+1], src[src_pos+2]);
        int ref_pos = htab[h];
        htab[h] = src_pos;

        int dist = src_pos - ref_pos;
        if (ref_pos > 0 && dist > 0 && dist <= MAX_DISTANCE && src_pos > ref_pos &&
            src[ref_pos] == src[src_pos] &&
            src[ref_pos+1] == src[src_pos+1] &&
            src[ref_pos+2] == src[src_pos+2]) {

            int match_len = MIN_MATCH;
            int max_ext = src_len - src_pos;
            if (max_ext > MAX_MATCH) max_ext = MAX_MATCH;
            while (match_len < max_ext && src[ref_pos+match_len] == src[src_pos+match_len])
                match_len++;

            int lit_count = src_pos - anchor;
            if (lit_count > 0)
                dst_pos = emit_literals(src, anchor, lit_count, dst, dst_pos);
            dst_pos = emit_backref(dst, dst_pos, dist, match_len);

            src_pos += match_len;
            anchor = src_pos;
            if (src_pos < src_len - 2) {
                int h2 = fastlz_hash(src[src_pos], src[src_pos+1], src[src_pos+2]);
                htab[h2] = src_pos;
            }
            continue;
        }
        src_pos++;
    }

    int final_lits = src_len - anchor;
    if (final_lits > 0)
        dst_pos = emit_literals(src, anchor, final_lits, dst, dst_pos);

    return dst_pos;
}

int fastlz_decompress(const uint8_t *src, int src_len,
                      uint8_t *dst, int dst_max) {
    int src_pos = 0, dst_pos = 0;

    while (src_pos < src_len) {
        int tag = src[src_pos++];
        int opcode = tag >> 5;

        if (opcode == 0) {
            int run_len = (tag & 31) + 1;
            if (src_pos + run_len > src_len || dst_pos + run_len > dst_max) return 0;
            memcpy(dst + dst_pos, src + src_pos, run_len);
            src_pos += run_len;
            dst_pos += run_len;
        } else {
            int match_len;
            int dist_hi = tag & 31;

            if (opcode == 7) {
                match_len = 9;
                int ext;
                do {
                    if (src_pos >= src_len) return 0;
                    ext = src[src_pos++];
                    match_len += ext;
                } while (ext == 255);
            } else {
                match_len = opcode + 2;
            }

            if (src_pos >= src_len) return 0;
            int dist_lo = src[src_pos++];
            int distance = ((dist_hi << 8) | dist_lo) + 1;

            int match_src = dst_pos - distance;
            if (match_src < 0 || dst_pos + match_len > dst_max) return 0;
            for (int i = 0; i < match_len; i++)
                dst[dst_pos + i] = dst[match_src + i];
            dst_pos += match_len;
        }
    }
    return dst_pos;
}

void fill_text(uint8_t *buf, int len) {
    const char *pat = "hello world ";
    int plen = 12;
    for (int i = 0; i < len; i++)
        buf[i] = (uint8_t)pat[i % plen];
}

int main(void) {
    printf("=== FastLZ C Reference ===\n");

    int test_len = 4096;
    uint8_t *src_buf = (uint8_t *)malloc(test_len);
    fill_text(src_buf, test_len);
    int comp_max = test_len + test_len / 4 + 64;
    uint8_t *comp_buf = (uint8_t *)malloc(comp_max);
    uint8_t *decomp_buf = (uint8_t *)malloc(test_len + 64);

    int comp_size = fastlz_compress(src_buf, test_len, comp_buf, comp_max);
    printf("Original: %d, Compressed: %d (%d%%)\n",
           test_len, comp_size, comp_size * 100 / test_len);

    int decomp_size = fastlz_decompress(comp_buf, comp_size, decomp_buf, test_len + 64);
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
        int csz = fastlz_compress(src_buf, test_len, comp_buf, comp_max);
        int dsz = fastlz_decompress(comp_buf, csz, decomp_buf, test_len + 64);
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

    printf("=== FastLZ C complete ===\n");
    return 0;
}
