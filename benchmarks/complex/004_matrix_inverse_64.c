#include <stdio.h>
#include <math.h>

int main(void) {
    static double aug[8192]; /* 64 x 128 */
    int n = 64;

    for (int i = 0; i < 64; i++) {
        for (int j = 0; j < 64; j++) {
            double val = (double)((i * 19 + j * 7 + 3) % 50) / 100.0;
            aug[i * 128 + j] = val;
            if (i == j) aug[i * 128 + j] = val + 64.0;
        }
        for (int j = 0; j < 64; j++) {
            aug[i * 128 + 64 + j] = (i == j) ? 1.0 : 0.0;
        }
    }

    for (int col = 0; col < 64; col++) {
        double max_val = fabs(aug[col * 128 + col]);
        int max_row = col;
        for (int r = col + 1; r < 64; r++) {
            double v = fabs(aug[r * 128 + col]);
            if (v > max_val) { max_val = v; max_row = r; }
        }
        if (max_row != col) {
            for (int j = 0; j < 128; j++) {
                double tmp = aug[col * 128 + j];
                aug[col * 128 + j] = aug[max_row * 128 + j];
                aug[max_row * 128 + j] = tmp;
            }
        }
        double pivot = aug[col * 128 + col];
        for (int j = 0; j < 128; j++)
            aug[col * 128 + j] /= pivot;
        for (int r = 0; r < 64; r++) {
            if (r != col) {
                double factor = aug[r * 128 + col];
                for (int j = 0; j < 128; j++)
                    aug[r * 128 + j] -= factor * aug[col * 128 + j];
            }
        }
    }

    double checksum = 0.0;
    for (int i = 0; i < 64; i++)
        for (int j = 0; j < 64; j++)
            checksum += aug[i * 128 + 64 + j];
    printf("%f\n", checksum);
    return 0;
}
