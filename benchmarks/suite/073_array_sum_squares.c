#include <stdio.h>
int main(void) {
    int arr[10] = {0};
    for (int i = 0; i < 10; i++) arr[i] = i * i;
    int sum = 0;
    for (int i = 0; i < 10; i++) sum += arr[i];
    printf("%d\n", sum);
    return 0;
}
