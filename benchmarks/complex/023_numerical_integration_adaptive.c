#include <stdio.h>
#include <math.h>

double func(double x) {
    double arg = 100.0 * x;
    double pi2 = 6.28318530717959;
    double a = arg - (double)((int)(arg / pi2)) * pi2;
    double a2 = a * a;
    double s = a - a*a2/6.0 + a*a2*a2/120.0 - a*a2*a2*a2/5040.0 + a*a2*a2*a2*a2/362880.0;
    return x * x * s;
}

double simpson(double a, double b) {
    double mid = (a + b) / 2.0;
    return (b - a) / 6.0 * (func(a) + 4.0*func(mid) + func(b));
}

int main(void) {
    double a = 0.01, b = 10.0;
    int n = 500000;
    double h = (b - a) / (double)n;
    double total = 0.0;

    for (int i = 0; i < n; i++) {
        double x0 = a + (double)i * h;
        total += simpson(x0, x0 + h);
    }

    printf("%f\n", total);
    return 0;
}
