#include <stdio.h>
static int is_perfect(int n) {
    int sum = 0;
    for (int i = 1; i < n; i++)
        if (n % i == 0) sum += i;
    return sum == n ? 1 : 0;
}
int main(void) {
    int count = 0;
    for (int n = 2; n < 8129; n++) count += is_perfect(n);
    printf("%d\n", count);
    return 0;
}
