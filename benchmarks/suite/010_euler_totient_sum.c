#include <stdio.h>
static int gcd(int a, int b) {
    int x = a, y = b;
    while (y != 0) { int t = y; y = x % y; x = t; }
    return x;
}
static int totient(int n) {
    int count = 0;
    for (int i = 1; i <= n; i++)
        if (gcd(i, n) == 1) count++;
    return count;
}
int main(void) {
    int sum = 0;
    for (int n = 1; n <= 100; n++) sum += totient(n);
    printf("%d\n", sum);
    return 0;
}
