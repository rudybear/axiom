#include <stdio.h>
static int sum_divisors(int n) {
    int sum = 0;
    for (int i = 1; i <= n; i++)
        if (n % i == 0) sum += i;
    return sum;
}
int main(void) { printf("%d\n", sum_divisors(12)); return 0; }
