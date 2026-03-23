#include <stdio.h>
int main(void) {
    double sum = 0.0;
    for (int i = 1; i <= 100000; i++) {
        double n = (double)i;
        sum += 1.0 / (n * n);
    }
    printf("%f\n", sum);
    return 0;
}
