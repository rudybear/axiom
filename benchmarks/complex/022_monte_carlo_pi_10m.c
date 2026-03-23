#include <stdio.h>
#include <stdint.h>

int main(void) {
    int n = 10000000, inside = 0;
    int64_t seed = 12345, lcg_a = 1103515245, lcg_c = 12345, lcg_m = 2147483648LL;

    for (int i = 0; i < n; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        double x = (double)seed / (double)lcg_m;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        double y = (double)seed / (double)lcg_m;
        if (x*x + y*y <= 1.0) inside++;
    }

    printf("%f\n", 4.0 * (double)inside / (double)n);
    return 0;
}
