#include <stdio.h>
#include <stdlib.h>

/* Naive string search in a large "text" (heap-allocated int array) */

int main() {
    int text_len = 500000;
    int pat_len = 8;

    int *text = (int *)malloc(text_len * sizeof(int));
    int *pattern = (int *)malloc(pat_len * sizeof(int));

    long long seed = 42;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    for (int i = 0; i < text_len; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        text[i] = (int)(seed % 4) + 97;
    }

    pattern[0] = 97; pattern[1] = 98; pattern[2] = 99; pattern[3] = 97;
    pattern[4] = 98; pattern[5] = 99; pattern[6] = 97; pattern[7] = 98;

    int match_count = 0;
    long long match_pos_sum = 0;
    int limit = text_len - pat_len + 1;

    for (int pass = 0; pass < 10; pass++) {
        for (int i = 0; i < limit; i++) {
            int match = 1;
            for (int j = 0; j < pat_len && match; j++) {
                if (text[i + j] != pattern[j]) match = 0;
            }
            if (match) {
                match_count++;
                match_pos_sum += i;
            }
        }
    }

    free(text);
    free(pattern);

    long long checksum = (long long)match_count * 1000 + match_pos_sum;
    printf("%lld\n", checksum);
    return 0;
}
