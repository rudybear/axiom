/*
 * Roaring Bitmap -- C reference implementation (core operations)
 * Compile: gcc -O3 -march=native -ffast-math -o roaring_c roaring.c
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>

#define BITSET_WORDS 1024

/* popcount64 via compiler builtin or SWAR fallback */
static inline int popcount64(uint64_t x) {
#if defined(__GNUC__) || defined(__clang__)
    return __builtin_popcountll(x);
#else
    x -= (x >> 1) & 0x5555555555555555ULL;
    x = (x & 0x3333333333333333ULL) + ((x >> 2) & 0x3333333333333333ULL);
    x = (x + (x >> 4)) & 0x0F0F0F0F0F0F0F0FULL;
    return (int)((x * 0x0101010101010101ULL) >> 56);
#endif
}

/* ===== Bitset Container ===== */

static uint64_t *bitset_create(void) {
    return (uint64_t *)calloc(BITSET_WORDS, sizeof(uint64_t));
}

static inline void bitset_set(uint64_t *bs, int k) {
    bs[k >> 6] |= (1ULL << (k & 63));
}

static inline int bitset_test(const uint64_t *bs, int k) {
    return (int)((bs[k >> 6] >> (k & 63)) & 1);
}

static int bitset_cardinality(const uint64_t *bs) {
    int count = 0;
    for (int i = 0; i < BITSET_WORDS; i++)
        count += popcount64(bs[i]);
    return count;
}

static void bitset_union(const uint64_t *a, const uint64_t *b, uint64_t *dst) {
    for (int i = 0; i < BITSET_WORDS; i++)
        dst[i] = a[i] | b[i];
}

static void bitset_intersection(const uint64_t *a, const uint64_t *b, uint64_t *dst) {
    for (int i = 0; i < BITSET_WORDS; i++)
        dst[i] = a[i] & b[i];
}

static int bitset_union_count(const uint64_t *a, const uint64_t *b) {
    int count = 0;
    for (int i = 0; i < BITSET_WORDS; i++)
        count += popcount64(a[i] | b[i]);
    return count;
}

static int bitset_intersection_count(const uint64_t *a, const uint64_t *b) {
    int count = 0;
    for (int i = 0; i < BITSET_WORDS; i++)
        count += popcount64(a[i] & b[i]);
    return count;
}

/* ===== Array Container ===== */

typedef struct {
    int len;
    int cap;
    int *data;
} array_container;

static array_container *array_create(int capacity) {
    array_container *a = (array_container *)malloc(sizeof(array_container));
    a->len = 0;
    a->cap = capacity;
    a->data = (int *)malloc(capacity * sizeof(int));
    return a;
}

static inline void array_append(array_container *a, int val) {
    a->data[a->len++] = val;
}

static array_container *array_union(const array_container *a, const array_container *b) {
    array_container *dst = array_create(a->len + b->len);
    int ai = 0, bi = 0;
    while (ai < a->len && bi < b->len) {
        if (a->data[ai] < b->data[bi]) {
            array_append(dst, a->data[ai++]);
        } else if (a->data[ai] > b->data[bi]) {
            array_append(dst, b->data[bi++]);
        } else {
            array_append(dst, a->data[ai++]);
            bi++;
        }
    }
    while (ai < a->len) array_append(dst, a->data[ai++]);
    while (bi < b->len) array_append(dst, b->data[bi++]);
    return dst;
}

static array_container *array_intersection(const array_container *a, const array_container *b) {
    int min_len = a->len < b->len ? a->len : b->len;
    array_container *dst = array_create(min_len);
    int ai = 0, bi = 0;
    while (ai < a->len && bi < b->len) {
        if (a->data[ai] < b->data[bi]) ai++;
        else if (a->data[ai] > b->data[bi]) bi++;
        else { array_append(dst, a->data[ai++]); bi++; }
    }
    return dst;
}

static void array_free(array_container *a) {
    free(a->data);
    free(a);
}

int main(void) {
    printf("=== Roaring Bitmap (C Reference) ===\n");

    /* Bitset container tests */
    printf("-- Bitset container tests --\n");
    uint64_t *bs1 = bitset_create();
    uint64_t *bs2 = bitset_create();

    bitset_set(bs1, 0); bitset_set(bs1, 1); bitset_set(bs1, 2);
    bitset_set(bs1, 100); bitset_set(bs1, 1000);
    bitset_set(bs1, 10000); bitset_set(bs1, 50000);

    int card1 = bitset_cardinality(bs1);
    printf("bs1 cardinality: %d\n", card1);
    printf("%s: bs1 cardinality\n", card1 == 7 ? "PASS" : "FAIL");
    printf("%s: bit 100 is set\n", bitset_test(bs1, 100) ? "PASS" : "FAIL");
    printf("%s: bit 99 is not set\n", !bitset_test(bs1, 99) ? "PASS" : "FAIL");

    bitset_set(bs2, 1); bitset_set(bs2, 2); bitset_set(bs2, 3);
    bitset_set(bs2, 100); bitset_set(bs2, 2000);
    bitset_set(bs2, 10000); bitset_set(bs2, 60000);

    uint64_t *bs_union = bitset_create();
    bitset_union(bs1, bs2, bs_union);
    int union_card = bitset_cardinality(bs_union);
    printf("Union cardinality: %d\n", union_card);
    printf("%s: union cardinality\n", union_card == 10 ? "PASS" : "FAIL");

    uint64_t *bs_inter = bitset_create();
    bitset_intersection(bs1, bs2, bs_inter);
    int inter_card = bitset_cardinality(bs_inter);
    printf("Intersection cardinality: %d\n", inter_card);
    printf("%s: intersection cardinality\n", inter_card == 4 ? "PASS" : "FAIL");

    printf("%s: lazy union count\n", bitset_union_count(bs1, bs2) == 10 ? "PASS" : "FAIL");
    printf("%s: lazy intersection count\n", bitset_intersection_count(bs1, bs2) == 4 ? "PASS" : "FAIL");

    free(bs1); free(bs2); free(bs_union); free(bs_inter);

    /* Array container tests */
    printf("-- Array container tests --\n");
    array_container *arr1 = array_create(10);
    array_container *arr2 = array_create(10);

    array_append(arr1, 10); array_append(arr1, 20);
    array_append(arr1, 30); array_append(arr1, 40); array_append(arr1, 50);
    array_append(arr2, 20); array_append(arr2, 30);
    array_append(arr2, 60); array_append(arr2, 70);

    array_container *arr_u = array_union(arr1, arr2);
    printf("Array union length: %d\n", arr_u->len);
    printf("%s: array union\n", arr_u->len == 7 ? "PASS" : "FAIL");

    array_container *arr_i = array_intersection(arr1, arr2);
    printf("Array intersection length: %d\n", arr_i->len);
    printf("%s: array intersection\n", arr_i->len == 2 ? "PASS" : "FAIL");

    array_free(arr1); array_free(arr2); array_free(arr_u); array_free(arr_i);

    /* Benchmark: bitset operations on 10K-element bitmaps */
    printf("-- Benchmark: bitset operations --\n");
    uint64_t *bench_bs1 = bitset_create();
    uint64_t *bench_bs2 = bitset_create();
    uint64_t *bench_dst = bitset_create();

    for (int i = 0; i < 10000; i++) {
        bitset_set(bench_bs1, (i * 7 + 3) & 65535);
        bitset_set(bench_bs2, (i * 11 + 5) & 65535);
    }

    printf("Bitmap A cardinality: %d\n", bitset_cardinality(bench_bs1));
    printf("Bitmap B cardinality: %d\n", bitset_cardinality(bench_bs2));

    int bench_iters = 100000;
    volatile int checksum = 0;
    struct timespec ts0, ts1, ts2, ts3, ts4, ts5, ts6, ts7;

    clock_gettime(CLOCK_MONOTONIC, &ts0);
    for (int iter = 0; iter < bench_iters; iter++) {
        bitset_union(bench_bs1, bench_bs2, bench_dst);
        checksum += bitset_test(bench_dst, 0);
    }
    clock_gettime(CLOCK_MONOTONIC, &ts1);
    long union_ms = (ts1.tv_sec - ts0.tv_sec) * 1000 + (ts1.tv_nsec - ts0.tv_nsec) / 1000000;
    printf("Union: %ld ms for %d iterations\n", union_ms, bench_iters);

    clock_gettime(CLOCK_MONOTONIC, &ts2);
    for (int iter = 0; iter < bench_iters; iter++) {
        bitset_intersection(bench_bs1, bench_bs2, bench_dst);
        checksum += bitset_test(bench_dst, 0);
    }
    clock_gettime(CLOCK_MONOTONIC, &ts3);
    long inter_ms = (ts3.tv_sec - ts2.tv_sec) * 1000 + (ts3.tv_nsec - ts2.tv_nsec) / 1000000;
    printf("Intersection: %ld ms for %d iterations\n", inter_ms, bench_iters);

    clock_gettime(CLOCK_MONOTONIC, &ts4);
    for (int iter = 0; iter < bench_iters; iter++) {
        checksum += bitset_cardinality(bench_bs1);
    }
    clock_gettime(CLOCK_MONOTONIC, &ts5);
    long card_ms = (ts5.tv_sec - ts4.tv_sec) * 1000 + (ts5.tv_nsec - ts4.tv_nsec) / 1000000;
    printf("Cardinality: %ld ms for %d iterations\n", card_ms, bench_iters);

    clock_gettime(CLOCK_MONOTONIC, &ts6);
    for (int iter = 0; iter < bench_iters; iter++) {
        checksum += bitset_union_count(bench_bs1, bench_bs2);
    }
    clock_gettime(CLOCK_MONOTONIC, &ts7);
    long ucount_ms = (ts7.tv_sec - ts6.tv_sec) * 1000 + (ts7.tv_nsec - ts6.tv_nsec) / 1000000;
    printf("Union count (lazy): %ld ms for %d iterations\n", ucount_ms, bench_iters);

    if (union_ms > 0) {
        long ops_per_sec = (long)bench_iters * 1000 / union_ms;
        long mbps = ops_per_sec * 8192 * 2 / 1048576;
        printf("Union throughput: %ld MB/s\n", mbps);
    }

    printf("Checksum (prevent DCE): %d\n", checksum);

    free(bench_bs1); free(bench_bs2); free(bench_dst);

    printf("=== Roaring complete ===\n");
    return 0;
}
