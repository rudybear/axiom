#include <stdio.h>
#include <stdlib.h>

static int flip_count(int *perm, int n, int *tmp) {
    for (int i = 0; i < n; i++) tmp[i] = perm[i];
    int flips = 0;
    int first = tmp[0];
    while (first != 0) {
        int lo = 0, hi = first;
        while (lo < hi) {
            int t = tmp[lo]; tmp[lo] = tmp[hi]; tmp[hi] = t;
            lo++; hi--;
        }
        flips++;
        first = tmp[0];
    }
    return flips;
}

int main(void) {
    int n = 10;
    int *perm = (int *)calloc(n, sizeof(int));
    int *count = (int *)calloc(n, sizeof(int));
    int *tmp = (int *)calloc(n, sizeof(int));

    for (int i = 0; i < n; i++) {
        perm[i] = i;
        count[i] = i + 1;
    }

    int max_flips = 0;
    int checksum = 0;
    int perm_count = 0;
    int done = 0;

    while (!done) {
        int flips = flip_count(perm, n, tmp);
        if (flips > max_flips) max_flips = flips;
        if (perm_count % 2 == 0) checksum += flips;
        else checksum -= flips;
        perm_count++;

        /* Generate next permutation */
        int i = 1;
        int saved = perm[0];
        for (int r = 0; r < i; r++) perm[r] = perm[r+1];
        perm[i] = saved;
        count[i]--;

        while (count[i] <= 0) {
            count[i] = i + 1;
            i++;
            if (i >= n) { done = 1; break; }
            saved = perm[0];
            for (int r = 0; r < i; r++) perm[r] = perm[r+1];
            perm[i] = saved;
            count[i]--;
        }
    }

    printf("%d\n", checksum);
    printf("%d\n", max_flips);

    free(perm); free(count); free(tmp);
    return 0;
}
