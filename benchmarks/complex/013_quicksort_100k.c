#include <stdio.h>
#include <stdint.h>

int main(void) {
    static int arr[100000];
    int n = 100000;

    int64_t seed = 42, lcg_a = 1103515245, lcg_c = 12345, lcg_m = 2147483648LL;
    for (int i = 0; i < 100000; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        arr[i] = (int)(seed % 1000000);
    }

    int stack_lo[200], stack_hi[200];
    int top = 0;
    stack_lo[0] = 0; stack_hi[0] = n - 1; top = 1;

    while (top > 0) {
        top--;
        int lo = stack_lo[top], hi = stack_hi[top];
        if (lo < hi) {
            int pivot = arr[hi], i = lo - 1;
            for (int j = lo; j < hi; j++) {
                if (arr[j] <= pivot) {
                    i++;
                    int tmp = arr[i]; arr[i] = arr[j]; arr[j] = tmp;
                }
            }
            i++;
            int tmp = arr[i]; arr[i] = arr[hi]; arr[hi] = tmp;
            int pi_idx = i;
            if (pi_idx - 1 > lo) { stack_lo[top] = lo; stack_hi[top] = pi_idx-1; top++; }
            if (pi_idx + 1 < hi) { stack_lo[top] = pi_idx+1; stack_hi[top] = hi; top++; }
        }
    }

    int64_t checksum = 0;
    for (int i = 0; i < 100; i++) checksum += arr[i];
    for (int i = 99900; i < 100000; i++) checksum += arr[i];
    for (int i = 49950; i < 50050; i++) checksum += arr[i];
    printf("%lld\n", (long long)checksum);
    return 0;
}
