#include <stdio.h>
static int triangular(int n) {
    int sum = 0;
    for (int i = 1; i <= n; i++) sum += i;
    return sum;
}
int main(void) { printf("%d\n", triangular(100)); return 0; }
