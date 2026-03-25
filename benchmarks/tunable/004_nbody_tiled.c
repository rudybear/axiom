#include <stdio.h>
#include <math.h>
#include <stdint.h>
#include <stdlib.h>

static void compute_forces_batched(double *x, double *y, double *z,
                                   double *fx, double *fy, double *fz,
                                   double *mass, int n, int batch, double softening) {
    for (int i = 0; i < n; i++) { fx[i] = 0; fy[i] = 0; fz[i] = 0; }

    int n_batches = (n + batch - 1) / batch;
    for (int bi = 0; bi < n_batches; bi++) {
        int i_start = bi * batch;
        int i_end = i_start + batch; if (i_end > n) i_end = n;

        for (int bj = bi; bj < n_batches; bj++) {
            int j_start = bj * batch;
            int j_end = j_start + batch; if (j_end > n) j_end = n;

            for (int i = i_start; i < i_end; i++) {
                double xi = x[i], yi = y[i], zi = z[i];
                double fxi = fx[i], fyi = fy[i], fzi = fz[i];

                int j_lo = j_start;
                if (bi == bj) j_lo = i + 1;

                for (int j = j_lo; j < j_end; j++) {
                    double dx = x[j] - xi;
                    double dy = y[j] - yi;
                    double dz = z[j] - zi;
                    double dist2 = dx*dx + dy*dy + dz*dz + softening;
                    double inv_dist = 1.0 / sqrt(dist2);
                    double inv_dist3 = inv_dist * inv_dist * inv_dist;

                    fxi += dx * mass[j] * inv_dist3;
                    fyi += dy * mass[j] * inv_dist3;
                    fzi += dz * mass[j] * inv_dist3;

                    fx[j] -= dx * mass[i] * inv_dist3;
                    fy[j] -= dy * mass[i] * inv_dist3;
                    fz[j] -= dz * mass[i] * inv_dist3;
                }
                fx[i] = fxi; fy[i] = fyi; fz[i] = fzi;
            }
        }
    }
}

static void integrate(double *x, double *y, double *z,
                      double *vx, double *vy, double *vz,
                      double *fx, double *fy, double *fz,
                      double *mass, int n, double dt) {
    for (int i = 0; i < n; i++) {
        double ax = fx[i]/mass[i], ay = fy[i]/mass[i], az = fz[i]/mass[i];
        vx[i] += ax*dt; vy[i] += ay*dt; vz[i] += az*dt;
        x[i] += vx[i]*dt; y[i] += vy[i]*dt; z[i] += vz[i]*dt;
    }
}

int main(void) {
    int n = 4096, steps = 100, batch = 128;
    double dt = 0.001, softening = 0.01;

    double *x = (double *)calloc(n, sizeof(double));
    double *y = (double *)calloc(n, sizeof(double));
    double *z = (double *)calloc(n, sizeof(double));
    double *vx = (double *)calloc(n, sizeof(double));
    double *vy = (double *)calloc(n, sizeof(double));
    double *vz = (double *)calloc(n, sizeof(double));
    double *fx = (double *)calloc(n, sizeof(double));
    double *fy = (double *)calloc(n, sizeof(double));
    double *fz = (double *)calloc(n, sizeof(double));
    double *mass = (double *)calloc(n, sizeof(double));

    int64_t seed = 42;
    for (int i = 0; i < n; i++) {
        seed = (1103515245LL*seed+12345LL) % 2147483648LL;
        x[i] = (double)(seed%1000)/100.0 - 5.0;
        seed = (1103515245LL*seed+12345LL) % 2147483648LL;
        y[i] = (double)(seed%1000)/100.0 - 5.0;
        seed = (1103515245LL*seed+12345LL) % 2147483648LL;
        z[i] = (double)(seed%1000)/100.0 - 5.0;
        seed = (1103515245LL*seed+12345LL) % 2147483648LL;
        mass[i] = (double)(seed%100)/10.0 + 0.1;
    }

    for (int s = 0; s < steps; s++) {
        compute_forces_batched(x, y, z, fx, fy, fz, mass, n, batch, softening);
        integrate(x, y, z, vx, vy, vz, fx, fy, fz, mass, n, dt);
    }

    double checksum = 0.0;
    for (int i = 0; i < n; i++) checksum += x[i] + y[i] + z[i];

    printf("%.6f\n", checksum);

    free(x); free(y); free(z);
    free(vx); free(vy); free(vz);
    free(fx); free(fy); free(fz);
    free(mass);
    return 0;
}
