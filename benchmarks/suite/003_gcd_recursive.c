#include <stdio.h>
static int gcd(int a, int b) {
    if (b == 0) return a;
    return gcd(b, a % b);
}
int main(void) { printf("%d\n", gcd(48, 36)); return 0; }
