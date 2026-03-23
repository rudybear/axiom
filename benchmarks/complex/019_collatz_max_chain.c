#include <stdio.h>
#include <stdint.h>

int collatz_len(int64_t start) {
    int64_t n = start;
    int steps = 0;
    while (n != 1) {
        if (n % 2 == 0) n /= 2;
        else n = 3*n + 1;
        steps++;
    }
    return steps;
}

int main(void) {
    int max_len = 0, max_start = 1;
    for (int i = 1; i <= 1000000; i++) {
        int len = collatz_len((int64_t)i);
        if (len > max_len) { max_len = len; max_start = i; }
    }
    printf("%d\n%d\n", max_start, max_len);
    return 0;
}
