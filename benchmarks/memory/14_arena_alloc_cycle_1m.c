#include <stdio.h>
#include <stdlib.h>

/* Arena simulation: allocate 1M small objects in batches, free batch at once */

int main() {
    int n = 1000000;
    int batch_size = 10000;
    int num_batches = n / batch_size;
    long long checksum = 0;

    for (int batch = 0; batch < num_batches; batch++) {
        /* Simulate arena: single large allocation for the batch */
        int *arena_mem = (int *)malloc(batch_size * 4 * sizeof(int));
        for (int j = 0; j < batch_size; j++) {
            int idx = batch * batch_size + j;
            int *p = arena_mem + j * 4;
            p[0] = idx;
            p[1] = idx * 2;
            p[2] = idx * 3;
            p[3] = idx * 4;
            checksum += p[0] + p[1] + p[2] + p[3];
        }
        free(arena_mem);
    }

    printf("%lld\n", checksum);
    return 0;
}
