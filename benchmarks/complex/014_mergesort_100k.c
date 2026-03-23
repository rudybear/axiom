#include <stdio.h>
#include <stdint.h>

static inline int min_i(int a, int b) { return a < b ? a : b; }

int main(void) {
    static int arr[100000], tmp[100000];
    int n = 100000;

    int64_t seed = 12345, lcg_a = 1103515245, lcg_c = 12345, lcg_m = 2147483648LL;
    for (int i = 0; i < 100000; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        arr[i] = (int)(seed % 1000000);
    }

    for (int width = 1; width < n; width *= 2) {
        for (int i = 0; i < n; i += 2*width) {
            int left = i, mid = min_i(i+width, n), right = min_i(i+2*width, n);
            int l = left, r = mid, k = left;
            while (l < mid && r < right) {
                if (arr[l] <= arr[r]) tmp[k++] = arr[l++];
                else tmp[k++] = arr[r++];
            }
            while (l < mid) tmp[k++] = arr[l++];
            while (r < right) tmp[k++] = arr[r++];
        }
        for (int j = 0; j < n; j++) arr[j] = tmp[j];
    }

    int64_t checksum = 0;
    for (int i = 0; i < 100; i++) checksum += arr[i];
    for (int i = 99900; i < 100000; i++) checksum += arr[i];
    for (int i = 49950; i < 50050; i++) checksum += arr[i];
    printf("%lld\n", (long long)checksum);
    return 0;
}
