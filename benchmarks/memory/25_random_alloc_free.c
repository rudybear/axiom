#include <stdio.h>
#include <stdlib.h>

/* Allocate 10K blocks, free in pseudo-random order (stresses fragmentation) */

int main() {
    int n = 10000;
    int block_size = 64;
    long long checksum = 0;

    long long seed = 42;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    for (int round = 0; round < 30; round++) {
        int *pool = (int *)malloc(n * block_size * sizeof(int));
        int *active = (int *)calloc(n, sizeof(int));

        for (int i = 0; i < n; i++) {
            int base = i * block_size;
            seed = (lcg_a * seed + lcg_c) % lcg_m;
            pool[base] = (int)(seed % 1000000);
            pool[base + block_size - 1] = (int)(seed % 999);
            active[i] = 1;
        }

        int freed = 0;
        while (freed < n) {
            seed = (lcg_a * seed + lcg_c) % lcg_m;
            int idx = (int)(seed % n);
            if (active[idx]) {
                int base = idx * block_size;
                checksum += pool[base];
                checksum += pool[base + block_size - 1];
                active[idx] = 0;
                freed++;
            }
        }

        free(pool);
        free(active);
    }

    for (int round = 0; round < 100; round++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        int sz = (int)(seed % 200) + 10;
        int *p = (int *)malloc(sz * sizeof(int));
        for (int j = 0; j < sz; j++)
            p[j] = j * round + (int)(seed % 100);
        long long s = 0;
        for (int j = 0; j < sz; j++) s += p[j];
        checksum += s;
        free(p);
    }

    printf("%lld\n", checksum);
    return 0;
}
