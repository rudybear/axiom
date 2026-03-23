#include <stdio.h>
static int mc91(int n) {
    if (n > 100) return n - 10;
    return mc91(mc91(n + 11));
}
int main(void) { printf("%d\n", mc91(42)); return 0; }
