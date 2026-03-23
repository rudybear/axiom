#include <stdio.h>

int main(void) {
    static double a[65536], b[65536], c[65536];
    int n = 256;

    for (int i = 0; i < 256; i++) {
        for (int j = 0; j < 256; j++) {
            int idx = i * 256 + j;
            a[idx] = (double)((i * 7 + j * 13 + 3) % 100) / 100.0;
            b[idx] = (double)((i * 11 + j * 5 + 7) % 100) / 100.0;
        }
    }

    for (int i = 0; i < 256; i++) {
        for (int j = 0; j < 256; j++) {
            double sum = 0.0;
            for (int k = 0; k < 256; k++) {
                sum += a[i * 256 + k] * b[k * 256 + j];
            }
            c[i * 256 + j] = sum;
        }
    }

    double checksum = 0.0;
    for (int i = 0; i < 65536; i++) {
        checksum += c[i];
    }
    printf("%f\n", checksum);
    return 0;
}
