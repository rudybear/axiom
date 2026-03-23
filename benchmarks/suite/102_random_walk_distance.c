#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
int main(void) {
    int n = 10000;
    int64_t seed = 42, a = 1103515245, c = 12345, m = 2147483648LL;
    int x = 0, y = 0;
    for (int i = 0; i < n; i++) {
        seed = (a * seed + c) % m;
        int dir = (int)(seed % 4);
        if (dir == 0) x++;
        else if (dir == 1) x--;
        else if (dir == 2) y++;
        else y--;
    }
    printf("%d\n", abs(x) + abs(y));
    return 0;
}
