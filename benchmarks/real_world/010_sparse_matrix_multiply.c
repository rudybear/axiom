#include <stdio.h>
#include <math.h>
#include <stdint.h>

static double values[100000];
static int col_idx[100000];
static int row_ptr[5001];
static double x[5000], y[5000];

int main(void) {
    int n = 5000, nnz_target = 100000;

    int64_t seed = 42, lcg_a = 1103515245LL, lcg_c = 12345LL, lcg_m = 2147483648LL;

    int nnz = 0, nnz_per_row = nnz_target / n;

    for (int i = 0; i < n; i++) {
        row_ptr[i] = nnz;
        if (nnz < 100000) {
            values[nnz] = (double)(i+1) * 0.01 + 2.0;
            col_idx[nnz] = i;
            nnz++;
        }
        for (int elems = 0; elems < nnz_per_row - 1 && nnz < 100000; elems++) {
            seed = (lcg_a * seed + lcg_c) % lcg_m;
            int col = (int)(seed % n);
            if (col != i) {
                seed = (lcg_a * seed + lcg_c) % lcg_m;
                double val = (double)seed / (double)lcg_m * 2.0 - 1.0;
                values[nnz] = val; col_idx[nnz] = col; nnz++;
            }
        }
    }
    row_ptr[n] = nnz;

    for (int i = 0; i < n; i++) x[i] = 1.0 / (double)(i+1);

    for (int iter = 0; iter < 20; iter++) {
        for (int i = 0; i < n; i++) {
            double sum = 0.0;
            for (int j = row_ptr[i]; j < row_ptr[i+1]; j++)
                sum += values[j] * x[col_idx[j]];
            y[i] = sum;
        }
        double norm = 0.0;
        for (int i = 0; i < n; i++) norm += y[i]*y[i];
        norm = sqrt(norm);
        if (norm > 1e-7)
            for (int i = 0; i < n; i++) x[i] = y[i] / norm;
    }

    double checksum = 0.0;
    for (int i = 0; i < n; i++) checksum += x[i] * (double)(i+1);
    checksum += (double)nnz;
    printf("%.6f\n", checksum);
    return 0;
}
