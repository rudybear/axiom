#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

int count_factors(int64_t n) {
    int64_t x = n;
    int count = 0;
    while (x % 2 == 0) { count++; x /= 2; }
    for (int64_t i = 3; i * i <= x; i += 2) {
        while (x % i == 0) { count++; x /= i; }
    }
    if (x > 1) count++;
    return count;
}

int main(void) {
    int total_factors = 0;
    int64_t seed = 999999937LL;

    for (int i = 0; i < 1000; i++) {
        seed = seed * 1103515245LL + 12345LL;
        int64_t hi = (seed / 65536) % 1000000;
        if (hi < 0) hi = -hi;
        seed = seed * 1103515245LL + 12345LL;
        int64_t lo = (seed / 65536) % 1000000;
        if (lo < 0) lo = -lo;
        int64_t num = hi * 1000000 + lo + 2;
        if (num < 2) num = 2;
        total_factors += count_factors(num);
    }

    printf("%d\n", total_factors);
    return 0;
}
