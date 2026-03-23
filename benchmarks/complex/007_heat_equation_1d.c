#include <stdio.h>
#include <math.h>

int main(void) {
    static double u[10000], u_new[10000];
    int n = 10000, steps = 5000;
    double alpha = 0.4;

    for (int i = 0; i < 10000; i++) {
        double x = (double)i / 10000.0;
        double center = x - 0.5;
        u[i] = 100.0 * exp(-200.0 * center * center);
    }

    for (int step = 0; step < steps; step++) {
        u_new[0] = 0.0;
        u_new[9999] = 0.0;
        for (int i = 1; i < 9999; i++)
            u_new[i] = u[i] + alpha * (u[i-1] - 2.0*u[i] + u[i+1]);
        for (int i = 0; i < 10000; i++) u[i] = u_new[i];
    }

    double checksum = 0.0;
    for (int i = 0; i < 10000; i++) checksum += u[i];
    printf("%f\n", checksum);
    return 0;
}
