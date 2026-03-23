#include <stdio.h>
static int count_paths(int m, int n) {
    if (m == 1 || n == 1) return 1;
    return count_paths(m - 1, n) + count_paths(m, n - 1);
}
int main(void) { printf("%d\n", count_paths(10, 10)); return 0; }
