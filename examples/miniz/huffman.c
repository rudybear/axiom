/*
 * Huffman Codec -- C reference implementation
 * Core building block of deflate/inflate (miniz).
 *
 * Algorithm:
 *   Encode: frequency count -> Huffman tree -> canonical codes -> bit packing
 *   Decode: read header -> rebuild canonical codes -> lookup table -> decode bits
 *
 * Build:
 *   gcc -O2 -o huffman_c huffman.c
 *   clang -O2 -o huffman_c huffman.c
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>

#ifdef _WIN32
#include <windows.h>
static uint64_t clock_ns_impl(void) {
    LARGE_INTEGER freq, cnt;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&cnt);
    return (uint64_t)((double)cnt.QuadPart / freq.QuadPart * 1e9);
}
#else
static uint64_t clock_ns_impl(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + ts.tv_nsec;
}
#endif

/* ========================================================================= */
/* Constants                                                                  */
/* ========================================================================= */

#define NUM_SYMBOLS   256
#define MAX_CODE_LEN  15
#define MAX_NODES     511  /* 256 leaves + 255 internal */

/* ========================================================================= */
/* Huffman tree node                                                         */
/* ========================================================================= */

typedef struct {
    int freq;
    int symbol;  /* -1 for internal nodes */
    int left;    /* -1 for leaves */
    int right;   /* -1 for leaves */
} HuffNode;

/* ========================================================================= */
/* Bit writer                                                                */
/* ========================================================================= */

typedef struct {
    uint8_t *buf;
    int byte_pos;
    int bit_pos;
    uint8_t cur_byte;
} BitWriter;

static void bw_init(BitWriter *bw, uint8_t *buf) {
    bw->buf = buf;
    bw->byte_pos = 0;
    bw->bit_pos = 0;
    bw->cur_byte = 0;
}

static inline void bw_write_bits(BitWriter *bw, int value, int count) {
    for (int i = 0; i < count; i++) {
        int bit = (value >> (count - 1 - i)) & 1;
        bw->cur_byte |= (bit << (7 - bw->bit_pos));
        bw->bit_pos++;
        if (bw->bit_pos == 8) {
            bw->buf[bw->byte_pos++] = bw->cur_byte;
            bw->bit_pos = 0;
            bw->cur_byte = 0;
        }
    }
}

static void bw_flush(BitWriter *bw) {
    if (bw->bit_pos > 0) {
        bw->buf[bw->byte_pos++] = bw->cur_byte;
        bw->bit_pos = 0;
        bw->cur_byte = 0;
    }
}

/* ========================================================================= */
/* Bit reader                                                                */
/* ========================================================================= */

typedef struct {
    const uint8_t *buf;
    int byte_pos;
    int bit_pos;
    uint8_t cur_byte;
} BitReader;

static void br_init(BitReader *br, const uint8_t *buf, int start_pos) {
    br->buf = buf;
    br->byte_pos = start_pos;
    br->bit_pos = 0;
    br->cur_byte = buf[start_pos];
}

static inline int br_read_bit(BitReader *br) {
    int bit = (br->cur_byte >> (7 - br->bit_pos)) & 1;
    br->bit_pos++;
    if (br->bit_pos == 8) {
        br->byte_pos++;
        br->cur_byte = br->buf[br->byte_pos];
        br->bit_pos = 0;
    }
    return bit;
}

static inline int br_read_bits(BitReader *br, int count) {
    int result = 0;
    for (int i = 0; i < count; i++) {
        result = (result << 1) | br_read_bit(br);
    }
    return result;
}

/* ========================================================================= */
/* Count byte frequencies                                                    */
/* ========================================================================= */

static void count_frequencies(const uint8_t *input, int in_len, int *freq) {
    memset(freq, 0, NUM_SYMBOLS * sizeof(int));
    for (int i = 0; i < in_len; i++) {
        freq[input[i]]++;
    }
}

/* ========================================================================= */
/* Find minimum-frequency unused node                                        */
/* ========================================================================= */

static int find_min_node(HuffNode *nodes, int *used, int node_count) {
    int min_idx = -1;
    int min_freq = INT32_MAX;
    for (int i = 0; i < node_count; i++) {
        if (!used[i] && nodes[i].freq < min_freq) {
            min_freq = nodes[i].freq;
            min_idx = i;
        }
    }
    return min_idx;
}

/* ========================================================================= */
/* Build Huffman tree                                                        */
/* ========================================================================= */

static int build_tree(int *freq, HuffNode *nodes, int *out_node_count) {
    int node_count = 0;

    /* Create leaf nodes */
    for (int sym = 0; sym < NUM_SYMBOLS; sym++) {
        if (freq[sym] > 0) {
            nodes[node_count].freq = freq[sym];
            nodes[node_count].symbol = sym;
            nodes[node_count].left = -1;
            nodes[node_count].right = -1;
            node_count++;
        }
    }

    /* Single symbol edge case */
    if (node_count == 1) {
        *out_node_count = 1;
        return 0;
    }

    int *used = (int *)calloc(MAX_NODES, sizeof(int));
    int remaining = node_count;

    while (remaining > 1) {
        int a = find_min_node(nodes, used, node_count);
        used[a] = 1;
        int b = find_min_node(nodes, used, node_count);
        used[b] = 1;

        nodes[node_count].freq = nodes[a].freq + nodes[b].freq;
        nodes[node_count].symbol = -1;
        nodes[node_count].left = a;
        nodes[node_count].right = b;
        node_count++;
        remaining--;
    }

    free(used);
    *out_node_count = node_count;
    return node_count - 1; /* root */
}

/* ========================================================================= */
/* Generate code lengths by tree traversal                                   */
/* ========================================================================= */

static void generate_lengths(HuffNode *nodes, int node_idx, int depth, int *code_lens) {
    if (nodes[node_idx].left == -1 && nodes[node_idx].right == -1) {
        int len = depth;
        if (len > MAX_CODE_LEN) len = MAX_CODE_LEN;
        if (len == 0) len = 1;
        code_lens[nodes[node_idx].symbol] = len;
        return;
    }
    if (nodes[node_idx].left >= 0)
        generate_lengths(nodes, nodes[node_idx].left, depth + 1, code_lens);
    if (nodes[node_idx].right >= 0)
        generate_lengths(nodes, nodes[node_idx].right, depth + 1, code_lens);
}

/* ========================================================================= */
/* Build canonical Huffman codes from bit lengths                            */
/* ========================================================================= */

static void build_canonical_codes(int *code_lens, int *codes) {
    int bl_count[MAX_CODE_LEN + 1];
    memset(bl_count, 0, sizeof(bl_count));

    for (int sym = 0; sym < NUM_SYMBOLS; sym++) {
        if (code_lens[sym] > 0)
            bl_count[code_lens[sym]]++;
    }

    int next_code[MAX_CODE_LEN + 1];
    next_code[0] = 0;
    int code = 0;
    for (int bits = 1; bits <= MAX_CODE_LEN; bits++) {
        code = (code + bl_count[bits - 1]) << 1;
        next_code[bits] = code;
    }

    for (int sym = 0; sym < NUM_SYMBOLS; sym++) {
        if (code_lens[sym] > 0) {
            codes[sym] = next_code[code_lens[sym]]++;
        } else {
            codes[sym] = 0;
        }
    }
}

/* ========================================================================= */
/* Huffman Encode                                                            */
/* ========================================================================= */
/* Output format:                                                            */
/*   [0..1]  = original length (16-bit LE)                                  */
/*   [2..3]  = number of unique symbols (16-bit LE)                         */
/*   For each unique symbol: (symbol_byte, length_byte)                     */
/*   Then: Huffman-coded bits (MSB first, padded)                           */

static int huffman_encode(const uint8_t *input, int in_len,
                          uint8_t *output, int out_max) {
    if (in_len < 1 || in_len > 65535) return 0;

    /* Count frequencies */
    int freq[NUM_SYMBOLS];
    count_frequencies(input, in_len, freq);

    /* Build tree */
    HuffNode nodes[MAX_NODES];
    int node_count;
    int root = build_tree(freq, nodes, &node_count);

    /* Generate code lengths */
    int code_lens[NUM_SYMBOLS];
    memset(code_lens, 0, sizeof(code_lens));
    generate_lengths(nodes, root, 0, code_lens);

    /* Build canonical codes */
    int codes[NUM_SYMBOLS];
    build_canonical_codes(code_lens, codes);

    /* Write header */
    int pos = 0;
    output[pos++] = in_len & 0xFF;
    output[pos++] = (in_len >> 8) & 0xFF;

    int num_unique = 0;
    for (int sym = 0; sym < NUM_SYMBOLS; sym++) {
        if (code_lens[sym] > 0) num_unique++;
    }

    output[pos++] = num_unique & 0xFF;
    output[pos++] = (num_unique >> 8) & 0xFF;

    for (int sym = 0; sym < NUM_SYMBOLS; sym++) {
        if (code_lens[sym] > 0) {
            output[pos++] = (uint8_t)sym;
            output[pos++] = (uint8_t)code_lens[sym];
        }
    }

    /* Encode data */
    BitWriter bw;
    bw_init(&bw, output);
    bw.byte_pos = pos;

    for (int i = 0; i < in_len; i++) {
        bw_write_bits(&bw, codes[input[i]], code_lens[input[i]]);
    }

    bw_flush(&bw);
    return bw.byte_pos;
}

/* ========================================================================= */
/* Huffman Decode                                                            */
/* ========================================================================= */

static int huffman_decode(const uint8_t *input, int in_len,
                          uint8_t *output, int out_max) {
    if (in_len < 4) return 0;

    int orig_len = input[0] | (input[1] << 8);
    if (orig_len > out_max) return 0;

    int num_unique = input[2] | (input[3] << 8);
    if (num_unique < 1 || num_unique > NUM_SYMBOLS) return 0;

    int pos = 4;

    /* Read (symbol, length) pairs */
    int code_lens[NUM_SYMBOLS];
    memset(code_lens, 0, sizeof(code_lens));

    for (int i = 0; i < num_unique; i++) {
        int sym = input[pos++];
        int len = input[pos++];
        code_lens[sym] = len;
    }

    /* Rebuild canonical codes */
    int codes[NUM_SYMBOLS];
    build_canonical_codes(code_lens, codes);

    /* Build decode lookup table */
    int table_bits = MAX_CODE_LEN;
    int table_size = 1 << table_bits;
    int *decode_sym = (int *)malloc(table_size * sizeof(int));
    int *decode_len = (int *)malloc(table_size * sizeof(int));

    for (int i = 0; i < table_size; i++) {
        decode_sym[i] = -1;
        decode_len[i] = 0;
    }

    for (int sym = 0; sym < NUM_SYMBOLS; sym++) {
        int cl = code_lens[sym];
        if (cl > 0) {
            int c = codes[sym];
            int prefix = c << (table_bits - cl);
            int fill_count = 1 << (table_bits - cl);
            for (int j = 0; j < fill_count; j++) {
                decode_sym[prefix + j] = sym;
                decode_len[prefix + j] = cl;
            }
        }
    }

    /* Decode bitstream */
    BitReader br;
    br_init(&br, input, pos);

    int out_pos = 0;
    while (out_pos < orig_len) {
        /* Save state */
        int saved_byte_pos = br.byte_pos;
        int saved_bit_pos = br.bit_pos;
        uint8_t saved_cur_byte = br.cur_byte;

        int peek_val = br_read_bits(&br, table_bits);

        int sym = decode_sym[peek_val];
        int cl = decode_len[peek_val];

        if (sym < 0 || cl == 0) {
            free(decode_sym);
            free(decode_len);
            return 0;
        }

        /* Restore and consume exact bits */
        br.byte_pos = saved_byte_pos;
        br.bit_pos = saved_bit_pos;
        br.cur_byte = saved_cur_byte;
        br_read_bits(&br, cl);

        output[out_pos++] = (uint8_t)sym;
    }

    free(decode_sym);
    free(decode_len);
    return out_pos;
}

/* ========================================================================= */
/* Test data generators                                                      */
/* ========================================================================= */

static void fill_text_like(uint8_t *buf, int len) {
    const char *pat = "the quick brown fox jumps over the lazy dog ";
    int pat_len = 44;
    for (int i = 0; i < len; i++) {
        buf[i] = (uint8_t)pat[i % pat_len];
    }
}

static void fill_skewed(uint8_t *buf, int len) {
    int seed = 12345;
    for (int i = 0; i < len; i++) {
        seed = (seed * 1103515245 + 12345) & 0x7FFFFFFF;
        int r = (seed >> 16) & 255;
        uint8_t ch;
        if      (r < 46)  ch = 32;          /* space: ~18% */
        else if (r < 79)  ch = 101;         /* 'e': ~13% */
        else if (r < 102) ch = 116;         /* 't': ~9% */
        else if (r < 122) ch = 97;          /* 'a': ~8% */
        else if (r < 140) ch = 111;         /* 'o': ~7% */
        else if (r < 155) ch = 105;         /* 'i': ~6% */
        else if (r < 168) ch = 110;         /* 'n': ~5% */
        else if (r < 180) ch = 115;         /* 's': ~5% */
        else if (r < 190) ch = 114;         /* 'r': ~4% */
        else if (r < 199) ch = 104;         /* 'h': ~4% */
        else if (r < 210) ch = 108;         /* 'l': ~4% */
        else if (r < 225) ch = (r & 31) + 97;
        else               ch = r & 127;
        buf[i] = ch;
    }
}

static void fill_uniform(uint8_t *buf, int len) {
    int seed = 98765;
    for (int i = 0; i < len; i++) {
        seed = (seed * 1103515245 + 12345) & 0x7FFFFFFF;
        buf[i] = (uint8_t)((seed >> 16) & 255);
    }
}

/* ========================================================================= */
/* Run a round-trip test                                                     */
/* ========================================================================= */

static int run_test(const char *name, uint8_t *data, int data_len) {
    int comp_max = data_len + data_len / 2 + 1024;
    uint8_t *comp_buf = (uint8_t *)malloc(comp_max);
    uint8_t *decomp_buf = (uint8_t *)malloc(data_len + 64);

    int comp_size = huffman_encode(data, data_len, comp_buf, comp_max);
    printf("Original: %d bytes\n", data_len);
    printf("Compressed: %d bytes\n", comp_size);

    int result = 0;

    if (comp_size > 0) {
        int ratio = (comp_size * 100) / data_len;
        printf("Ratio: %d%%\n", ratio);

        int decomp_size = huffman_decode(comp_buf, comp_size, decomp_buf, data_len + 64);
        printf("Decompressed: %d bytes\n", decomp_size);

        if (decomp_size == data_len && memcmp(data, decomp_buf, data_len) == 0) {
            printf("PASS: Round-trip verified\n");
            result = 1;
        } else {
            printf("FAIL: Round-trip mismatch\n");
            if (decomp_size != data_len) {
                printf("  Expected length: %d, got: %d\n", data_len, decomp_size);
            }
        }
    } else {
        printf("FAIL: Compression returned 0\n");
    }

    free(comp_buf);
    free(decomp_buf);
    return result;
}

/* ========================================================================= */
/* Main                                                                      */
/* ========================================================================= */

int main(void) {
    printf("=== Huffman Codec (miniz core) -- C Reference ===\n");

    int pass_count = 0;
    int test_count = 0;

    /* Test 1: Text-like data */
    printf("\n--- Test 1: Text-like data (1024 bytes) ---\n");
    int test1_len = 1024;
    uint8_t *test1 = (uint8_t *)malloc(test1_len);
    fill_text_like(test1, test1_len);
    test_count++;
    pass_count += run_test("text-like", test1, test1_len);
    free(test1);

    /* Test 2: Skewed distribution */
    printf("\n--- Test 2: Skewed distribution (4096 bytes) ---\n");
    int test2_len = 4096;
    uint8_t *test2 = (uint8_t *)malloc(test2_len);
    fill_skewed(test2, test2_len);
    test_count++;
    pass_count += run_test("skewed", test2, test2_len);
    free(test2);

    /* Test 3: Uniform random */
    printf("\n--- Test 3: Uniform random (2048 bytes) ---\n");
    int test3_len = 2048;
    uint8_t *test3 = (uint8_t *)malloc(test3_len);
    fill_uniform(test3, test3_len);
    test_count++;
    pass_count += run_test("uniform", test3, test3_len);
    free(test3);

    /* Test 4: Large text */
    printf("\n--- Test 4: Large text (65536 bytes) ---\n");
    int test4_len = 65535;
    uint8_t *test4 = (uint8_t *)malloc(test4_len);
    fill_skewed(test4, test4_len);
    test_count++;
    pass_count += run_test("large-text", test4, test4_len);
    free(test4);

    /* Test 5: Single byte repeated */
    printf("\n--- Test 5: Single byte repeated (1024 bytes) ---\n");
    int test5_len = 1024;
    uint8_t *test5 = (uint8_t *)malloc(test5_len);
    memset(test5, 'A', test5_len);
    test_count++;
    pass_count += run_test("single-byte", test5, test5_len);
    free(test5);

    /* Summary */
    printf("\n--- Summary ---\n");
    printf("Tests passed: %d/%d\n", pass_count, test_count);

    /* Benchmark */
    printf("\n--- Benchmark: 64KB skewed data x 1000 encode+decode ---\n");

    int bench_len = 65535;
    uint8_t *bench_src = (uint8_t *)malloc(bench_len);
    fill_skewed(bench_src, bench_len);

    int bench_comp_max = bench_len + bench_len / 2 + 1024;
    uint8_t *bench_comp = (uint8_t *)malloc(bench_comp_max);
    uint8_t *bench_decomp = (uint8_t *)malloc(bench_len + 64);

    int iterations = 1000;
    uint64_t t0 = clock_ns_impl();
    int checksum = 0;

    for (int iter = 0; iter < iterations; iter++) {
        int csz = huffman_encode(bench_src, bench_len, bench_comp, bench_comp_max);
        int dsz = huffman_decode(bench_comp, csz, bench_decomp, bench_len + 64);
        checksum += csz + dsz;
    }

    uint64_t t1 = clock_ns_impl();
    uint64_t elapsed_ms = (t1 - t0) / 1000000;

    printf("Elapsed: %llu ms\n", (unsigned long long)elapsed_ms);
    printf("Checksum (prevent DCE): %d\n", checksum);

    if (elapsed_ms > 0) {
        uint64_t total_mb = ((uint64_t)iterations * bench_len * 2) / 1048576;
        uint64_t throughput = (total_mb * 1000) / elapsed_ms;
        printf("Throughput: %llu MB/s\n", (unsigned long long)throughput);
    }

    free(bench_src);
    free(bench_comp);
    free(bench_decomp);

    printf("\n=== Huffman codec complete ===\n");
    return 0;
}
