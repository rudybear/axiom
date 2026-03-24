#include <stdio.h>
#include <stdlib.h>

/* BST: insert 50K keys, search 50K. Arena-simulated (single allocation). */

static int *pool;

int bst_insert(int root, int new_node) {
    int new_key = pool[new_node * 3];
    if (root == -1) return new_node;
    int cur = root;
    while (1) {
        int cur_key = pool[cur * 3];
        if (new_key < cur_key) {
            int left = pool[cur * 3 + 1];
            if (left == -1) { pool[cur * 3 + 1] = new_node; return root; }
            cur = left;
        } else {
            int right = pool[cur * 3 + 2];
            if (right == -1) { pool[cur * 3 + 2] = new_node; return root; }
            cur = right;
        }
    }
}

int bst_search(int root, int key) {
    int cur = root;
    while (cur != -1) {
        int cur_key = pool[cur * 3];
        if (key == cur_key) return 1;
        if (key < cur_key) cur = pool[cur * 3 + 1];
        else cur = pool[cur * 3 + 2];
    }
    return 0;
}

int main() {
    int n = 50000;
    /* Single arena-like allocation for pool + keys */
    int *arena = (int *)malloc((n * 3 + n) * sizeof(int));
    pool = arena;
    int *keys = arena + n * 3;
    int root = -1;

    long long seed = 777;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    for (int i = 0; i < n; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int key = (int)(seed % 10000000);
        keys[i] = key;
        pool[i * 3] = key;
        pool[i * 3 + 1] = -1;
        pool[i * 3 + 2] = -1;
        root = bst_insert(root, i);
    }

    int found_count = 0;
    for (int i = 0; i < n; i++) {
        found_count += bst_search(root, keys[i]);
    }

    int miss_count = 0;
    for (int i = 0; i < n; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int key = (int)(seed % 10000000) + 10000000;
        miss_count += (1 - bst_search(root, key));
    }

    free(arena);

    long long checksum = (long long)found_count * 1000 + miss_count;
    printf("%lld\n", checksum);
    return 0;
}
