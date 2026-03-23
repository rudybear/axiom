#include <stdio.h>
static int reverse_num(int n) {
    int result = 0, x = n;
    while (x > 0) { result = result * 10 + x % 10; x /= 10; }
    return result;
}
int main(void) { printf("%d\n", reverse_num(12345)); return 0; }
