#include <stdio.h>
#include <stdint.h>

void sift_down(int *arr, int start, int end_idx) {
    int root = start;
    while (2*root + 1 <= end_idx) {
        int child = 2*root + 1, swap_idx = root;
        if (arr[swap_idx] < arr[child]) swap_idx = child;
        if (child+1 <= end_idx && arr[swap_idx] < arr[child+1]) swap_idx = child+1;
        if (swap_idx == root) return;
        int tmp = arr[root]; arr[root] = arr[swap_idx]; arr[swap_idx] = tmp;
        root = swap_idx;
    }
}

int main(void) {
    static int arr[100000];
    int n = 100000;

    int64_t seed = 67890, lcg_a = 1103515245, lcg_c = 12345, lcg_m = 2147483648LL;
    for (int i = 0; i < 100000; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        arr[i] = (int)(seed % 1000000);
    }

    for (int start = (n-2)/2; start >= 0; start--)
        sift_down(arr, start, n-1);

    for (int end_idx = n-1; end_idx > 0; end_idx--) {
        int tmp = arr[0]; arr[0] = arr[end_idx]; arr[end_idx] = tmp;
        sift_down(arr, 0, end_idx-1);
    }

    int64_t checksum = 0;
    for (int i = 0; i < 100; i++) checksum += arr[i];
    for (int i = 99900; i < 100000; i++) checksum += arr[i];
    for (int i = 49950; i < 50050; i++) checksum += arr[i];
    printf("%lld\n", (long long)checksum);
    return 0;
}
