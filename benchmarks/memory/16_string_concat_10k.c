#include <stdio.h>
#include <stdlib.h>

/* Concatenate 10K "strings" (int arrays representing chars) via realloc */

int main() {
    int n = 10000;
    int str_len = 0;
    int str_cap = 16;
    int *str = (int *)malloc(str_cap * sizeof(int));

    long long seed = 42;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    for (int i = 0; i < n; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int append_len = (int)(seed % 11) + 5;

        int needed = str_len + append_len;
        if (needed > str_cap) {
            while (str_cap < needed) str_cap *= 2;
            str = (int *)realloc(str, str_cap * sizeof(int));
        }

        for (int j = 0; j < append_len; j++) {
            seed = (lcg_a * seed + lcg_c) % lcg_m;
            int ch = (int)(seed % 26) + 97;
            str[str_len + j] = ch;
        }
        str_len += append_len;
    }

    long long checksum = 0;
    for (int i = 0; i < str_len; i++) {
        checksum += str[i];
    }
    checksum += str_len;

    free(str);
    printf("%lld\n", checksum);
    return 0;
}
