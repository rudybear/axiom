#include <stdio.h>
static int min_val(int a, int b) { return a < b ? a : b; }
static int max_val(int a, int b) { return a > b ? a : b; }
int main(void) {
    int a = min_val(3, 7), b = max_val(3, 7);
    printf("%d\n", a + b - min_val(a, b));
    return 0;
}
