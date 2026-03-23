#include <stdio.h>
#include <stdint.h>
static int64_t mod_exp(int64_t base, int64_t exp, int64_t modulus) {
    int64_t result = 1, b = base % modulus, e = exp;
    while (e > 0) {
        if (e % 2 == 1) result = result * b % modulus;
        e /= 2;
        b = b * b % modulus;
    }
    return result;
}
int main(void) { printf("%lld\n", (long long)mod_exp(7, 256, 1000)); return 0; }
