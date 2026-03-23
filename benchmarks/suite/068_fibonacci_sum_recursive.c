#include <stdio.h>
static int fib(int n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}
int main(void) {
    int sum = 0;
    for (int i = 0; i <= 30; i++) sum += fib(i);
    printf("%d\n", sum);
    return 0;
}
