#include <stdio.h>
#include <math.h>
#include <stdlib.h>

static double matrix_a(int i, int j) {
    int ij = i + j;
    return 1.0 / (double)(ij * (ij + 1) / 2 + i + 1);
}

static void mult_av(int n, double *v, double *av) {
    for (int i = 0; i < n; i++) {
        double sum = 0.0;
        for (int j = 0; j < n; j++)
            sum += matrix_a(i, j) * v[j];
        av[i] = sum;
    }
}

static void mult_atv(int n, double *v, double *atv) {
    for (int i = 0; i < n; i++) {
        double sum = 0.0;
        for (int j = 0; j < n; j++)
            sum += matrix_a(j, i) * v[j];
        atv[i] = sum;
    }
}

static void mult_atav(int n, double *v, double *atav, double *tmp) {
    mult_av(n, v, tmp);
    mult_atv(n, tmp, atav);
}

int main(void) {
    int n = 5500;
    double *u = (double *)calloc(n, sizeof(double));
    double *v = (double *)calloc(n, sizeof(double));
    double *tmp = (double *)calloc(n, sizeof(double));

    for (int i = 0; i < n; i++) u[i] = 1.0;

    for (int iter = 0; iter < 10; iter++) {
        mult_atav(n, u, v, tmp);
        mult_atav(n, v, u, tmp);
    }

    double vbv = 0.0, vv = 0.0;
    for (int i = 0; i < n; i++) {
        vbv += u[i] * v[i];
        vv += v[i] * v[i];
    }

    printf("%.9f\n", sqrt(vbv / vv));

    free(u); free(v); free(tmp);
    return 0;
}
