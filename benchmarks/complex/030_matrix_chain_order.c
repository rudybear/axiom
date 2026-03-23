#include <stdio.h>
#include <stdint.h>

int main(void) {
    int n = 200;
    int p[201];
    static int64_t m[40000];

    int64_t seed = 31415, lcg_a = 1103515245, lcg_c = 12345, lcg_m = 2147483648LL;
    for (int i = 0; i <= 200; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        p[i] = (int)(seed % 100) + 10;
    }

    for (int i = 0; i < 40000; i++) m[i] = 0;

    for (int l = 2; l <= n; l++) {
        for (int i = 0; i <= n - l; i++) {
            int j = i + l - 1;
            m[i*200+j] = 9999999999LL;
            for (int k = i; k < j; k++) {
                int64_t cost = m[i*200+k] + m[(k+1)*200+j] + (int64_t)p[i]*(int64_t)p[k+1]*(int64_t)p[j+1];
                if (cost < m[i*200+j]) m[i*200+j] = cost;
            }
        }
    }

    printf("%lld\n", (long long)m[n-1]);
    return 0;
}
