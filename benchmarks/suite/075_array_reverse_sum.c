#include <stdio.h>
int main(void) {
    int arr[100], rev[100];
    for (int i = 0; i < 100; i++) arr[i] = i;
    for (int i = 0; i < 100; i++) rev[i] = arr[99 - i];
    int sum = 0;
    for (int i = 0; i < 100; i++) sum += rev[i];
    printf("%d\n", sum);
    return 0;
}
