#include <stdio.h>
#include <stdlib.h>
#include <math.h>

/* N-body with heap-allocated body arrays. 200 bodies, 500 steps. */

int main() {
    int n = 200;
    int steps = 500;
    double dt = 0.01;
    double softening = 0.01;

    double *x = (double *)malloc(n * sizeof(double));
    double *y = (double *)malloc(n * sizeof(double));
    double *z = (double *)malloc(n * sizeof(double));
    double *vx = (double *)malloc(n * sizeof(double));
    double *vy = (double *)malloc(n * sizeof(double));
    double *vz = (double *)malloc(n * sizeof(double));
    double *mass = (double *)malloc(n * sizeof(double));

    long long seed = 42;
    long long lcg_a = 1103515245;
    long long lcg_c = 12345;
    long long lcg_m = 2147483648LL;

    for (int i = 0; i < n; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        x[i] = (double)seed / 2147483648.0 * 100.0 - 50.0;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        y[i] = (double)seed / 2147483648.0 * 100.0 - 50.0;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        z[i] = (double)seed / 2147483648.0 * 100.0 - 50.0;
        vx[i] = 0.0; vy[i] = 0.0; vz[i] = 0.0;
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        mass[i] = (double)seed / 2147483648.0 * 10.0 + 1.0;
    }

    double *ax = (double *)malloc(n * sizeof(double));
    double *ay = (double *)malloc(n * sizeof(double));
    double *az = (double *)malloc(n * sizeof(double));

    for (int step = 0; step < steps; step++) {
        for (int i = 0; i < n; i++) { ax[i] = 0; ay[i] = 0; az[i] = 0; }

        for (int i = 0; i < n; i++) {
            for (int j = i + 1; j < n; j++) {
                double dx = x[j] - x[i];
                double dy = y[j] - y[i];
                double dz = z[j] - z[i];
                double r2 = dx*dx + dy*dy + dz*dz + softening;
                double r = sqrt(r2);
                double inv_r3 = 1.0 / (r * r2);
                ax[i] += dx * mass[j] * inv_r3;
                ay[i] += dy * mass[j] * inv_r3;
                az[i] += dz * mass[j] * inv_r3;
                ax[j] -= dx * mass[i] * inv_r3;
                ay[j] -= dy * mass[i] * inv_r3;
                az[j] -= dz * mass[i] * inv_r3;
            }
        }

        for (int i = 0; i < n; i++) {
            vx[i] += ax[i] * dt;
            vy[i] += ay[i] * dt;
            vz[i] += az[i] * dt;
            x[i] += vx[i] * dt;
            y[i] += vy[i] * dt;
            z[i] += vz[i] * dt;
        }
    }

    double energy = 0;
    for (int i = 0; i < n; i++) {
        double v2 = vx[i]*vx[i] + vy[i]*vy[i] + vz[i]*vz[i];
        energy += 0.5 * mass[i] * v2;
    }

    free(x); free(y); free(z);
    free(vx); free(vy); free(vz);
    free(mass); free(ax); free(ay); free(az);

    long long checksum = (long long)(energy * 1000.0);
    printf("%lld\n", checksum);
    return 0;
}
