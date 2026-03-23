#include <stdio.h>
#include <math.h>

int main(void) {
    int np = 500, steps = 5000;
    double dt = 0.0005, k_spring = 100.0, rest_len = 0.1, damping = 0.999;
    double x[500], y[500], old_x[500], old_y[500], fx[500], fy[500];

    for (int i = 0; i < 500; i++) {
        x[i] = (double)i * rest_len;
        y[i] = (double)(i % 20) * 0.01;
        old_x[i] = x[i];
        old_y[i] = y[i];
    }

    for (int step = 0; step < steps; step++) {
        for (int i = 0; i < 500; i++) { fx[i] = 0.0; fy[i] = -9.81; }

        for (int i = 0; i < 499; i++) {
            double dx = x[i+1] - x[i], dy = y[i+1] - y[i];
            double dist = sqrt(dx*dx + dy*dy + 0.0001);
            double stretch = dist - rest_len;
            double force = k_spring * stretch / dist;
            double ffx = force*dx, ffy = force*dy;
            fx[i] += ffx; fy[i] += ffy;
            fx[i+1] -= ffx; fy[i+1] -= ffy;
        }

        for (int i = 0; i + 10 < 500; i += 10) {
            double dx = x[i+10] - x[i], dy = y[i+10] - y[i];
            double dist = sqrt(dx*dx + dy*dy + 0.0001);
            double rl = rest_len * 10.0;
            double stretch = dist - rl;
            double force = k_spring * 0.5 * stretch / dist;
            double ffx = force*dx, ffy = force*dy;
            fx[i] += ffx; fy[i] += ffy;
            fx[i+10] -= ffx; fy[i+10] -= ffy;
        }

        for (int i = 1; i < 500; i++) {
            double new_x = 2.0*x[i] - old_x[i] + fx[i]*dt*dt;
            double new_y = 2.0*y[i] - old_y[i] + fy[i]*dt*dt;
            old_x[i] = x[i]; old_y[i] = y[i];
            x[i] = new_x * damping; y[i] = new_y * damping;
        }
    }

    double checksum = 0.0;
    for (int i = 0; i < 500; i++) checksum += x[i] + y[i];
    printf("%f\n", checksum);
    return 0;
}
