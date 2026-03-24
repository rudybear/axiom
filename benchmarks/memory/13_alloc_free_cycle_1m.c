#include <stdio.h>
#include <stdlib.h>

/* Allocate and free 1M small objects (16 bytes = 4 int) one at a time */

int main() {
    int n = 1000000;
    long long checksum = 0;

    for (int i = 0; i < n; i++) {
        int *p = (int *)malloc(4 * sizeof(int));
        p[0] = i;
        p[1] = i * 2;
        p[2] = i * 3;
        p[3] = i * 4;
        checksum += p[0] + p[1] + p[2] + p[3];
        free(p);
    }

    printf("%lld\n", checksum);
    return 0;
}
