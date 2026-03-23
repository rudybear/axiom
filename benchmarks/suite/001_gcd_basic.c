#include <stdio.h>
static int gcd(int a, int b) {
    int x = a, y = b;
    while (y != 0) { int t = y; y = x % y; x = t; }
    return x;
}
int main(void) { printf("%d\n", gcd(48, 36)); return 0; }
