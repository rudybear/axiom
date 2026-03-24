#include <stdio.h>
#include <stdlib.h>

/* Classic Benchmarks Game binary-trees at depth 21. */

static int *g_pool;
static int g_next_idx;

int build_tree(int depth, int value) {
    int idx = g_next_idx++;
    int base = idx * 3;
    g_pool[base] = value;
    if (depth == 0) {
        g_pool[base + 1] = -1;
        g_pool[base + 2] = -1;
        return idx;
    }
    int left = build_tree(depth - 1, 2 * value - 1);
    int right = build_tree(depth - 1, 2 * value);
    g_pool[base + 1] = left;
    g_pool[base + 2] = right;
    return idx;
}

long long checksum_tree(int idx) {
    if (idx == -1) return 0;
    int base = idx * 3;
    long long val = g_pool[base];
    return val + checksum_tree(g_pool[base + 1]) - checksum_tree(g_pool[base + 2]);
}

int main() {
    int max_depth = 21;
    int min_depth = 4;

    /* Long-lived tree */
    int *long_pool = (int *)malloc(6300000 * sizeof(int));
    g_pool = long_pool;
    g_next_idx = 0;
    int long_root = build_tree(max_depth, 0);

    long long total_check = 0;

    int depth = min_depth;
    while (depth <= max_depth) {
        int iterations = 1;
        for (int d = 0; d < (max_depth - depth + min_depth); d++)
            iterations *= 2;

        int max_iters = iterations;
        if (max_iters > 1000) max_iters = 1000;

        long long check = 0;
        for (int i = 0; i < max_iters; i++) {
            int *iter_pool = (int *)malloc(6300000 * sizeof(int));
            g_pool = iter_pool;
            g_next_idx = 0;
            int root = build_tree(depth, i);
            check += checksum_tree(root);
            free(iter_pool);
        }
        total_check += check;
        depth += 2;
    }

    g_pool = long_pool;
    total_check += checksum_tree(long_root);

    free(long_pool);
    printf("%lld\n", total_check);
    return 0;
}
