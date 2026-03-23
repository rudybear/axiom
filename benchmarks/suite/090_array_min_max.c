#include <stdio.h>
int main(void) {
    int arr[100];
    for (int i = 0; i < 100; i++) arr[i] = (i * 37 + 13) % 100;
    int mn = arr[0], mx = arr[0];
    for (int i = 1; i < 100; i++) {
        if (arr[i] < mn) mn = arr[i];
        if (arr[i] > mx) mx = arr[i];
    }
    printf("%d\n", mx - mn);
    return 0;
}
