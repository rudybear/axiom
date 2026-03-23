#include <stdio.h>
static int gcd(int a, int b) {
    int x = a, y = b;
    while (y != 0) { int t = y; y = x % y; x = t; }
    return x;
}
static int lcm(int a, int b) { return a / gcd(a, b) * b; }
int main(void) { printf("%d\n", lcm(12, 18)); return 0; }
