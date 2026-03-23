#include <stdio.h>
static int digit_sum(int n) {
    int sum = 0, x = n;
    while (x > 0) { sum += x % 10; x /= 10; }
    return sum;
}
int main(void) {
    int count = 0;
    for (int n = 1; n <= 50; n++)
        if (n % digit_sum(n) == 0) count++;
    printf("%d\n", count);
    return 0;
}
