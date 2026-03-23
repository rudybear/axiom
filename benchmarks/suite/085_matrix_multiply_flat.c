#include <stdio.h>
int main(void) {
    int a[25], b[25], c[25] = {0};
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) {
            a[i*5+j] = i + j + 1;
            b[i*5+j] = i + j + 1;
        }
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) {
            int sum = 0;
            for (int k = 0; k < 5; k++) sum += a[i*5+k] * b[k*5+j];
            c[i*5+j] = sum;
        }
    int total = 0;
    for (int i = 0; i < 25; i++) total += c[i];
    printf("%d\n", total);
    return 0;
}
