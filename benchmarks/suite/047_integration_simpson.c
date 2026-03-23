#include <stdio.h>
int main(void) {
    double a = 0.0, b = 1.0;
    int n = 1000000;
    double dx = (b - a) / (double)n;
    double sum = a * a + b * b;
    for (int i = 1; i < n; i++) {
        double x = a + (double)i * dx;
        if (i % 2 == 0) sum += 2.0 * x * x;
        else sum += 4.0 * x * x;
    }
    printf("%f\n", sum * dx / 3.0);
    return 0;
}
