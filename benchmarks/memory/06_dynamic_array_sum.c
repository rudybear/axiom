#include <stdio.h>
#include <stdlib.h>

/* Allocate large dynamic array (2M i32), fill with computed values, sum, free */

int main() {
    int n = 2000000;
    int *data = (int *)malloc(n * sizeof(int));

    /* Fill with computed values */
    for (int i = 0; i < n; i++) {
        data[i] = (i * 7 + 13) % 100000;
    }

    /* Multiple passes */
    long long total = 0;
    for (int pass = 0; pass < 5; pass++) {
        long long sum = 0;
        for (int i = 0; i < n; i++) {
            sum += data[i];
        }
        total += sum;
    }

    free(data);
    printf("%lld\n", total);
    return 0;
}
