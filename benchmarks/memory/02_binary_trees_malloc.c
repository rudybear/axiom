#include <stdio.h>
#include <stdlib.h>

/* Binary Trees benchmark (malloc/free per pool) - depth 20 */
/* Node layout: 3 int slots per node [value, left_index, right_index] */

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

int count_nodes(int idx) {
    if (idx == -1) return 0;
    int base = idx * 3;
    return 1 + count_nodes(g_pool[base + 1]) + count_nodes(g_pool[base + 2]);
}

long long checksum_tree(int idx) {
    if (idx == -1) return 0;
    int base = idx * 3;
    long long val = g_pool[base];
    return val + checksum_tree(g_pool[base + 1]) - checksum_tree(g_pool[base + 2]);
}

int main() {
    int max_depth = 20;
    int min_depth = 4;
    int stretch_depth = max_depth + 1;

    /* Stretch tree */
    g_pool = (int *)malloc(12600000 * sizeof(int));
    g_next_idx = 0;
    int stretch_root = build_tree(stretch_depth, 0);
    int stretch_count = count_nodes(stretch_root);
    long long total_check = (long long)stretch_count;
    free(g_pool);

    /* Long-lived tree */
    int *long_pool = (int *)malloc(3200000 * sizeof(int));
    g_pool = long_pool;
    g_next_idx = 0;
    int long_root = build_tree(max_depth, 0);

    /* Iterate over depths */
    int depth = min_depth;
    while (depth <= max_depth) {
        int iterations = 1;
        for (int d = 0; d < (max_depth - depth + min_depth); d++) {
            iterations *= 2;
        }

        long long check = 0;
        for (int i = 0; i < iterations; i++) {
            int *iter_pool = (int *)malloc(3200000 * sizeof(int));
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
