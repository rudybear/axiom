// LZAV-style compression core -- C reference implementation
// Matches the AXIOM port's algorithm for comparison

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>

#define HASH_TABLE_SIZE 16384
#define HASH_LOG 14
#define HASH_MASK 16383
#define MIN_MATCH 4
#define MAX_TOKEN_LEN 63

// Token types (upper 2 bits)
#define TYPE_LITERAL     0
#define TYPE_SHORT_MATCH 1
#define TYPE_LONG_MATCH  2
#define TYPE_REPEAT_MATCH 3

static inline uint32_t read32_le(const uint8_t *p) {
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) |
           ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

static inline int lzav_hash(uint32_t val) {
    return (int)((val * 2654435761U) >> 18) & HASH_MASK;
}

static inline void write_token(uint8_t *dst, int pos, int type, int length) {
    dst[pos] = (uint8_t)((type << 6) | (length & MAX_TOKEN_LEN));
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

static int match_length(const uint8_t *src, int pos1, int pos2, int max_len) {
    int len = 0;
    while (len < max_len && src[pos1 + len] == src[pos2 + len])
        len++;
    return len;
}

int lzav_compress(const uint8_t *src, int src_len,
                  uint8_t *dst, int dst_max) {
    if (src_len < 1) return 0;

    int hash_table[HASH_TABLE_SIZE];
    memset(hash_table, 0xFF, sizeof(hash_table)); // -1

    int src_pos = 0, dst_pos = 0, anchor = 0;
    int src_end = src_len;
    int match_limit = src_len - MIN_MATCH;
    int last_offset = 0;

    if (src_len < MIN_MATCH) {
        int lit_len = src_len;
        int tok_len = lit_len > MAX_TOKEN_LEN ? MAX_TOKEN_LEN : lit_len;
        write_token(dst, dst_pos, TYPE_LITERAL, tok_len);
        dst_pos++;
        if (lit_len > MAX_TOKEN_LEN)
            dst_pos = write_extended_length(dst, dst_pos, lit_len - MAX_TOKEN_LEN);
        memcpy(dst + dst_pos, src, lit_len);
        dst_pos += lit_len;
        return dst_pos;
    }

    while (src_pos <= match_limit) {
        uint32_t cur_val = read32_le(src + src_pos);
        int h = lzav_hash(cur_val);
        int ref_pos = hash_table[h];
        hash_table[h] = src_pos;

        int best_len = 0, best_offset = 0, best_type = TYPE_LITERAL;

        // Try repeat match
        if (last_offset > 0 && src_pos >= last_offset) {
            int rep_ref = src_pos - last_offset;
            if (rep_ref >= 0) {
                int max_ext = src_end - src_pos;
                int rep_len = match_length(src, rep_ref, src_pos, max_ext);
                if (rep_len >= MIN_MATCH) {
                    best_len = rep_len;
                    best_offset = last_offset;
                    best_type = TYPE_REPEAT_MATCH;
                }
            }
        }

        // Try hash match
        if (ref_pos >= 0) {
            int offset = src_pos - ref_pos;
            if (offset > 0 && offset < 65536) {
                uint32_t ref_val = read32_le(src + ref_pos);
                if (cur_val == ref_val) {
                    int max_ext = src_end - src_pos;
                    int m_len = match_length(src, ref_pos, src_pos, max_ext);
                    if (m_len >= MIN_MATCH && m_len > best_len) {
                        best_len = m_len;
                        best_offset = offset;
                        best_type = offset < 256 ? TYPE_SHORT_MATCH : TYPE_LONG_MATCH;
                    }
                }
            }
        }

        if (best_len < MIN_MATCH) {
            src_pos++;
            continue;
        }

        // Emit literals
        int lit_len = src_pos - anchor;
        if (lit_len > 0) {
            int tok_len = lit_len > MAX_TOKEN_LEN ? MAX_TOKEN_LEN : lit_len;
            if (dst_pos + 1 + lit_len + 3 + 4 > dst_max) return 0;
            write_token(dst, dst_pos, TYPE_LITERAL, tok_len);
            dst_pos++;
            if (lit_len > MAX_TOKEN_LEN)
                dst_pos = write_extended_length(dst, dst_pos, lit_len - MAX_TOKEN_LEN);
            memcpy(dst + dst_pos, src + anchor, lit_len);
            dst_pos += lit_len;
        }

        // Emit match
        int ml_code = best_len - MIN_MATCH;
        int tok_ml = ml_code > MAX_TOKEN_LEN ? MAX_TOKEN_LEN : ml_code;
        if (dst_pos + 4 > dst_max) return 0;
        write_token(dst, dst_pos, best_type, tok_ml);
        dst_pos++;
        if (ml_code > MAX_TOKEN_LEN)
            dst_pos = write_extended_length(dst, dst_pos, ml_code - MAX_TOKEN_LEN);

        if (best_type == TYPE_SHORT_MATCH) {
            dst[dst_pos++] = (uint8_t)(best_offset & 0xFF);
        } else if (best_type == TYPE_LONG_MATCH) {
            dst[dst_pos++] = (uint8_t)(best_offset & 0xFF);
            dst[dst_pos++] = (uint8_t)((best_offset >> 8) & 0xFF);
        }

        last_offset = best_offset;
        src_pos += best_len;
        anchor = src_pos;

        if (src_pos <= match_limit) {
            uint32_t skip_val = read32_le(src + src_pos);
            int skip_h = lzav_hash(skip_val);
            hash_table[skip_h] = src_pos;
        }
    }

    // Final literals
    int last_lit_len = src_end - anchor;
    if (last_lit_len > 0) {
        int tok_len2 = last_lit_len > MAX_TOKEN_LEN ? MAX_TOKEN_LEN : last_lit_len;
        if (dst_pos + 1 + last_lit_len + 4 > dst_max) return 0;
        write_token(dst, dst_pos, TYPE_LITERAL, tok_len2);
        dst_pos++;
        if (last_lit_len > MAX_TOKEN_LEN)
            dst_pos = write_extended_length(dst, dst_pos, last_lit_len - MAX_TOKEN_LEN);
        memcpy(dst + dst_pos, src + anchor, last_lit_len);
        dst_pos += last_lit_len;
    }

    return dst_pos;
}

int lzav_decompress(const uint8_t *src, int src_len,
                    uint8_t *dst, int dst_max) {
    int src_pos = 0, dst_pos = 0, last_offset = 0;

    while (src_pos < src_len) {
        int token = src[src_pos++];
        int tok_type = (token >> 6) & 3;
        int tok_len = token & MAX_TOKEN_LEN;

        if (tok_type == TYPE_LITERAL) {
            int lit_len = tok_len;
            if (tok_len == MAX_TOKEN_LEN) {
                int extra = 255;
                while (extra == 255) {
                    if (src_pos >= src_len) return 0;
                    extra = src[src_pos++];
                    lit_len += extra;
                }
            }
            if (dst_pos + lit_len > dst_max) return 0;
            memcpy(dst + dst_pos, src + src_pos, lit_len);
            src_pos += lit_len;
            dst_pos += lit_len;

        } else if (tok_type == TYPE_SHORT_MATCH) {
            int ml_code = tok_len;
            if (tok_len == MAX_TOKEN_LEN) {
                int extra = 255;
                while (extra == 255) {
                    if (src_pos >= src_len) return 0;
                    extra = src[src_pos++];
                    ml_code += extra;
                }
            }
            if (src_pos >= src_len) return 0;
            int offset = src[src_pos++];
            int m_len = ml_code + MIN_MATCH;
            int match_src = dst_pos - offset;
            if (match_src < 0 || dst_pos + m_len > dst_max) return 0;
            for (int i = 0; i < m_len; i++)
                dst[dst_pos + i] = dst[match_src + i];
            dst_pos += m_len;
            last_offset = offset;

        } else if (tok_type == TYPE_LONG_MATCH) {
            int ml_code = tok_len;
            if (tok_len == MAX_TOKEN_LEN) {
                int extra = 255;
                while (extra == 255) {
                    if (src_pos >= src_len) return 0;
                    extra = src[src_pos++];
                    ml_code += extra;
                }
            }
            if (src_pos + 2 > src_len) return 0;
            int offset = src[src_pos] | (src[src_pos + 1] << 8);
            src_pos += 2;
            int m_len = ml_code + MIN_MATCH;
            int match_src = dst_pos - offset;
            if (match_src < 0 || dst_pos + m_len > dst_max) return 0;
            for (int i = 0; i < m_len; i++)
                dst[dst_pos + i] = dst[match_src + i];
            dst_pos += m_len;
            last_offset = offset;

        } else { // TYPE_REPEAT_MATCH
            int ml_code = tok_len;
            if (tok_len == MAX_TOKEN_LEN) {
                int extra = 255;
                while (extra == 255) {
                    if (src_pos >= src_len) return 0;
                    extra = src[src_pos++];
                    ml_code += extra;
                }
            }
            if (last_offset == 0) return 0;
            int m_len = ml_code + MIN_MATCH;
            int match_src = dst_pos - last_offset;
            if (match_src < 0 || dst_pos + m_len > dst_max) return 0;
            for (int i = 0; i < m_len; i++)
                dst[dst_pos + i] = dst[match_src + i];
            dst_pos += m_len;
        }
    }

    return dst_pos;
}

static int buffers_equal(const uint8_t *a, const uint8_t *b, int len) {
    for (int i = 0; i < len; i++)
        if (a[i] != b[i]) return 0;
    return 1;
}

static void fill_repeating(uint8_t *buf, int len) {
    for (int i = 0; i < len; i++)
        buf[i] = (i & 7) + 65;
}

static void fill_mixed(uint8_t *buf, int len) {
    for (int i = 0; i < len; i++) {
        int section = i / 64;
        if (section % 2 == 0)
            buf[i] = (i % 8 + 65) & 0xFF;
        else
            buf[i] = (i * 37 + 13) & 0xFF;
    }
}

static void fill_text_like(uint8_t *buf, int len) {
    const char *pat = "the quick brown fox jumps over the lazy dog ";
    int plen = 44;
    for (int i = 0; i < len; i++)
        buf[i] = pat[i % plen];
}

static void fill_run_data(uint8_t *buf, int len) {
    for (int i = 0; i < len; i++) {
        int block = i / 128;
        int phase = block & 3;
        if (phase == 0) buf[i] = (i & 7) + 65;
        else if (phase == 1) buf[i] = (i & 3) + 48;
        else if (phase == 2) buf[i] = (i & 7) + 65;
        else buf[i] = (i & 15) + 97;
    }
}

static int run_test(const char *name, uint8_t *src_buf, int test_len) {
    int comp_max = test_len + test_len / 4 + 64;
    uint8_t *comp_buf = (uint8_t *)malloc(comp_max);
    uint8_t *decomp_buf = (uint8_t *)malloc(test_len + 64);

    printf("%s", name);
    int comp_size = lzav_compress(src_buf, test_len, comp_buf, comp_max);
    printf("Original: %d bytes, Compressed: %d bytes", test_len, comp_size);

    int result = 0;
    if (comp_size > 0) {
        int ratio = (comp_size * 100) / test_len;
        printf(", Ratio: %d%%", ratio);
        int decomp_size = lzav_decompress(comp_buf, comp_size, decomp_buf, test_len + 64);
        if (decomp_size == test_len && buffers_equal(src_buf, decomp_buf, test_len)) {
            printf(" -- PASS\n");
            result = 1;
        } else {
            printf(" -- FAIL (decompressed %d bytes)\n", decomp_size);
        }
    } else {
        printf(" -- FAIL (compression returned 0)\n");
    }

    free(comp_buf);
    free(decomp_buf);
    return result;
}

#ifdef _WIN32
#include <windows.h>
static int64_t clock_ns(void) {
    LARGE_INTEGER freq, count;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&count);
    return (int64_t)((double)count.QuadPart / freq.QuadPart * 1e9);
}
#else
static int64_t clock_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000LL + ts.tv_nsec;
}
#endif

int main(void) {
    printf("=== LZAV Compression (C reference) ===\n");
    printf("Token: 2-bit type (lit/short/long/repeat) + 6-bit length\n");
    printf("Hash table: 16K entries (vs LZ4's 4K)\n\n");

    int pass_count = 0;
    int total_tests = 4;

    uint8_t *src1 = (uint8_t *)malloc(1024);
    fill_repeating(src1, 1024);
    pass_count += run_test("--- Test 1: Repeating pattern (1024B) ---\n", src1, 1024);
    free(src1);

    uint8_t *src2 = (uint8_t *)malloc(2048);
    fill_mixed(src2, 2048);
    pass_count += run_test("--- Test 2: Mixed data (2048B) ---\n", src2, 2048);
    free(src2);

    uint8_t *src3 = (uint8_t *)malloc(4096);
    fill_text_like(src3, 4096);
    pass_count += run_test("--- Test 3: Text-like data (4096B) ---\n", src3, 4096);
    free(src3);

    uint8_t *src4 = (uint8_t *)malloc(4096);
    fill_run_data(src4, 4096);
    pass_count += run_test("--- Test 4: Run data (4096B) ---\n", src4, 4096);
    free(src4);

    printf("\nTests passed: %d/%d\n", pass_count, total_tests);

    // Benchmark
    int bench_len = 4096;
    uint8_t *bench_src = (uint8_t *)malloc(bench_len);
    fill_text_like(bench_src, bench_len);
    int bench_comp_max = bench_len + bench_len / 4 + 64;
    uint8_t *bench_comp = (uint8_t *)malloc(bench_comp_max);
    uint8_t *bench_decomp = (uint8_t *)malloc(bench_len + 64);

    int iterations = 50000;
    printf("\n--- Benchmark: 4KB text x 50K compress+decompress ---\n");

    int64_t t0 = clock_ns();
    int checksum = 0;
    for (int iter = 0; iter < iterations; iter++) {
        int csz = lzav_compress(bench_src, bench_len, bench_comp, bench_comp_max);
        int dsz = lzav_decompress(bench_comp, csz, bench_decomp, bench_len + 64);
        checksum += csz + dsz;
    }
    int64_t t1 = clock_ns();
    int64_t elapsed_ms = (t1 - t0) / 1000000;

    printf("Elapsed: %lld ms\n", (long long)elapsed_ms);
    printf("Checksum (prevent DCE): %d\n", checksum);
    if (elapsed_ms > 0) {
        int64_t total_mb = ((int64_t)iterations * bench_len * 2) / 1048576;
        int64_t throughput = (total_mb * 1000) / elapsed_ms;
        printf("Throughput: %lld MB/s\n", (long long)throughput);
    }

    free(bench_src);
    free(bench_comp);
    free(bench_decomp);

    printf("\n=== LZAV complete ===\n");
    return 0;
}
