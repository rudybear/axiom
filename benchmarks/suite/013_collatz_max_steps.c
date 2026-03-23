#include <stdio.h>
#include <stdint.h>
static int collatz_steps(int64_t n) {
    int64_t x = n;
    int steps = 0;
    while (x != 1) {
        if (x % 2 == 0) x /= 2;
        else x = 3 * x + 1;
        steps++;
    }
    return steps;
}
int main(void) {
    int max_steps = 0;
    for (int i = 1; i <= 1000; i++) {
        int s = collatz_steps((int64_t)i);
        if (s > max_steps) max_steps = s;
    }
    printf("%d\n", max_steps);
    return 0;
}
