#include <stdio.h>
int main(void) {
    double sigma = 10.0, rho = 28.0, beta = 8.0/3.0, dt = 0.001;
    double x = 1.0, y = 1.0, z = 1.0;
    for (int i = 0; i < 10000; i++) {
        double dx = sigma * (y - x);
        double dy = x * (rho - z) - y;
        double dz = x * y - beta * z;
        x += dx * dt; y += dy * dt; z += dz * dt;
    }
    printf("%f\n", x);
    return 0;
}
