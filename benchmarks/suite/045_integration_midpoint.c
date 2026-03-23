#include <stdio.h>
int main(void) {
    double a = 0.0, b = 1.0;
    int n = 1000000;
    double dx = (b - a) / (double)n, sum = 0.0;
    for (int i = 0; i < n; i++) {
        double x = a + ((double)i + 0.5) * dx;
        sum += x * x;
    }
    printf("%f\n", sum * dx);
    return 0;
}
