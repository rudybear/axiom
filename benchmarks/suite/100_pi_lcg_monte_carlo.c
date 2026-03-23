#include <stdio.h>
#include <stdint.h>
int main(void) {
    int n = 1000000, inside = 0;
    int64_t seed = 12345, a = 1103515245, c = 12345, m = 2147483648LL;
    for (int i = 0; i < n; i++) {
        seed = (a * seed + c) % m;
        double x = (double)seed / (double)m;
        seed = (a * seed + c) % m;
        double y = (double)seed / (double)m;
        if (x * x + y * y <= 1.0) inside++;
    }
    printf("%f\n", 4.0 * (double)inside / (double)n);
    return 0;
}
