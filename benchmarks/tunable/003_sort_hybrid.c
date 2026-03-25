#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

static void insertion_sort(int *arr, int lo, int hi) {
    for (int i = lo + 1; i <= hi; i++) {
        int key = arr[i];
        int j = i - 1;
        while (j >= lo && arr[j] > key) {
            arr[j+1] = arr[j];
            j--;
        }
        arr[j+1] = key;
    }
}

static int median_of_three(int *arr, int a, int b, int c) {
    int va = arr[a], vb = arr[b], vc = arr[c];
    if (va <= vb && vb <= vc) return b;
    if (vc <= vb && vb <= va) return b;
    if (vb <= va && va <= vc) return a;
    if (vc <= va && va <= vb) return a;
    return c;
}

static int partition(int *arr, int lo, int hi) {
    int mid = lo + (hi - lo) / 2;
    int pivot_idx = median_of_three(arr, lo, mid, hi);
    int pivot = arr[pivot_idx];
    int tmp = arr[pivot_idx]; arr[pivot_idx] = arr[hi]; arr[hi] = tmp;

    int store = lo;
    for (int j = lo; j < hi; j++) {
        if (arr[j] < pivot) {
            int t = arr[store]; arr[store] = arr[j]; arr[j] = t;
            store++;
        }
    }
    int t2 = arr[store]; arr[store] = arr[hi]; arr[hi] = t2;
    return store;
}

static void quicksort_hybrid(int *arr, int n, int threshold) {
    int stack_lo[128], stack_hi[128];
    int sp = 0;
    stack_lo[0] = 0; stack_hi[0] = n - 1;
    sp = 1;

    while (sp > 0) {
        sp--;
        int lo = stack_lo[sp];
        int hi = stack_hi[sp];

        if (hi - lo < threshold) {
            insertion_sort(arr, lo, hi);
        } else {
            int p = partition(arr, lo, hi);
            if (p - 1 > lo) {
                stack_lo[sp] = lo; stack_hi[sp] = p - 1; sp++;
            }
            if (p + 1 < hi) {
                stack_lo[sp] = p + 1; stack_hi[sp] = hi; sp++;
            }
        }
    }
}

int main(void) {
    int n = 5000000;
    int threshold = 32;
    int *arr = (int *)calloc(n, sizeof(int));

    int64_t seed = 42;
    for (int i = 0; i < n; i++) {
        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        arr[i] = (int)seed;
    }

    quicksort_hybrid(arr, n, threshold);

    int64_t checksum = 0;
    for (int i = 0; i < n / 1000; i++) {
        checksum += (int64_t)arr[i * 1000];
    }
    int sorted_ok = 1;
    for (int i = 0; i < 100; i++) {
        if (arr[i] > arr[i+1]) sorted_ok = 0;
    }

    printf("%lld\n", (long long)checksum);
    printf("%d\n", sorted_ok);

    free(arr);
    return 0;
}
