#include <stdio.h>
static int ackermann(int m, int n) {
    if (m == 0) return n + 1;
    if (n == 0) return ackermann(m - 1, 1);
    return ackermann(m - 1, ackermann(m, n - 1));
}
int main(void) { printf("%d\n", ackermann(2, 3)); return 0; }
