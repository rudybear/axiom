#include <stdio.h>
#include <stdlib.h>

/* Allocate objects of varying sizes (16, 64, 256, 1024 bytes), 100K of each */

int main() {
    int count = 100000;
    long long checksum = 0;

    /* Size class 1: 4 ints (16 bytes) */
    for (int i = 0; i < count; i++) {
        int *p = (int *)malloc(4 * sizeof(int));
        p[0] = i;
        p[3] = i * 7;
        checksum += p[0] + p[3];
        free(p);
    }

    /* Size class 2: 16 ints (64 bytes) */
    for (int i = 0; i < count; i++) {
        int *p = (int *)malloc(16 * sizeof(int));
        p[0] = i;
        p[15] = i * 13;
        checksum += p[0] + p[15];
        free(p);
    }

    /* Size class 3: 64 ints (256 bytes) */
    for (int i = 0; i < count; i++) {
        int *p = (int *)malloc(64 * sizeof(int));
        p[0] = i;
        p[63] = i * 17;
        checksum += p[0] + p[63];
        free(p);
    }

    /* Size class 4: 256 ints (1024 bytes) */
    for (int i = 0; i < count; i++) {
        int *p = (int *)malloc(256 * sizeof(int));
        p[0] = i;
        p[255] = i * 19;
        checksum += p[0] + p[255];
        free(p);
    }

    printf("%lld\n", checksum);
    return 0;
}
