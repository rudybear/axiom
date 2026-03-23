#include <stdio.h>
#include <math.h>
#include <stdint.h>

static double A[250000];
static double v[500], w[500];

int main(void) {
    int n = 500;

    int64_t seed = 12345, lcg_a = 1103515245LL, lcg_c = 12345LL, lcg_m = 2147483648LL;

    for (int i = 0; i < n; i++) {
        for (int j = i; j < n; j++) {
            seed = (lcg_a * seed + lcg_c) % lcg_m;
            double val = (double)seed / (double)lcg_m * 2.0 - 1.0;
            A[i*n+j] = val; A[j*n+i] = val;
        }
        A[i*n+i] += (double)n * 0.5;
    }

    double inv_sqrt_n = 1.0 / sqrt((double)n);
    for (int i = 0; i < n; i++) v[i] = inv_sqrt_n;

    double eigenvalue = 0.0;

    for (int iter = 0; iter < 200; iter++) {
        for (int i = 0; i < n; i++) {
            double sum = 0.0;
            for (int j = 0; j < n; j++) sum += A[i*n+j] * v[j];
            w[i] = sum;
        }

        eigenvalue = 0.0;
        for (int i = 0; i < n; i++) eigenvalue += v[i]*w[i];

        double norm = 0.0;
        for (int i = 0; i < n; i++) norm += w[i]*w[i];
        norm = sqrt(norm);
        if (norm > 1e-7)
            for (int i = 0; i < n; i++) v[i] = w[i] / norm;
    }

    double residual = 0.0;
    for (int i = 0; i < n; i++) {
        double av_i = 0.0;
        for (int j = 0; j < n; j++) av_i += A[i*n+j]*v[j];
        double diff = av_i - eigenvalue*v[i];
        residual += diff*diff;
    }
    residual = sqrt(residual);

    double v_sum = 0.0;
    for (int i = 0; i < n; i++) v_sum += v[i] * (double)(i+1);

    double checksum = eigenvalue * 100.0 + residual + v_sum;
    printf("%.6f\n", checksum);
    return 0;
}
