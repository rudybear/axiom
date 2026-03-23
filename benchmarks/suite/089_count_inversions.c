#include <stdio.h>
int main(void) {
    int arr[20];
    for (int i = 0; i < 20; i++) arr[i] = 20 - i;
    int count = 0;
    for (int i = 0; i < 19; i++)
        for (int j = i + 1; j < 20; j++)
            if (arr[i] > arr[j]) count++;
    printf("%d\n", count);
    return 0;
}
