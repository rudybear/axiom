#include <stdio.h>
static int is_prime(int n) {
    if (n < 2) return 0;
    for (int i = 2; i * i <= n; i++)
        if (n % i == 0) return 0;
    return 1;
}
int main(void) { printf("%d\n", is_prime(104729)); return 0; }
