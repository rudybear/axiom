#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

static void matmul_tiled(double *a, double *b, double *c, int n, int tile) {
    for (int ii = 0; ii < n / tile; ii++) {
        for (int jj = 0; jj < n / tile; jj++) {
            for (int kk = 0; kk < n / tile; kk++) {
                int i_base = ii * tile;
                int j_base = jj * tile;
                int k_base = kk * tile;

                for (int i = i_base; i < i_base + tile; i++) {
                    for (int k = k_base; k < k_base + tile; k++) {
                        double a_ik = a[i * n + k];
                        for (int j = j_base; j < j_base + tile; j++) {
                            c[i * n + j] += a_ik * b[k * n + j];
                        }
                    }
                }
            }
        }
    }
}

static double compute_checksum(double *mat, int n) {
    double sum = 0.0;
    int total = n * n;
    for (int idx = 0; idx < total / 97; idx++) {
        sum += mat[idx * 97];
    }
    return sum;
}

int main(void) {
    int n = 512;
    int tile = 32;
    int total = n * n;

    double *a = (double *)calloc(total, sizeof(double));
    double *b = (double *)calloc(total, sizeof(double));
    double *c = (double *)calloc(total, sizeof(double));

    int64_t seed = 42;
    for (int i = 0; i < total; i++) {
        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        a[i] = (double)(seed % 100) / 100.0;
        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        b[i] = (double)(seed % 100) / 100.0;
    }

    matmul_tiled(a, b, c, n, tile);

    double checksum = compute_checksum(c, n);
    printf("%.6f\n", checksum);

    free(a); free(b); free(c);
    return 0;
}
