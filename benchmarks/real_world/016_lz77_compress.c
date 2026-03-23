#include <stdio.h>
#include <stdint.h>

static int data[100000];
static int out_offset[100000], out_length[100000], out_char[100000];

int main(void) {
    int data_size = 100000, window_size = 4096, lookahead_size = 18;

    int64_t seed = 42, lcg_a = 1103515245LL, lcg_c = 12345LL, lcg_m = 2147483648LL;

    for (int i = 0; i < data_size; i++) {
        seed = (lcg_a*seed+lcg_c) % lcg_m;
        int64_t r = seed % 100;
        if (r < 30) {
            int copy_dist = (int)((seed/100) % 50) + 1;
            data[i] = (i >= copy_dist) ? data[i-copy_dist] : (int)(seed%256);
        } else if (r < 60) {
            data[i] = (int)(seed % 16);
        } else {
            data[i] = (int)(seed % 256);
        }
    }

    int out_count = 0, pos = 0;
    while (pos < data_size) {
        int best_offset = 0, best_length = 0;
        int search_start = pos - window_size;
        if (search_start < 0) search_start = 0;

        for (int si = search_start; si < pos; si++) {
            int match_len = 0;
            int max_match = lookahead_size;
            if (pos + max_match > data_size) max_match = data_size - pos;

            while (match_len < max_match && data[si+match_len] == data[pos+match_len])
                match_len++;

            if (match_len > best_length) { best_length = match_len; best_offset = pos - si; }
        }

        if (out_count < 100000) {
            out_offset[out_count] = best_offset;
            out_length[out_count] = best_length;
            out_char[out_count] = (pos+best_length < data_size) ? data[pos+best_length] : 0;
            out_count++;
        }
        pos += best_length + 1;
    }

    int literal_count = 0, match_count = 0, max_match_len = 0;
    int64_t total_match_len = 0;
    for (int i = 0; i < out_count; i++) {
        if (out_length[i] == 0) literal_count++;
        else {
            match_count++; total_match_len += out_length[i];
            if (out_length[i] > max_match_len) max_match_len = out_length[i];
        }
    }

    int64_t compressed_bits = (int64_t)literal_count*9 + (int64_t)match_count*(12+5+9);
    int64_t checksum = (int64_t)out_count*10000 + compressed_bits + total_match_len*100 + max_match_len;
    printf("%lld\n", (long long)checksum);
    return 0;
}
