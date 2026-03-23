#include <stdio.h>
static int gcd(int a, int b) {
    if (b == 0) return a;
    return gcd(b, a % b);
}
int main(void) {
    int result = 0;
    for (int i = 1; i <= 1000; i++) result = gcd(i, 997);
    printf("%d\n", result);
    return 0;
}
