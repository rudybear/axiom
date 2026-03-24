#include <stdio.h>
#include <stdlib.h>

/* Hash map with separate chaining. 50K inserts + 50K lookups. Heap-allocated. */

static int hash_key(int key, int mask) {
    long long h = (long long)key * 2654435761LL;
    int h32 = (int)(h % 4294967296LL);
    if (h32 < 0) return (-h32) % (mask + 1);
    return h32 % (mask + 1);
}

int main() {
    int num_buckets = 65536;
    int mask = 65535;
    int max_nodes = 50000;

    int *buckets = (int *)malloc(num_buckets * sizeof(int));
    for (int i = 0; i < num_buckets; i++) buckets[i] = -1;

    /* Node pool: 3 slots per node [key, value, next] */
    int *pool = (int *)malloc(max_nodes * 3 * sizeof(int));
    int pool_next = 0;

    long long seed = 42;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    int *keys = (int *)malloc(max_nodes * sizeof(int));

    /* INSERT 50K */
    for (int i = 0; i < max_nodes; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int key = (int)(seed % 10000000);
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int val = (int)(seed % 1000000);
        keys[i] = key;

        int h = hash_key(key, mask);
        int node = pool_next++;
        int base = node * 3;
        pool[base] = key;
        pool[base + 1] = val;
        pool[base + 2] = buckets[h];
        buckets[h] = node;
    }

    /* LOOKUP 50K */
    int found_count = 0;
    long long val_sum = 0;
    for (int i = 0; i < max_nodes; i++) {
        int key = keys[i];
        int h = hash_key(key, mask);
        int cur = buckets[h];
        int found = 0;
        while (cur != -1 && !found) {
            int base = cur * 3;
            if (pool[base] == key) {
                val_sum += pool[base + 1];
                found_count++;
                found = 1;
            }
            if (!found) cur = pool[base + 2];
        }
    }

    free(buckets);
    free(pool);
    free(keys);

    long long checksum = val_sum + (long long)found_count * 1000;
    printf("%lld\n", checksum);
    return 0;
}
