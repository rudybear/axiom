#include <stdio.h>
int main(void) {
    int arr[100];
    for (int i = 0; i < 100; i++) arr[i] = i * 2;
    int target = 84, lo = 0, hi = 99, result = -1;
    while (lo <= hi) {
        int mid = lo + (hi - lo) / 2;
        if (arr[mid] == target) { result = mid; break; }
        else if (arr[mid] < target) lo = mid + 1;
        else hi = mid - 1;
    }
    printf("%d\n", result);
    return 0;
}
