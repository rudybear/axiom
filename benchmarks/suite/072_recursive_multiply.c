#include <stdio.h>
static int multiply(int a, int b) {
    if (b == 0) return 0;
    if (b > 0) return a + multiply(a, b - 1);
    return -multiply(a, -b);
}
int main(void) { printf("%d\n", multiply(237, 237)); return 0; }
