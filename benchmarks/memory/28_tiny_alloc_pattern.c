#include <stdio.h>
#include <stdlib.h>

/* Allocate 1M tiny blocks (4 bytes each). */

int main() {
    int n = 1000000;
    long long checksum = 0;

    /* Phase 1: Arena (bump allocator simulation) */
    int *arena = (int *)malloc(n * sizeof(int));
    for (int i = 0; i < n; i++) {
        arena[i] = i * 3 + 7;
        checksum += arena[i];
    }

    /* Phase 2: Reuse arena */
    for (int i = 0; i < n; i++) {
        arena[i] = i * 5 + 11;
        checksum += arena[i];
    }
    free(arena);

    /* Phase 3: Individual malloc/free */
    for (int i = 0; i < n; i++) {
        int *p = (int *)malloc(sizeof(int));
        *p = i * 7 + 13;
        checksum += *p;
        free(p);
    }

    printf("%lld\n", checksum);
    return 0;
}
