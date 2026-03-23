#include <stdio.h>
int main(void) {
    int coeffs[5] = {1, 2, 3, 4, 5};
    int x = 10, result = coeffs[0];
    for (int i = 1; i < 5; i++) result = result * x + coeffs[i];
    printf("%d\n", result);
    return 0;
}
