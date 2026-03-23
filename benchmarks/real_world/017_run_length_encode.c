#include <stdio.h>
#include <stdint.h>

static int data[500000], decoded[500000];
static int enc_val[500000], enc_cnt[500000];

int main(void) {
    int data_size = 500000;
    int64_t seed = 42, lcg_a = 1103515245LL, lcg_c = 12345LL, lcg_m = 2147483648LL;

    int i = 0;
    while (i < data_size) {
        seed = (lcg_a*seed+lcg_c) % lcg_m;
        int val = (int)(seed % 256);
        seed = (lcg_a*seed+lcg_c) % lcg_m;
        int run_len = 1;
        int64_t r = seed % 100;
        if (r < 40) run_len = 1;
        else if (r < 60) run_len = (int)(seed%5)+2;
        else if (r < 80) run_len = (int)(seed%20)+5;
        else if (r < 95) run_len = (int)(seed%100)+10;
        else run_len = (int)(seed%255)+50;

        for (int j = 0; j < run_len && i < data_size; j++, i++)
            data[i] = val;
    }

    // Encode
    int enc_size = 0, pos = 0;
    while (pos < data_size) {
        int val = data[pos], count = 1;
        while (pos+count < data_size && data[pos+count] == val && count < 255) count++;
        enc_val[enc_size] = val; enc_cnt[enc_size] = count;
        enc_size++; pos += count;
    }

    // Decode
    int dec_pos = 0;
    for (int ei = 0; ei < enc_size; ei++) {
        for (int j = 0; j < enc_cnt[ei] && dec_pos < data_size; j++)
            decoded[dec_pos++] = enc_val[ei];
    }

    int errors = 0;
    for (int idx = 0; idx < data_size; idx++)
        if (decoded[idx] != data[idx]) errors++;

    int max_run = 0; int64_t total_run = 0;
    for (int ei = 0; ei < enc_size; ei++) {
        if (enc_cnt[ei] > max_run) max_run = enc_cnt[ei];
        total_run += enc_cnt[ei];
    }

    int compressed_size = enc_size * 2;

    int hist[256] = {0};
    for (int ei = 0; ei < enc_size; ei++) {
        int bin = enc_cnt[ei]; if (bin > 255) bin = 255;
        hist[bin]++;
    }
    int64_t hist_sum = 0;
    for (int h = 0; h < 256; h++) hist_sum += (int64_t)hist[h] * h;

    int64_t checksum = (int64_t)enc_size*10000 + (int64_t)compressed_size*100 + total_run + (int64_t)max_run*1000 + errors + hist_sum;
    printf("%lld\n", (long long)checksum);
    return 0;
}
