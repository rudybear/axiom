#include <stdio.h>
int main(void) {
    int arr[10] = {2, 7, 11, 15, 1, 8, 3, 4, 5, 6};
    int target = 16, result = -1;
    for (int i = 0; i < 9; i++)
        for (int j = i + 1; j < 10; j++)
            if (arr[i] + arr[j] == target) result = i + j;
    printf("%d\n", result);
    return 0;
}
