#include <stdio.h>
int main(void) {
    double product = 1.0;
    for (int i = 1; i <= 1000000; i++) {
        double n = (double)i;
        product *= 4.0 * n * n / (4.0 * n * n - 1.0);
    }
    printf("%f\n", 2.0 * product);
    return 0;
}
