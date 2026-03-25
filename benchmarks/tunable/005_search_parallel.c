#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

static int hash_value(int val) {
    int h = val;
    h ^= h >> 16;
    h *= 73244475;
    h ^= h >> 16;
    h *= 73244475;
    h ^= h >> 16;
    return h;
}

static int64_t search_chunk(int *data, int start, int end, int target_hash_mod) {
    int64_t count = 0;
    int64_t first_match = -1;

    for (int i = start; i < end; i++) {
        int h = hash_value(data[i]);
        int hmod = h & 1023;
        if (hmod == target_hash_mod) {
            count++;
            if (first_match < 0) first_match = (int64_t)i;
        }
    }
    return count * 1000000000LL + first_match + 1;
}

static int64_t merge_results(int64_t *chunk_results, int n_chunks) {
    int64_t total_count = 0;
    int64_t global_first = 999999999LL;

    for (int i = 0; i < n_chunks; i++) {
        int64_t result = chunk_results[i];
        int64_t count = result / 1000000000LL;
        int64_t first = result % 1000000000LL - 1;
        total_count += count;
        if (first >= 0 && first < global_first) global_first = first;
    }
    return total_count * 1000000LL + global_first;
}

int main(void) {
    int n = 20000000;
    int chunk_size = 65536;

    int *data = (int *)calloc(n, sizeof(int));

    int64_t seed = 42;
    for (int i = 0; i < n; i++) {
        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        data[i] = (int)seed;
    }

    int64_t total_checksum = 0;
    int n_chunks = (n + chunk_size - 1) / chunk_size;
    int64_t *chunk_results = (int64_t *)calloc(n_chunks, sizeof(int64_t));

    for (int target = 0; target < 16; target++) {
        for (int ci = 0; ci < n_chunks; ci++) {
            int start = ci * chunk_size;
            int end = start + chunk_size;
            if (end > n) end = n;
            chunk_results[ci] = search_chunk(data, start, end, target);
        }
        int64_t merged = merge_results(chunk_results, n_chunks);
        total_checksum = (total_checksum + merged) % 1000000007LL;
    }

    printf("%lld\n", (long long)total_checksum);

    free(data);
    free(chunk_results);
    return 0;
}
