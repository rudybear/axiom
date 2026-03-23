#include <stdio.h>
static int sum_proper_divisors(int n) {
    int sum = 0;
    for (int i = 1; i < n; i++)
        if (n % i == 0) sum += i;
    return sum;
}
int main(void) {
    int count = 0;
    for (int n = 2; n <= 1000; n++)
        if (sum_proper_divisors(n) > n) count++;
    printf("%d\n", count);
    return 0;
}
