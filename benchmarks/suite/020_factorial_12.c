#include <stdio.h>
#include <stdint.h>
static int64_t factorial(int n) {
    int64_t result = 1;
    for (int i = 2; i <= n; i++) result *= i;
    return result;
}
int main(void) { printf("%lld\n", (long long)factorial(12)); return 0; }
