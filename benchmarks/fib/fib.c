/* Recursive Fibonacci benchmark — C reference.
 * From github.com/drujensen/fib
 *
 * Compile: clang -O2 -o fib fib.c
 * Expected output: 2971215073
 */
#include <stdio.h>
#include <stdint.h>

static int64_t fib(int64_t n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}

int main(void) {
    printf("%lld\n", (long long)fib(47));
    return 0;
}
