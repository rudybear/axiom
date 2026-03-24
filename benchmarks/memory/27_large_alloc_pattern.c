#include <stdio.h>
#include <stdlib.h>

/* Allocate a few very large blocks (1MB each), fill, checksum, free */

int main() {
    int block_count = 8;
    int block_elems = 262144;
    long long checksum = 0;

    for (int round = 0; round < 5; round++) {
        for (int b = 0; b < block_count; b++) {
            int *block = (int *)malloc(block_elems * sizeof(int));

            for (int i = 0; i < block_elems; i++) {
                block[i] = (i * 7 + b * 13 + round * 31) % 1000000;
            }

            long long sum = 0;
            for (int i = 0; i < block_elems; i++) {
                sum += block[i];
            }

            checksum += sum;
            free(block);
        }
    }

    printf("%lld\n", checksum);
    return 0;
}
