#include <stdio.h>
int main(void) {
    int arr[10] = {-2, 1, -3, 4, -1, 2, 1, -5, 4, 1};
    int max_ending = arr[0], max_so_far = arr[0];
    for (int i = 1; i < 10; i++) {
        max_ending = max_ending + arr[i] > arr[i] ? max_ending + arr[i] : arr[i];
        if (max_ending > max_so_far) max_so_far = max_ending;
    }
    printf("%d\n", max_so_far);
    return 0;
}
