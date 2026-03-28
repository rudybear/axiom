/*
 * BLAKE3 -- C reference implementation (portable, no SIMD)
 * Compile: gcc -O3 -march=native -ffast-math -o blake3_c blake3.c
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>

#define BLOCK_LEN 64
#define CHUNK_START 1
#define CHUNK_END   2
#define ROOT        8

static const uint32_t IV[8] = {
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
    0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19
};

/* Message word permutation per round */
static const uint8_t MSG_SCHEDULE[7][16] = {
    { 0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14, 15},
    { 2,  6,  3, 10,  7,  0,  4, 13,  1, 11, 12,  5,  9, 14, 15,  8},
    { 3,  4, 10, 12, 13,  2,  7, 14,  6,  5,  9,  0, 11, 15,  8,  1},
    {10,  7, 12,  9, 14,  3, 13, 15,  4,  0, 11,  2,  5,  8,  1,  6},
    {12, 13,  9, 11, 15, 10, 14,  8,  7,  2,  5,  3,  0,  1,  6,  4},
    { 9, 14, 11,  5,  8, 12, 15,  1, 13,  3,  0, 10,  2,  6,  4,  7},
    {11, 15,  5,  0,  1,  9,  8,  6, 14, 10,  2, 12,  3,  4,  7, 13}
};

static inline uint32_t rotr32(uint32_t x, int n) {
    return (x >> n) | (x << (32 - n));
}

static inline uint32_t read32_le(const uint8_t *p) {
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) |
           ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

static inline void write32_le(uint8_t *p, uint32_t v) {
    p[0] = v & 0xFF; p[1] = (v >> 8) & 0xFF;
    p[2] = (v >> 16) & 0xFF; p[3] = (v >> 24) & 0xFF;
}

static inline void g(uint32_t state[16], int a, int b, int c, int d,
                     uint32_t mx, uint32_t my) {
    state[a] = state[a] + state[b] + mx;
    state[d] = rotr32(state[d] ^ state[a], 16);
    state[c] = state[c] + state[d];
    state[b] = rotr32(state[b] ^ state[c], 12);

    state[a] = state[a] + state[b] + my;
    state[d] = rotr32(state[d] ^ state[a], 8);
    state[c] = state[c] + state[d];
    state[b] = rotr32(state[b] ^ state[c], 7);
}

static void round_fn(uint32_t state[16], const uint32_t msg[16], int round) {
    const uint8_t *s = MSG_SCHEDULE[round];
    g(state, 0, 4,  8, 12, msg[s[0]],  msg[s[1]]);
    g(state, 1, 5,  9, 13, msg[s[2]],  msg[s[3]]);
    g(state, 2, 6, 10, 14, msg[s[4]],  msg[s[5]]);
    g(state, 3, 7, 11, 15, msg[s[6]],  msg[s[7]]);
    g(state, 0, 5, 10, 15, msg[s[8]],  msg[s[9]]);
    g(state, 1, 6, 11, 12, msg[s[10]], msg[s[11]]);
    g(state, 2, 7,  8, 13, msg[s[12]], msg[s[13]]);
    g(state, 3, 4,  9, 14, msg[s[14]], msg[s[15]]);
}

static void blake3_compress(const uint32_t cv[8], const uint8_t block[64],
                            uint32_t counter_lo, uint32_t counter_hi,
                            uint32_t block_len, uint32_t flags,
                            uint32_t out[8]) {
    uint32_t msg[16];
    for (int i = 0; i < 16; i++)
        msg[i] = read32_le(block + i * 4);

    uint32_t state[16] = {
        cv[0], cv[1], cv[2], cv[3], cv[4], cv[5], cv[6], cv[7],
        IV[0], IV[1], IV[2], IV[3],
        counter_lo, counter_hi, block_len, flags
    };

    for (int r = 0; r < 7; r++)
        round_fn(state, msg, r);

    for (int i = 0; i < 8; i++)
        out[i] = state[i] ^ state[i + 8];
}

static void blake3_hash(const uint8_t *input, int input_len, uint8_t *output) {
    uint32_t cv[8];
    memcpy(cv, IV, sizeof(IV));

    uint8_t block[64];
    int offset = 0;
    int blocks_processed = 0;
    uint32_t compress_out[8];

    while (offset + 64 <= input_len) {
        memcpy(block, input + offset, 64);
        uint32_t flags = 0;
        if (blocks_processed == 0) flags = CHUNK_START;
        if (offset + 64 >= input_len) flags |= CHUNK_END | ROOT;
        blake3_compress(cv, block, blocks_processed, 0, BLOCK_LEN, flags, compress_out);
        memcpy(cv, compress_out, sizeof(compress_out));
        offset += 64;
        blocks_processed++;
    }

    int remain = input_len - offset;
    if (remain > 0) {
        memset(block, 0, 64);
        memcpy(block, input + offset, remain);
        uint32_t flags = CHUNK_END | ROOT;
        if (blocks_processed == 0) flags |= CHUNK_START;
        blake3_compress(cv, block, blocks_processed, 0, remain, flags, compress_out);
        memcpy(cv, compress_out, sizeof(compress_out));
    }

    if (input_len == 0) {
        memset(block, 0, 64);
        uint32_t flags = CHUNK_START | CHUNK_END | ROOT;
        blake3_compress(cv, block, 0, 0, 0, flags, compress_out);
        memcpy(cv, compress_out, sizeof(compress_out));
    }

    for (int i = 0; i < 8; i++)
        write32_le(output + i * 4, cv[i]);
}

int main(void) {
    printf("=== BLAKE3 Hash (C Reference) ===\n");

    uint8_t hash_out[32];

    /* Test: empty input */
    blake3_hash(NULL, 0, hash_out);
    printf("BLAKE3(\"\") first 4 bytes: %u %u %u %u\n",
           hash_out[0], hash_out[1], hash_out[2], hash_out[3]);
    /* Expected: 0xAF=175, 0x13=19, 0x49=73, 0xB9=185 */
    if (hash_out[0] == 0xAF && hash_out[1] == 0x13 &&
        hash_out[2] == 0x49 && hash_out[3] == 0xB9) {
        printf("PASS: BLAKE3 empty hash first 4 bytes match reference\n");
    } else {
        printf("FAIL: BLAKE3 empty hash mismatch\n");
    }

    /* Test: single zero byte */
    uint8_t one_byte[1] = {0};
    blake3_hash(one_byte, 1, hash_out);
    printf("BLAKE3(0x00) first bytes: %u %u %u %u\n",
           hash_out[0], hash_out[1], hash_out[2], hash_out[3]);

    /* Benchmark: 50 x 1MB */
    int bench_size = 1048576;
    int bench_iters = 50;
    uint8_t *bench_data = (uint8_t *)malloc(bench_size);
    for (int i = 0; i < bench_size; i++) bench_data[i] = i & 0xFF;

    printf("Benchmarking: 50 x 1MB BLAKE3 hash...\n");

    struct timespec ts0, ts1;
    clock_gettime(CLOCK_MONOTONIC, &ts0);
    volatile int checksum = 0;

    for (int iter = 0; iter < bench_iters; iter++) {
        blake3_hash(bench_data, bench_size, hash_out);
        checksum += hash_out[0];
    }

    clock_gettime(CLOCK_MONOTONIC, &ts1);
    long elapsed_ms = (ts1.tv_sec - ts0.tv_sec) * 1000 + (ts1.tv_nsec - ts0.tv_nsec) / 1000000;

    printf("Elapsed: %ld ms\n", elapsed_ms);
    long total_bytes = (long)bench_size * bench_iters;
    if (elapsed_ms > 0)
        printf("Throughput: %ld MB/s\n", (total_bytes * 1000) / (elapsed_ms * 1048576));
    printf("Checksum (prevent DCE): %d\n", checksum);

    free(bench_data);
    printf("=== BLAKE3 complete ===\n");
    return 0;
}
