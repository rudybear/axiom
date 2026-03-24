#include <stdio.h>
#include <stdlib.h>

/* Start with 1 element, realloc doubling 20 times (1->1M). Time the growth. */

int main() {
    long long checksum = 0;

    for (int trial = 0; trial < 50; trial++) {
        int capacity = 1;
        int *data = (int *)malloc(capacity * sizeof(int));
        data[0] = trial;

        for (int step = 0; step < 20; step++) {
            int new_cap = capacity * 2;
            data = (int *)realloc(data, new_cap * sizeof(int));
            for (int i = capacity; i < new_cap; i++) {
                data[i] = i + trial * 7;
            }
            capacity = new_cap;
        }

        long long sum = 0;
        for (int i = 0; i < capacity; i++) {
            sum += data[i];
        }
        checksum += sum;
        free(data);
    }

    printf("%lld\n", checksum);
    return 0;
}
