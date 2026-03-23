#include <stdio.h>
int main(void) {
    int coeffs[5] = {5, 4, 3, 2, 1};
    int x = 10, result = 0;
    for (int i = 0; i < 5; i++) {
        int power = 1;
        for (int j = 0; j < i; j++) power *= x;
        result += coeffs[i] * power;
    }
    printf("%d\n", result);
    return 0;
}
