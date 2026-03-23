#include <stdio.h>
static int gcd(int a, int b) {
    int x = a, y = b;
    while (y != 0) { int t = y; y = x % y; x = t; }
    return x;
}
int main(void) {
    int count = 0;
    for (int i = 1; i <= 50; i++)
        if (gcd(i, 30) == 1) count++;
    printf("%d\n", count);
    return 0;
}
