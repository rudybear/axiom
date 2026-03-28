/*
 * Base64 Codec -- C reference implementation
 * Turbo-Base64 core algorithm (scalar path)
 * Compile: gcc -O3 -march=native -ffast-math -o base64_c base64.c
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>

/* Encode table: 64 entries */
static const uint8_t encode_table[64] = {
    'A','B','C','D','E','F','G','H','I','J','K','L','M','N','O','P',
    'Q','R','S','T','U','V','W','X','Y','Z','a','b','c','d','e','f',
    'g','h','i','j','k','l','m','n','o','p','q','r','s','t','u','v',
    'w','x','y','z','0','1','2','3','4','5','6','7','8','9','+','/'
};

/* Decode table: 256 entries (0xFF = invalid) */
static const uint8_t decode_table[256] = {
    255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,
    255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,
    255,255,255,255,255,255,255,255,255,255,255, 62,255,255,255, 63,
     52, 53, 54, 55, 56, 57, 58, 59, 60, 61,255,255,255,255,255,255,
    255,  0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14,
     15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,255,255,255,255,255,
    255, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
     41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51,255,255,255,255,255,
    255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,
    255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,
    255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,
    255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,
    255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,
    255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,
    255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,
    255,255,255,255,255,255,255,255,255,255,255,255,255,255,255,255
};

static int base64_encode(const uint8_t *src, int src_len, uint8_t *dst) {
    int si = 0, di = 0;
    int limit = src_len - 2;

    while (si < limit) {
        uint8_t b0 = src[si], b1 = src[si+1], b2 = src[si+2];
        dst[di]   = encode_table[b0 >> 2];
        dst[di+1] = encode_table[((b0 & 3) << 4) | (b1 >> 4)];
        dst[di+2] = encode_table[((b1 & 15) << 2) | (b2 >> 6)];
        dst[di+3] = encode_table[b2 & 63];
        si += 3; di += 4;
    }

    int remain = src_len - si;
    if (remain == 1) {
        uint8_t b0 = src[si];
        dst[di]   = encode_table[b0 >> 2];
        dst[di+1] = encode_table[(b0 & 3) << 4];
        dst[di+2] = '=';
        dst[di+3] = '=';
        di += 4;
    } else if (remain == 2) {
        uint8_t b0 = src[si], b1 = src[si+1];
        dst[di]   = encode_table[b0 >> 2];
        dst[di+1] = encode_table[((b0 & 3) << 4) | (b1 >> 4)];
        dst[di+2] = encode_table[(b1 & 15) << 2];
        dst[di+3] = '=';
        di += 4;
    }

    return di;
}

static int base64_decode(const uint8_t *src, int src_len, uint8_t *dst) {
    int si = 0, di = 0;

    while (si < src_len) {
        uint8_t v0 = decode_table[src[si]];
        uint8_t v1 = decode_table[src[si+1]];
        uint8_t v2 = decode_table[src[si+2]];
        uint8_t v3 = decode_table[src[si+3]];

        dst[di++] = (v0 << 2) | (v1 >> 4);
        if (src[si+2] != '=') dst[di++] = (v1 << 4) | (v2 >> 2);
        if (src[si+3] != '=') dst[di++] = (v2 << 6) | v3;

        si += 4;
    }

    return di;
}

static void fill_test_data(uint8_t *buf, int len) {
    for (int i = 0; i < len; i++) {
        buf[i] = (uint8_t)((i * 137 + 73) & 0xFF);
    }
}

int main(void) {
    printf("=== Base64 Codec (C Reference) ===\n");

    /* Verification */
    const char *input = "Hello, World!";
    int input_len = 13;
    uint8_t enc_buf[20];
    int enc_len = base64_encode((const uint8_t *)input, input_len, enc_buf);

    printf("Encoded length: %d\n", enc_len);
    if (enc_len == 20 && memcmp(enc_buf, "SGVsbG8sIFdvcmxkIQ==", 20) == 0) {
        printf("PASS: encode \"Hello, World!\" matches expected\n");
    } else {
        printf("FAIL: encode mismatch\n");
    }

    uint8_t dec_buf[16];
    int dec_len = base64_decode(enc_buf, enc_len, dec_buf);
    printf("Decoded length: %d\n", dec_len);
    if (dec_len == input_len && memcmp(dec_buf, input, input_len) == 0) {
        printf("PASS: decode roundtrip matches original\n");
    } else {
        printf("FAIL: decode mismatch\n");
    }

    /* Benchmark: 100MB encode + decode */
    int bench_size = 1048576;
    int bench_iters = 100;
    int enc_size = ((bench_size + 2) / 3) * 4;

    uint8_t *bench_src = (uint8_t *)malloc(bench_size);
    uint8_t *bench_enc = (uint8_t *)malloc(enc_size + 4);
    uint8_t *bench_dec = (uint8_t *)malloc(bench_size + 4);

    fill_test_data(bench_src, bench_size);

    printf("Benchmarking: 100MB encode + decode...\n");

    struct timespec ts0, ts1, ts2, ts3;

    /* Encode benchmark */
    clock_gettime(CLOCK_MONOTONIC, &ts0);
    volatile int checksum = 0;
    for (int iter = 0; iter < bench_iters; iter++) {
        int elen = base64_encode(bench_src, bench_size, bench_enc);
        checksum += elen;
    }
    clock_gettime(CLOCK_MONOTONIC, &ts1);
    long encode_ms = (ts1.tv_sec - ts0.tv_sec) * 1000 + (ts1.tv_nsec - ts0.tv_nsec) / 1000000;

    /* Decode benchmark */
    int enc_out_len = base64_encode(bench_src, bench_size, bench_enc);
    clock_gettime(CLOCK_MONOTONIC, &ts2);
    for (int iter = 0; iter < bench_iters; iter++) {
        int dlen = base64_decode(bench_enc, enc_out_len, bench_dec);
        checksum += dlen;
    }
    clock_gettime(CLOCK_MONOTONIC, &ts3);
    long decode_ms = (ts3.tv_sec - ts2.tv_sec) * 1000 + (ts3.tv_nsec - ts2.tv_nsec) / 1000000;

    printf("Encode: %ld ms\n", encode_ms);
    printf("Decode: %ld ms\n", decode_ms);

    long total_bytes = (long)bench_size * bench_iters;
    if (encode_ms > 0) {
        printf("Encode throughput: %ld MB/s\n", (total_bytes * 1000) / (encode_ms * 1048576));
    }
    if (decode_ms > 0) {
        printf("Decode throughput: %ld MB/s\n", (total_bytes * 1000) / (decode_ms * 1048576));
    }

    printf("Checksum (prevent DCE): %d\n", checksum);

    free(bench_src);
    free(bench_enc);
    free(bench_dec);

    printf("=== Base64 complete ===\n");
    return 0;
}
