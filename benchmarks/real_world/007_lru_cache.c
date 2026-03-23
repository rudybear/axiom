#include <stdio.h>
#include <stdint.h>

static int64_t node_key[4098], node_val[4098];
static int node_prev[4098], node_next[4098], node_hash_next[4098];
static int hash_bucket[8192];
static int free_stack[4096];

int main(void) {
    int cap = 4096, hash_size = 8192;
    int head = 0, tail = 1;

    node_next[head] = tail; node_prev[tail] = head;
    node_prev[head] = -1; node_next[tail] = -1;

    int free_top = 4096;
    for (int i = 0; i < 4096; i++) free_stack[i] = i + 2;
    for (int i = 0; i < 8192; i++) hash_bucket[i] = -1;
    for (int i = 0; i < 4098; i++) node_hash_next[i] = -1;

    int size = 0;
    int64_t hash_mult = 2654435761LL, mod32 = 4294967296LL;
    int64_t seed = 42, lcg_a = 1103515245LL, lcg_c = 12345LL, lcg_m = 2147483648LL;

    int hit_count = 0, miss_count = 0, evict_count = 0;
    int64_t val_sum = 0;

    for (int op = 0; op < 100000; op++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int64_t op_key = seed % 8000;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int64_t op_val = seed;
        int is_get = (int)(seed % 3);

        int h = (int)((op_key * hash_mult % mod32) % hash_size);

        if (is_get == 0) {
            // GET
            int found_node = -1;
            for (int cur = hash_bucket[h]; cur != -1 && found_node == -1; cur = node_hash_next[cur]) {
                if (node_key[cur] == op_key) found_node = cur;
            }
            if (found_node != -1) {
                hit_count++; val_sum += node_val[found_node];
                int p = node_prev[found_node], n = node_next[found_node];
                node_next[p] = n; node_prev[n] = p;
                int old_first = node_next[head];
                node_next[head] = found_node; node_prev[found_node] = head;
                node_next[found_node] = old_first; node_prev[old_first] = found_node;
            } else { miss_count++; }
        } else {
            // PUT
            int found_node = -1;
            for (int cur = hash_bucket[h]; cur != -1 && found_node == -1; cur = node_hash_next[cur]) {
                if (node_key[cur] == op_key) found_node = cur;
            }
            if (found_node != -1) {
                node_val[found_node] = op_val;
                int p = node_prev[found_node], n = node_next[found_node];
                node_next[p] = n; node_prev[n] = p;
                int old_first = node_next[head];
                node_next[head] = found_node; node_prev[found_node] = head;
                node_next[found_node] = old_first; node_prev[old_first] = found_node;
            } else {
                if (size >= cap) {
                    int lru = node_prev[tail], lru_p = node_prev[lru];
                    node_next[lru_p] = tail; node_prev[tail] = lru_p;
                    int lru_h = (int)((node_key[lru] * hash_mult % mod32) % hash_size);
                    int prev_h = -1, removed = 0;
                    for (int hcur = hash_bucket[lru_h]; hcur != -1 && !removed;) {
                        if (hcur == lru) {
                            if (prev_h == -1) hash_bucket[lru_h] = node_hash_next[lru];
                            else node_hash_next[prev_h] = node_hash_next[lru];
                            removed = 1;
                        } else { prev_h = hcur; hcur = node_hash_next[hcur]; }
                    }
                    free_top--; if (free_top < 0) free_top = 0;
                    free_stack[free_top] = lru;
                    size--; evict_count++;
                }
                if (free_top > 0) {
                    int new_node = free_stack[--free_top];
                    node_key[new_node] = op_key; node_val[new_node] = op_val;
                    int old_first = node_next[head];
                    node_next[head] = new_node; node_prev[new_node] = head;
                    node_next[new_node] = old_first; node_prev[old_first] = new_node;
                    node_hash_next[new_node] = hash_bucket[h]; hash_bucket[h] = new_node;
                    size++;
                }
            }
        }
    }

    int64_t checksum = val_sum + (int64_t)hit_count*1000 + (int64_t)miss_count*100 + (int64_t)evict_count*10 + size;
    printf("%lld\n", (long long)checksum);
    return 0;
}
