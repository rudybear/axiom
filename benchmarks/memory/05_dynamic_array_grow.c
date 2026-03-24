#include <stdio.h>
#include <stdlib.h>

/* Dynamic array: start capacity 16, double when full, insert 1M elements */

int main() {
    int n = 1000000;
    int capacity = 16;
    int size = 0;
    int *data = (int *)malloc(capacity * sizeof(int));

    long long seed = 12345;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    for (int i = 0; i < n; i++) {
        if (size >= capacity) {
            capacity *= 2;
            data = (int *)realloc(data, capacity * sizeof(int));
        }
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        data[size] = (int)(seed % 1000000);
        size++;
    }

    long long sum = 0;
    for (int i = 0; i < size; i++) {
        sum += data[i];
    }

    free(data);

    long long checksum = sum + size + capacity;
    printf("%lld\n", checksum);
    return 0;
}
