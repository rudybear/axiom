#include <stdio.h>
static int catalan(int n) {
    if (n <= 1) return 1;
    int sum = 0;
    for (int i = 0; i < n; i++) sum += catalan(i) * catalan(n - 1 - i);
    return sum;
}
int main(void) { printf("%d\n", catalan(5)); return 0; }
