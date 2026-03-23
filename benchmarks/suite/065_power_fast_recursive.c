#include <stdio.h>
#include <stdint.h>
static int64_t fast_power(int64_t base, int exp) {
    if (exp == 0) return 1;
    if (exp % 2 == 0) { int64_t half = fast_power(base, exp/2); return half * half; }
    return base * fast_power(base, exp - 1);
}
int main(void) { printf("%lld\n", (long long)fast_power(2, 30)); return 0; }
