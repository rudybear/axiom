#include <stdio.h>
#include <stdint.h>
int main(void) {
    int x = 42;
    int64_t y = (int64_t)x;
    int z = (int)y;
    printf("%d\n", z);
    return 0;
}
