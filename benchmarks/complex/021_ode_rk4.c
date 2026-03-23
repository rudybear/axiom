#include <stdio.h>

double f_ode(double x, double y) {
    return -2.0 * x * y;
}

int main(void) {
    int n = 1000000;
    double x = 0.0, y = 1.0, h = 5.0 / (double)n;

    for (int i = 0; i < n; i++) {
        double k1 = h * f_ode(x, y);
        double k2 = h * f_ode(x + h/2.0, y + k1/2.0);
        double k3 = h * f_ode(x + h/2.0, y + k2/2.0);
        double k4 = h * f_ode(x + h, y + k3);
        y += (k1 + 2.0*k2 + 2.0*k3 + k4) / 6.0;
        x += h;
    }

    printf("%.15e\n", y);
    return 0;
}
