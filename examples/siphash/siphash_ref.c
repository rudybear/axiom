/*
 * SipHash-2-4 reference implementation in C
 * For comparison with AXIOM port
 *
 * Reference: https://131002.net/siphash/
 * Authors: Jean-Philippe Aumasson, Daniel J. Bernstein
 */

#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <time.h>

#ifdef _WIN32
#include <windows.h>
static uint64_t clock_ns_val(void) {
    LARGE_INTEGER freq, count;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&count);
    return (uint64_t)((double)count.QuadPart / freq.QuadPart * 1e9);
}
#else
static uint64_t clock_ns_val(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + ts.tv_nsec;
}
#endif

#define ROTL(x, b) (((x) << (b)) | ((x) >> (64 - (b))))

#define U8TO64_LE(p)                                                     \
    (((uint64_t)(p)[0]) | ((uint64_t)(p)[1] << 8) |                     \
     ((uint64_t)(p)[2] << 16) | ((uint64_t)(p)[3] << 24) |             \
     ((uint64_t)(p)[4] << 32) | ((uint64_t)(p)[5] << 40) |             \
     ((uint64_t)(p)[6] << 48) | ((uint64_t)(p)[7] << 56))

#define SIPROUND                                                         \
    do {                                                                 \
        v0 += v1; v1 = ROTL(v1, 13); v1 ^= v0; v0 = ROTL(v0, 32);     \
        v2 += v3; v3 = ROTL(v3, 16); v3 ^= v2;                         \
        v0 += v3; v3 = ROTL(v3, 21); v3 ^= v0;                         \
        v2 += v1; v1 = ROTL(v1, 17); v1 ^= v2; v2 = ROTL(v2, 32);     \
    } while (0)

static uint64_t siphash_2_4(const uint8_t *in, size_t inlen,
                              uint64_t k0, uint64_t k1) {
    uint64_t v0 = k0 ^ 0x736f6d6570736575ULL;
    uint64_t v1 = k1 ^ 0x646f72616e646f6dULL;
    uint64_t v2 = k0 ^ 0x6c7967656e657261ULL;
    uint64_t v3 = k1 ^ 0x7465646279746573ULL;

    const uint8_t *end = in + inlen - (inlen % 8);
    const int left = inlen & 7;
    uint64_t b = ((uint64_t)inlen) << 56;

    for (; in != end; in += 8) {
        uint64_t m = U8TO64_LE(in);
        v3 ^= m;
        SIPROUND;
        SIPROUND;
        v0 ^= m;
    }

    switch (left) {
    case 7: b |= ((uint64_t)in[6]) << 48; /* fall through */
    case 6: b |= ((uint64_t)in[5]) << 40; /* fall through */
    case 5: b |= ((uint64_t)in[4]) << 32; /* fall through */
    case 4: b |= ((uint64_t)in[3]) << 24; /* fall through */
    case 3: b |= ((uint64_t)in[2]) << 16; /* fall through */
    case 2: b |= ((uint64_t)in[1]) << 8;  /* fall through */
    case 1: b |= ((uint64_t)in[0]);        break;
    case 0: break;
    }

    v3 ^= b;
    SIPROUND;
    SIPROUND;
    v0 ^= b;

    v2 ^= 0xff;
    SIPROUND;
    SIPROUND;
    SIPROUND;
    SIPROUND;

    return v0 ^ v1 ^ v2 ^ v3;
}

int main(void) {
    /* Key: 00 01 02 ... 0F */
    uint64_t k0 = 0x0706050403020100ULL;
    uint64_t k1 = 0x0F0E0D0C0B0A0908ULL;

    /* Message: 00 01 02 ... 0E (15 bytes) */
    uint8_t msg[15];
    for (int i = 0; i < 15; i++) msg[i] = (uint8_t)i;

    uint64_t hash = siphash_2_4(msg, 15, k0, k1);

    printf("SipHash-2-4 Test (C reference)\n");
    printf("Key: 00 01 02 ... 0F\n");
    printf("Message: 00 01 02 ... 0E (15 bytes)\n");
    printf("Hash result: %lld\n", (long long)hash);

    uint64_t expected = 0xa129ca6149be45e5ULL;
    printf("Expected:    %lld\n", (long long)expected);

    if (hash == expected)
        printf("PASS: Hash matches reference!\n");
    else
        printf("FAIL: Hash does not match reference.\n");

    /* Benchmark: 1,000,000 hashes */
    int iterations = 1000000;
    uint64_t start = clock_ns_val();

    uint64_t checksum = 0;
    for (int i = 0; i < iterations; i++) {
        uint64_t h = siphash_2_4(msg, 15, k0, k1);
        checksum ^= h;
    }

    uint64_t end = clock_ns_val();
    uint64_t elapsed_ns = end - start;

    printf("Benchmark: %d hashes of 15-byte message\n", iterations);
    printf("Elapsed (ns): %lld\n", (long long)elapsed_ns);
    printf("Checksum: %lld\n", (long long)checksum);

    uint64_t bytes_total = (uint64_t)iterations * 15;
    printf("Total bytes hashed: %lld\n", (long long)bytes_total);

    uint64_t throughput_mbps = (bytes_total * 1000) / elapsed_ns;
    printf("Throughput: ~%lld MB/s\n", (long long)throughput_mbps);

    return 0;
}
