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
int main(void) { printf("%d\n", collatz_steps(871)); return 0; }
