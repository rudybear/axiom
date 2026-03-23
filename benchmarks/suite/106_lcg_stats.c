#include <stdio.h>
#include <stdint.h>
int main(void) {
    int64_t seed = 1, a = 1103515245, c = 12345, m = 2147483648LL, sum = 0;
    for (int i = 0; i < 1000; i++) {
        seed = (a * seed + c) % m;
        sum += seed;
    }
    printf("%lld\n", (long long)(sum / 1000));
    return 0;
}
