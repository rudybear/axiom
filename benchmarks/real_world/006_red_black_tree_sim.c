#include <stdio.h>
#include <stdint.h>

static int64_t key_arr[60000];
static int left_arr[60000], right_arr[60000], parent_arr[60000], color_arr[60000];
static int64_t insert_keys[50000];

int main(void) {
    int nil = 0, root = 0, node_count = 1;
    color_arr[0] = 0; left_arr[0] = 0; right_arr[0] = 0; parent_arr[0] = 0;

    int64_t seed = 98765, lcg_a = 1103515245LL, lcg_c = 12345LL, lcg_m = 2147483648LL;

    for (int ins = 0; ins < 50000; ins++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int64_t new_key = seed;
        insert_keys[ins] = new_key;

        int new_node = node_count++;
        key_arr[new_node] = new_key;
        left_arr[new_node] = nil; right_arr[new_node] = nil;
        color_arr[new_node] = 1; parent_arr[new_node] = nil;

        if (root == nil) { root = new_node; color_arr[new_node] = 0; continue; }

        int cur = root, par = nil, go_left = 0, placed = 0;
        while (cur != nil && !placed) {
            par = cur;
            if (new_key < key_arr[cur]) { go_left = 1; cur = left_arr[cur]; }
            else { go_left = 0; cur = right_arr[cur]; }
            if (cur == nil) placed = 1;
        }

        parent_arr[new_node] = par;
        if (new_key < key_arr[par]) left_arr[par] = new_node;
        else right_arr[par] = new_node;

        int z = new_node, fix_count = 0;
        while (parent_arr[z] != nil && color_arr[parent_arr[z]] == 1 && fix_count < 20) {
            int p = parent_arr[z], gp = parent_arr[p];
            if (gp == nil) { color_arr[p] = 0; break; }
            if (p == left_arr[gp]) {
                int uncle = right_arr[gp];
                if (color_arr[uncle] == 1) {
                    color_arr[p] = 0; color_arr[uncle] = 0; color_arr[gp] = 1; z = gp;
                } else { color_arr[p] = 0; color_arr[gp] = 1; break; }
            } else {
                int uncle = left_arr[gp];
                if (color_arr[uncle] == 1) {
                    color_arr[p] = 0; color_arr[uncle] = 0; color_arr[gp] = 1; z = gp;
                } else { color_arr[p] = 0; color_arr[gp] = 1; break; }
            }
            fix_count++;
        }
        color_arr[root] = 0;
    }

    int found_count = 0;
    int64_t depth_sum = 0;
    for (int s = 0; s < 50000; s++) {
        int64_t search_key = insert_keys[s];
        int cur = root, depth = 0, found = 0;
        while (cur != nil && !found) {
            depth++;
            if (key_arr[cur] == search_key) { found = 1; found_count++; }
            else if (search_key < key_arr[cur]) cur = left_arr[cur];
            else cur = right_arr[cur];
        }
        depth_sum += depth;
    }

    int red_count = 0, black_count = 0;
    for (int i = 1; i < node_count; i++) {
        if (color_arr[i] == 1) red_count++;
        else black_count++;
    }

    int64_t checksum = (int64_t)found_count * 10000 + depth_sum + red_count + (int64_t)black_count * 100;
    printf("%lld\n", (long long)checksum);
    return 0;
}
