#include <stdio.h>
#include <math.h>

int main(void) {
    static double a[90300];
    static double x[300];
    int n = 300;

    for (int i = 0; i < 300; i++) {
        for (int j = 0; j < 301; j++) {
            int idx = i * 301 + j;
            a[idx] = (double)((i * 31 + j * 17 + 11) % 300) / 100.0;
            if (i == j) a[idx] += 300.0;
        }
    }

    for (int col = 0; col < 300; col++) {
        double max_val = fabs(a[col * 301 + col]);
        int max_row = col;
        for (int r = col + 1; r < 300; r++) {
            double v = fabs(a[r * 301 + col]);
            if (v > max_val) { max_val = v; max_row = r; }
        }
        if (max_row != col) {
            for (int j = 0; j < 301; j++) {
                double tmp = a[col * 301 + j];
                a[col * 301 + j] = a[max_row * 301 + j];
                a[max_row * 301 + j] = tmp;
            }
        }
        double pivot = a[col * 301 + col];
        for (int r = col + 1; r < 300; r++) {
            double factor = a[r * 301 + col] / pivot;
            for (int j = col; j < 301; j++)
                a[r * 301 + j] -= factor * a[col * 301 + j];
        }
    }

    for (int i = 299; i >= 0; i--) {
        double sum = a[i * 301 + 300];
        for (int j = i + 1; j < 300; j++)
            sum -= a[i * 301 + j] * x[j];
        x[i] = sum / a[i * 301 + i];
    }

    double checksum = 0.0;
    for (int i = 0; i < 300; i++) checksum += x[i];
    printf("%f\n", checksum);
    return 0;
}
