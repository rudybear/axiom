#include <stdio.h>
static int digit_sum(int n) {
    int sum = 0, x = n;
    while (x > 0) { sum += x % 10; x /= 10; }
    return sum;
}
int main(void) { printf("%d\n", digit_sum(123456789)); return 0; }
