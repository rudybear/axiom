#include <stdio.h>
int main(void) {
    int arr[10];
    for (int i = 0; i < 10; i++) arr[i] = i + 1;
    int product = 1;
    for (int i = 0; i < 10; i++) product *= arr[i];
    printf("%d\n", product);
    return 0;
}
