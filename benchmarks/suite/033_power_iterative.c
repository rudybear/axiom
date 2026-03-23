#include <stdio.h>
#include <stdint.h>
static int64_t power(int64_t base, int exp) {
    int64_t result = 1;
    for (int i = 0; i < exp; i++) result *= base;
    return result;
}
int main(void) { printf("%lld\n", (long long)power(2, 30)); return 0; }
