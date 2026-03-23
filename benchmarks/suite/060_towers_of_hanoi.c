#include <stdio.h>
static int hanoi_count(int n) {
    if (n == 0) return 0;
    return 2 * hanoi_count(n - 1) + 1;
}
int main(void) { printf("%d\n", hanoi_count(10)); return 0; }
