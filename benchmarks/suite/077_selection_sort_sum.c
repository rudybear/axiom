#include <stdio.h>
int main(void) {
    int arr[20];
    for (int i = 0; i < 20; i++) arr[i] = 20 - i;
    for (int i = 0; i < 19; i++) {
        int min_idx = i;
        for (int j = i + 1; j < 20; j++)
            if (arr[j] < arr[min_idx]) min_idx = j;
        int t = arr[i]; arr[i] = arr[min_idx]; arr[min_idx] = t;
    }
    int sum = 0;
    for (int i = 0; i < 20; i++) sum += arr[i];
    printf("%d\n", sum);
    return 0;
}
