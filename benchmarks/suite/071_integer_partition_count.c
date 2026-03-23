#include <stdio.h>
static int partitions(int n, int k) {
    if (n == 0) return 1;
    if (n < 0 || k == 0) return 0;
    return partitions(n - k, k) + partitions(n, k - 1);
}
int main(void) { printf("%d\n", partitions(10, 10)); return 0; }
