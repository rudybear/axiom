#include <stdio.h>
#include <math.h>

static double diag[1000], upper_arr[1000], lower_arr[1000];
static double upper5[1000], lower5[1000];
static double b[1000], x[1000], r[1000], p[1000], ap[1000];

int main(void) {
    int n = 1000;

    for (int i = 0; i < n; i++) {
        diag[i] = 4.0 + (double)(i%10) * 0.1;
        if (i < n-1) { upper_arr[i] = -1.0; lower_arr[i+1] = -1.0; }
        if (i < n-5) { upper5[i] = -0.3; lower5[i+5] = -0.3; }
    }

    for (int i = 0; i < n; i++) b[i] = 1.0 + (double)i * 0.001;
    for (int i = 0; i < n; i++) { x[i] = 0.0; r[i] = b[i]; p[i] = r[i]; }

    double rsold = 0.0;
    for (int i = 0; i < n; i++) rsold += r[i]*r[i];

    int max_iter = 1000, iter_count = 0;
    double tol = 1e-10;

    for (int iter = 0; iter < max_iter; iter++) {
        for (int i = 0; i < n; i++) {
            double val = diag[i]*p[i];
            if (i > 0) val += lower_arr[i]*p[i-1];
            if (i < n-1) val += upper_arr[i]*p[i+1];
            if (i >= 5) val += lower5[i]*p[i-5];
            if (i < n-5) val += upper5[i]*p[i+5];
            ap[i] = val;
        }

        double pap = 0.0;
        for (int i = 0; i < n; i++) pap += p[i]*ap[i];
        if (fabs(pap) < tol) break;

        double alpha = rsold / pap;
        for (int i = 0; i < n; i++) x[i] += alpha*p[i];
        for (int i = 0; i < n; i++) r[i] -= alpha*ap[i];

        double rsnew = 0.0;
        for (int i = 0; i < n; i++) rsnew += r[i]*r[i];
        iter_count = iter + 1;

        if (rsnew < tol) break;

        double beta = rsnew / rsold;
        for (int i = 0; i < n; i++) p[i] = r[i] + beta*p[i];
        rsold = rsnew;
    }

    double residual = 0.0;
    for (int i = 0; i < n; i++) {
        double ax_i = diag[i]*x[i];
        if (i > 0) ax_i += lower_arr[i]*x[i-1];
        if (i < n-1) ax_i += upper_arr[i]*x[i+1];
        if (i >= 5) ax_i += lower5[i]*x[i-5];
        if (i < n-5) ax_i += upper5[i]*x[i+5];
        double diff = ax_i - b[i];
        residual += diff*diff;
    }
    residual = sqrt(residual);

    double x_norm = 0.0;
    for (int i = 0; i < n; i++) x_norm += x[i]*x[i];
    x_norm = sqrt(x_norm);

    double checksum = x_norm * 1000.0 + residual + (double)iter_count;
    printf("%.6f\n", checksum);
    return 0;
}
