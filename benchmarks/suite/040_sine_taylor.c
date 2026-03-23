#include <stdio.h>
static double sine_taylor(double x, int terms) {
    double result = 0.0, power = x, factorial = 1.0;
    for (int i = 0; i < terms; i++) {
        int n = 2 * i + 1;
        if (i > 0) { power *= x * x; factorial *= (double)(n - 1) * n; }
        if (i % 2 == 0) result += power / factorial;
        else result -= power / factorial;
    }
    return result;
}
int main(void) { printf("%f\n", sine_taylor(1.0, 15)); return 0; }
