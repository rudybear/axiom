#include <stdio.h>
#include <stdint.h>

int main(void) {
    int n = 10000000;
    int64_t seed = 42, lcg_a = 1103515245, lcg_c = 12345, lcg_m = 2147483648LL;
    double sum = 0.0, sum_sq = 0.0, m_f = (double)lcg_m;
    int hist[100] = {0};

    for (int i = 0; i < n; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        double val = (double)seed / m_f;
        sum += val;
        sum_sq += val * val;
        int bin = (int)(val * 100.0);
        if (bin >= 100) bin = 99;
        if (bin < 0) bin = 0;
        hist[bin]++;
    }

    double mean = sum / (double)n;
    double variance = sum_sq / (double)n - mean * mean;
    double expected = (double)n / 100.0;
    double chi_sq = 0.0;
    for (int i = 0; i < 100; i++) {
        double diff = (double)hist[i] - expected;
        chi_sq += diff * diff / expected;
    }

    printf("%f\n%f\n%f\n", mean, variance, chi_sq);
    return 0;
}
