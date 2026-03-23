#include <stdio.h>
#include <stdint.h>

static int64_t freq[256];
static int data[200000];
static int64_t tree_freq[511];
static int tree_left[511], tree_right[511], tree_leaf[511], tree_sym[511], active[511];
static int code_len[256];
static int stack_node[512], stack_depth[512];

int main(void) {
    int data_size = 200000;

    int64_t seed = 42, lcg_a = 1103515245LL, lcg_c = 12345LL, lcg_m = 2147483648LL;

    for (int i = 0; i < data_size; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int64_t raw = seed % 1000;
        int byte_val = 0;
        if (raw < 200) byte_val = (int)(seed % 10);
        else if (raw < 500) byte_val = 10 + (int)(seed % 30);
        else if (raw < 800) byte_val = 40 + (int)(seed % 60);
        else byte_val = 100 + (int)(seed % 156);
        data[i] = byte_val;
        freq[byte_val]++;
    }

    int node_count = 0;
    for (int i = 0; i < 256; i++) {
        if (freq[i] > 0) {
            tree_freq[node_count] = freq[i];
            tree_left[node_count] = -1; tree_right[node_count] = -1;
            tree_leaf[node_count] = 1; tree_sym[node_count] = i;
            active[node_count] = 1;
            node_count++;
        }
    }
    int active_count = node_count;

    while (active_count > 1) {
        int min1 = -1; int64_t min1_freq = 9999999999LL;
        for (int i = 0; i < node_count; i++) {
            if (active[i] && tree_freq[i] < min1_freq) { min1 = i; min1_freq = tree_freq[i]; }
        }
        active[min1] = 0;

        int min2 = -1; int64_t min2_freq = 9999999999LL;
        for (int i = 0; i < node_count; i++) {
            if (active[i] && tree_freq[i] < min2_freq) { min2 = i; min2_freq = tree_freq[i]; }
        }
        active[min2] = 0;

        tree_freq[node_count] = min1_freq + min2_freq;
        tree_left[node_count] = min1; tree_right[node_count] = min2;
        tree_leaf[node_count] = 0; tree_sym[node_count] = -1;
        active[node_count] = 1;
        node_count++; active_count--;
    }

    int root = node_count - 1;
    stack_node[0] = root; stack_depth[0] = 0;
    int sp = 1;

    while (sp > 0) {
        sp--;
        int cur = stack_node[sp], depth = stack_depth[sp];
        if (tree_leaf[cur]) {
            code_len[tree_sym[cur]] = depth;
        } else {
            if (tree_left[cur] >= 0) { stack_node[sp] = tree_left[cur]; stack_depth[sp] = depth+1; sp++; }
            if (tree_right[cur] >= 0) { stack_node[sp] = tree_right[cur]; stack_depth[sp] = depth+1; sp++; }
        }
    }

    int64_t total_bits = 0;
    for (int i = 0; i < 256; i++) total_bits += freq[i] * code_len[i];

    int unique_count = 0, max_code_len = 0;
    for (int i = 0; i < 256; i++) {
        if (freq[i] > 0) {
            unique_count++;
            if (code_len[i] > max_code_len) max_code_len = code_len[i];
        }
    }

    int64_t checksum = total_bits * 100 + (int64_t)unique_count * 10000 + max_code_len;
    printf("%lld\n", (long long)checksum);
    return 0;
}
