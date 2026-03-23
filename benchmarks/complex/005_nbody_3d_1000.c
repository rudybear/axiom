#include <stdio.h>
#include <math.h>
#include <stdint.h>

int main(void) {
    int nb = 100, steps = 1000;
    double dt = 0.001, softening = 0.01;
    double px[100], py[100], pz[100];
    double vx[100], vy[100], vz[100];
    double mass[100], fx[100], fy[100], fz[100];

    int64_t seed = 42, lcg_a = 1103515245, lcg_c = 12345, lcg_m = 2147483648LL;
    for (int i = 0; i < 100; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        px[i] = (double)seed / (double)lcg_m * 10.0 - 5.0;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        py[i] = (double)seed / (double)lcg_m * 10.0 - 5.0;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        pz[i] = (double)seed / (double)lcg_m * 10.0 - 5.0;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        vx[i] = (double)seed / (double)lcg_m * 0.2 - 0.1;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        vy[i] = (double)seed / (double)lcg_m * 0.2 - 0.1;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        vz[i] = (double)seed / (double)lcg_m * 0.2 - 0.1;
        mass[i] = 1.0 + (double)(i % 10) * 0.5;
    }

    for (int step = 0; step < steps; step++) {
        for (int i = 0; i < 100; i++) { fx[i] = fy[i] = fz[i] = 0.0; }
        for (int i = 0; i < 100; i++) {
            for (int j = i + 1; j < 100; j++) {
                double dx = px[j] - px[i], dy = py[j] - py[i], dz = pz[j] - pz[i];
                double dist_sq = dx*dx + dy*dy + dz*dz + softening;
                double dist = sqrt(dist_sq);
                double force = mass[i] * mass[j] / (dist_sq * dist);
                double ffx = force*dx, ffy = force*dy, ffz = force*dz;
                fx[i] += ffx; fy[i] += ffy; fz[i] += ffz;
                fx[j] -= ffx; fy[j] -= ffy; fz[j] -= ffz;
            }
        }
        for (int i = 0; i < 100; i++) {
            vx[i] += fx[i]/mass[i]*dt; vy[i] += fy[i]/mass[i]*dt; vz[i] += fz[i]/mass[i]*dt;
            px[i] += vx[i]*dt; py[i] += vy[i]*dt; pz[i] += vz[i]*dt;
        }
    }

    double ke = 0.0;
    for (int i = 0; i < 100; i++)
        ke += 0.5 * mass[i] * (vx[i]*vx[i] + vy[i]*vy[i] + vz[i]*vz[i]);
    printf("%f\n", ke);
    return 0;
}
