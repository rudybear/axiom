#include <stdio.h>
static int depth(int n) {
    if (n <= 1) return 0;
    int left = depth(n / 2);
    int right = depth(n - n / 2 - 1);
    return (left > right ? left : right) + 1;
}
int main(void) { printf("%d\n", depth(1000000)); return 0; }
