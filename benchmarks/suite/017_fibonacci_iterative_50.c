#include <stdio.h>
#include <stdint.h>
static int64_t fib(int n) {
    if (n <= 1) return n;
    int64_t a = 0, b = 1;
    for (int i = 2; i <= n; i++) { int64_t t = b; b = a + b; a = t; }
    return b;
}
int main(void) { printf("%lld\n", (long long)fib(50)); return 0; }
