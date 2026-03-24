#include <stdio.h>
#include <stdlib.h>

/* Pure allocation throughput: 1M allocations comparing approaches */

int main() {
    int n = 1000000;
    int obj_size = 4;
    long long checksum = 0;

    /* Phase 1: Single large allocation (arena simulation) */
    int *arena = (int *)malloc(n * obj_size * sizeof(int));
    int arena_offset = 0;
    for (int i = 0; i < n; i++) {
        int *p = arena + arena_offset;
        arena_offset += obj_size;
        p[0] = i;
        checksum += p[0];
    }

    /* Phase 2: Reset and reuse */
    arena_offset = 0;
    for (int i = 0; i < n; i++) {
        int *p = arena + arena_offset;
        arena_offset += obj_size;
        p[0] = i * 3;
        checksum += p[0];
    }
    free(arena);

    /* Phase 3: Individual malloc/free */
    for (int i = 0; i < n; i++) {
        int *p = (int *)malloc(obj_size * sizeof(int));
        p[0] = i * 5;
        checksum += p[0];
        free(p);
    }

    printf("%lld\n", checksum);
    return 0;
}
