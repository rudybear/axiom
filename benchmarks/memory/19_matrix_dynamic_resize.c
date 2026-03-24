#include <stdio.h>
#include <stdlib.h>

/* Matrix that grows dynamically (realloc-like pattern), fill and compute trace + sum */

int main() {
    int n = 4;
    int *data = (int *)malloc(n * n * sizeof(int));

    for (int i = 0; i < n; i++)
        for (int j = 0; j < n; j++)
            data[i * n + j] = i + j + 1;

    while (n < 512) {
        int new_n = n * 2;
        int *new_data = (int *)malloc(new_n * new_n * sizeof(int));

        for (int i = 0; i < n; i++)
            for (int j = 0; j < n; j++)
                new_data[i * new_n + j] = data[i * n + j];

        for (int i = 0; i < n; i++)
            for (int j = n; j < new_n; j++)
                new_data[i * new_n + j] = (i + j + 1) % 100;

        for (int i = n; i < new_n; i++)
            for (int j = 0; j < new_n; j++)
                new_data[i * new_n + j] = (i + j + 1) % 100;

        free(data);
        data = new_data;
        n = new_n;
    }

    long long trace = 0;
    long long total = 0;
    for (int i = 0; i < n; i++) {
        trace += data[i * n + i];
        for (int j = 0; j < n; j++)
            total += data[i * n + j];
    }

    free(data);

    long long checksum = trace * 1000 + total;
    printf("%lld\n", checksum);
    return 0;
}
