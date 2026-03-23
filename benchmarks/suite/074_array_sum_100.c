#include <stdio.h>
int main(void) {
    int arr[100] = {0};
    for (int i = 0; i < 100; i++) arr[i] = i;
    int sum = 0;
    for (int i = 0; i < 100; i++) sum += arr[i];
    printf("%d\n", sum);
    return 0;
}
