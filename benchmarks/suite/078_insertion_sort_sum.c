#include <stdio.h>
int main(void) {
    int arr[20];
    for (int i = 0; i < 20; i++) arr[i] = 20 - i;
    for (int i = 1; i < 20; i++) {
        int key = arr[i], j = i - 1;
        while (j >= 0 && arr[j] > key) { arr[j+1] = arr[j]; j--; }
        arr[j+1] = key;
    }
    int sum = 0;
    for (int i = 0; i < 20; i++) sum += arr[i];
    printf("%d\n", sum);
    return 0;
}
