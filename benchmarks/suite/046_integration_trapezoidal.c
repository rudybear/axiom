#include <stdio.h>
int main(void) {
    double a = 0.0, b = 1.0;
    int n = 1000000;
    double dx = (b - a) / (double)n;
    double sum = a * a / 2.0 + b * b / 2.0;
    for (int i = 1; i < n; i++) {
        double x = a + (double)i * dx;
        sum += x * x;
    }
    printf("%f\n", sum * dx);
    return 0;
}
