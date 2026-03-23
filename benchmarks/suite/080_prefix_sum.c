#include <stdio.h>
int main(void) {
    int arr[10];
    for (int i = 0; i < 10; i++) arr[i] = i;
    for (int i = 1; i < 10; i++) arr[i] += arr[i-1];
    printf("%d\n", arr[9]);
    return 0;
}
