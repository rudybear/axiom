#include <stdio.h>
#include <math.h>

int main(void) {
    static double u_prev[10000], u_curr[10000], u_next[10000];
    int n = 10000, steps = 5000;
    double c2 = 0.2;

    for (int i = 0; i < 10000; i++) {
        double x = (double)i / 10000.0;
        double center = x - 0.5;
        u_curr[i] = exp(-500.0 * center * center);
        u_prev[i] = u_curr[i];
    }

    for (int step = 0; step < steps; step++) {
        u_next[0] = 0.0;
        u_next[9999] = 0.0;
        for (int i = 1; i < 9999; i++)
            u_next[i] = 2.0*u_curr[i] - u_prev[i] + c2*(u_curr[i-1] - 2.0*u_curr[i] + u_curr[i+1]);
        for (int i = 0; i < 10000; i++) {
            u_prev[i] = u_curr[i];
            u_curr[i] = u_next[i];
        }
    }

    double checksum = 0.0;
    for (int i = 0; i < 10000; i++) checksum += fabs(u_curr[i]);
    printf("%f\n", checksum);
    return 0;
}
