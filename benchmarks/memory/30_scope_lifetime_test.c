#include <stdio.h>
#include <stdlib.h>

/* Compare heap allocation patterns: explicit malloc vs arena for scoped data */

long long process_batch_arena(int *arena, int *arena_offset, int batch_id, int batch_size) {
    int *keys = arena + *arena_offset; *arena_offset += batch_size;
    int *vals = arena + *arena_offset; *arena_offset += batch_size;
    int *sorted_idx = arena + *arena_offset; *arena_offset += batch_size;
    /* temp */ *arena_offset += batch_size;

    long long seed = (long long)batch_id * 1000 + 42;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    for (int i = 0; i < batch_size; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        keys[i] = (int)(seed % 1000000);
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        vals[i] = (int)(seed % 1000);
        sorted_idx[i] = i;
    }

    for (int i = 0; i < batch_size; i++) {
        int min_idx = i;
        int min_key = keys[sorted_idx[i]];
        for (int j = i + 1; j < batch_size; j++) {
            int k = keys[sorted_idx[j]];
            if (k < min_key) { min_key = k; min_idx = j; }
        }
        if (min_idx != i) {
            int tmp = sorted_idx[i];
            sorted_idx[i] = sorted_idx[min_idx];
            sorted_idx[min_idx] = tmp;
        }
    }

    long long result = 0;
    for (int i = 0; i < batch_size; i++) {
        result += (long long)vals[sorted_idx[i]] * (i + 1);
    }
    return result;
}

long long process_batch_heap(int batch_id, int batch_size) {
    int *keys = (int *)malloc(batch_size * sizeof(int));
    int *vals = (int *)malloc(batch_size * sizeof(int));
    int *sorted_idx = (int *)malloc(batch_size * sizeof(int));
    int *temp = (int *)malloc(batch_size * sizeof(int));

    long long seed = (long long)batch_id * 1000 + 42;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    for (int i = 0; i < batch_size; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        keys[i] = (int)(seed % 1000000);
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        vals[i] = (int)(seed % 1000);
        sorted_idx[i] = i;
    }

    for (int i = 0; i < batch_size; i++) {
        int min_idx = i;
        int min_key = keys[sorted_idx[i]];
        for (int j = i + 1; j < batch_size; j++) {
            int k = keys[sorted_idx[j]];
            if (k < min_key) { min_key = k; min_idx = j; }
        }
        if (min_idx != i) {
            int tmp = sorted_idx[i];
            sorted_idx[i] = sorted_idx[min_idx];
            sorted_idx[min_idx] = tmp;
        }
    }

    long long result = 0;
    for (int i = 0; i < batch_size; i++) {
        result += (long long)vals[sorted_idx[i]] * (i + 1);
    }

    free(keys); free(vals); free(sorted_idx); free(temp);
    return result;
}

int main() {
    int num_batches = 200;
    int batch_size = 500;
    long long checksum = 0;

    /* Phase 1: Arena-based */
    int arena_size = batch_size * 4;
    int *arena = (int *)malloc(arena_size * sizeof(int));
    for (int b = 0; b < num_batches; b++) {
        int offset = 0;
        checksum += process_batch_arena(arena, &offset, b, batch_size);
        /* arena_reset = just reset offset to 0 */
    }
    free(arena);

    /* Phase 2: Heap-based */
    for (int b = 0; b < num_batches; b++) {
        checksum += process_batch_heap(b, batch_size);
    }

    printf("%lld\n", checksum);
    return 0;
}
