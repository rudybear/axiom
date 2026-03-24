#include <stdio.h>
#include <stdlib.h>

/* Allocate 10K blocks sequentially, then free in reverse order (LIFO) */

int main() {
    int n = 10000;
    int block_size = 256;
    long long checksum = 0;

    /* Pool-based rounds */
    for (int round = 0; round < 20; round++) {
        int *pool = (int *)malloc(n * block_size * sizeof(int));

        for (int i = 0; i < n; i++) {
            int base = i * block_size;
            pool[base] = i + round * n;
            pool[base + block_size - 1] = i * 7 + round;
        }

        for (int i = 0; i < n; i++) {
            int base = i * block_size;
            checksum += pool[base];
            checksum += pool[base + block_size - 1];
        }

        free(pool);
    }

    /* Individual alloc/free rounds */
    for (int round = 0; round < 50; round++) {
        long long data_check = 0;
        int *p1 = (int *)malloc(block_size * sizeof(int));
        int *p2 = (int *)malloc(block_size * sizeof(int));
        int *p3 = (int *)malloc(block_size * sizeof(int));
        int *p4 = (int *)malloc(block_size * sizeof(int));
        int *p5 = (int *)malloc(block_size * sizeof(int));

        for (int j = 0; j < block_size; j++) {
            p1[j] = j + round;
            p2[j] = j * 2 + round;
            p3[j] = j * 3 + round;
            p4[j] = j * 4 + round;
            p5[j] = j * 5 + round;
        }

        for (int j = 0; j < block_size; j++) {
            data_check += p1[j] + p2[j] + p3[j] + p4[j] + p5[j];
        }

        free(p5); free(p4); free(p3); free(p2); free(p1);
        checksum += data_check;
    }

    printf("%lld\n", checksum);
    return 0;
}
