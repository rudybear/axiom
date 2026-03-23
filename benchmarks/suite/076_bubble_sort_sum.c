#include <stdio.h>
int main(void) {
    int arr[20];
    for (int i = 0; i < 20; i++) arr[i] = 20 - i;
    for (int i = 0; i < 19; i++)
        for (int j = 0; j < 19 - i; j++)
            if (arr[j] > arr[j+1]) { int t = arr[j]; arr[j] = arr[j+1]; arr[j+1] = t; }
    int sum = 0;
    for (int i = 0; i < 20; i++) sum += arr[i];
    printf("%d\n", sum);
    return 0;
}
