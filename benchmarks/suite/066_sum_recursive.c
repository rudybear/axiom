#include <stdio.h>
static int sum_to(int n) {
    if (n == 0) return 0;
    return n + sum_to(n - 1);
}
int main(void) { printf("%d\n", sum_to(100)); return 0; }
