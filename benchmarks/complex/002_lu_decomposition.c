#include <stdio.h>

int main(void) {
    static double a[40000], l[40000], u[40000];
    int n = 200;

    for (int i = 0; i < 200; i++) {
        for (int j = 0; j < 200; j++) {
            int idx = i * 200 + j;
            a[idx] = (double)((i * 17 + j * 23 + 5) % 200) / 50.0;
            l[idx] = 0.0;
            u[idx] = 0.0;
            if (i == j) a[idx] += 200.0;
        }
    }

    for (int i = 0; i < 200; i++) {
        for (int k = i; k < 200; k++) {
            double sum = 0.0;
            for (int j = 0; j < i; j++)
                sum += l[i * 200 + j] * u[j * 200 + k];
            u[i * 200 + k] = a[i * 200 + k] - sum;
        }
        for (int k = i; k < 200; k++) {
            if (i == k) {
                l[i * 200 + i] = 1.0;
            } else {
                double sum = 0.0;
                for (int j = 0; j < i; j++)
                    sum += l[k * 200 + j] * u[j * 200 + i];
                l[k * 200 + i] = (a[k * 200 + i] - sum) / u[i * 200 + i];
            }
        }
    }

    double checksum = 0.0;
    for (int i = 0; i < 200; i++)
        checksum += u[i * 200 + i];
    printf("%f\n", checksum);
    return 0;
}
