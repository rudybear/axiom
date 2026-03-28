// xxHash32 -- C reference implementation for comparison
// Compile: gcc -O3 -o xxhash32_c xxhash32.c

#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <time.h>

#define PRIME1 0x9E3779B1U
#define PRIME2 0x85EBCA77U
#define PRIME3 0xC2B2AE3DU
#define PRIME4 0x27D4EB2FU
#define PRIME5 0x165667B1U

static inline uint32_t rotl32(uint32_t x, int r) {
    return (x << r) | (x >> (32 - r));
}

static inline uint32_t read32_le(const uint8_t* p) {
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) |
           ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

static inline uint32_t xxh32_round(uint32_t acc, uint32_t val) {
    acc += val * PRIME2;
    acc = rotl32(acc, 13);
    acc *= PRIME1;
    return acc;
}

static inline uint32_t xxh32_avalanche(uint32_t h) {
    h ^= h >> 15;
    h *= PRIME2;
    h ^= h >> 13;
    h *= PRIME3;
    h ^= h >> 16;
    return h;
}

uint32_t xxhash32(const uint8_t* data, int len, uint32_t seed) {
    uint32_t h;
    int offset = 0;

    if (len >= 16) {
        uint32_t v1 = seed + PRIME1 + PRIME2;
        uint32_t v2 = seed + PRIME2;
        uint32_t v3 = seed;
        uint32_t v4 = seed - PRIME1;

        int limit = len - 16;
        while (offset <= limit) {
            v1 = xxh32_round(v1, read32_le(data + offset));
            v2 = xxh32_round(v2, read32_le(data + offset + 4));
            v3 = xxh32_round(v3, read32_le(data + offset + 8));
            v4 = xxh32_round(v4, read32_le(data + offset + 12));
            offset += 16;
        }

        h = rotl32(v1, 1) + rotl32(v2, 7) + rotl32(v3, 12) + rotl32(v4, 18);
    } else {
        h = seed + PRIME5;
    }

    h += (uint32_t)len;

    int tail_limit = len - 4;
    while (offset <= tail_limit) {
        uint32_t k = read32_le(data + offset);
        h += k * PRIME3;
        h = rotl32(h, 17) * PRIME4;
        offset += 4;
    }

    while (offset < len) {
        h += (uint32_t)data[offset] * PRIME5;
        h = rotl32(h, 11) * PRIME1;
        offset++;
    }

    h = xxh32_avalanche(h);
    return h;
}

int main(void) {
    printf("=== xxHash32 C Reference ===\n");

    // Test: 16 zeros, seed=0
    uint8_t zeros[16] = {0};
    uint32_t h_zeros = xxhash32(zeros, 16, 0);
    printf("xxHash32(16 zeros, seed=0) = %u\n", h_zeros);

    // Test: "abcd", seed=0
    uint8_t abcd[4] = {97, 98, 99, 100};
    uint32_t h_abcd = xxhash32(abcd, 4, 0);
    printf("xxHash32(\"abcd\", seed=0) = %u\n", h_abcd);

    // Test: empty, seed=0
    uint32_t h_empty = xxhash32(NULL, 0, 0);
    printf("xxHash32(\"\", seed=0) = %u (expected 46947589)\n", h_empty);

    // Benchmark: 256 bytes x 10M
    uint8_t bench_data[256];
    for (int i = 0; i < 256; i++) {
        bench_data[i] = (uint8_t)((uint32_t)i * 0x9E3779B1U);
    }

    int iterations = 10000000;
    printf("Benchmarking: 10M hashes of 256 bytes...\n");

    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);

    uint32_t checksum = 0;
    for (int i = 0; i < iterations; i++) {
        uint32_t h = xxhash32(bench_data, 256, (uint32_t)i);
        checksum += h;
    }

    clock_gettime(CLOCK_MONOTONIC, &t1);
    long elapsed_ms = (t1.tv_sec - t0.tv_sec) * 1000 +
                      (t1.tv_nsec - t0.tv_nsec) / 1000000;

    printf("Elapsed: %ld ms\n", elapsed_ms);
    printf("Checksum: %u\n", checksum);

    if (elapsed_ms > 0) {
        long total_bytes = (long)iterations * 256;
        long throughput = (total_bytes * 1000) / (elapsed_ms * 1048576);
        printf("Throughput: %ld MB/s\n", throughput);
    }

    printf("=== xxHash32 C complete ===\n");
    return 0;
}
