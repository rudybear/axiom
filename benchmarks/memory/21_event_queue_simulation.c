#include <stdio.h>
#include <stdlib.h>

/* Priority queue (heap-allocated array), 100K insert/extract-min operations */

static long long *heap_arr;
static int heap_size;

void heap_push(long long priority) {
    heap_arr[heap_size] = priority;
    int i = heap_size;
    heap_size++;
    while (i > 0) {
        int parent = (i - 1) / 2;
        if (heap_arr[i] < heap_arr[parent]) {
            long long tmp = heap_arr[i];
            heap_arr[i] = heap_arr[parent];
            heap_arr[parent] = tmp;
            i = parent;
        } else {
            break;
        }
    }
}

long long heap_pop() {
    if (heap_size == 0) return -1;
    long long result = heap_arr[0];
    heap_size--;
    heap_arr[0] = heap_arr[heap_size];

    int i = 0;
    while (1) {
        int left = 2 * i + 1;
        int right = 2 * i + 2;
        int smallest = i;
        if (left < heap_size && heap_arr[left] < heap_arr[smallest]) smallest = left;
        if (right < heap_size && heap_arr[right] < heap_arr[smallest]) smallest = right;
        if (smallest != i) {
            long long tmp = heap_arr[i];
            heap_arr[i] = heap_arr[smallest];
            heap_arr[smallest] = tmp;
            i = smallest;
        } else break;
    }
    return result;
}

int main() {
    int max_size = 200000;
    heap_arr = (long long *)malloc(max_size * sizeof(long long));
    heap_size = 0;

    long long seed = 42;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    long long checksum = 0;
    int ops = 100000;

    for (int i = 0; i < ops; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        long long priority = seed % 10000000;
        heap_push(priority);

        if (i % 3 == 0) {
            checksum += heap_pop();
        }
    }

    while (heap_size > 0) {
        checksum += heap_pop();
    }

    free(heap_arr);

    printf("%lld\n", checksum);
    return 0;
}
