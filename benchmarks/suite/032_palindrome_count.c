#include <stdio.h>
static int reverse_num(int n) {
    int result = 0, x = n;
    while (x > 0) { result = result * 10 + x % 10; x /= 10; }
    return result;
}
int main(void) {
    int count = 0;
    for (int n = 1; n < 1000; n++)
        if (reverse_num(n) == n) count++;
    printf("%d\n", count);
    return 0;
}
