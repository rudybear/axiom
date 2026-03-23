#include <stdio.h>
static int power(int base, int exp) {
    if (exp == 0) return 1;
    return base * power(base, exp - 1);
}
int main(void) { printf("%d\n", power(3, 10)); return 0; }
