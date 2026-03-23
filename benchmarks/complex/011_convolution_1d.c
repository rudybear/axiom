#include <stdio.h>
#include <math.h>
#include <stdint.h>

int main(void) {
    static double signal_arr[10000], kernel[100], output[10099];

    int64_t seed = 98765, lcg_a = 1103515245, lcg_c = 12345, lcg_m = 2147483648LL;
    for (int i = 0; i < 10000; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        signal_arr[i] = (double)seed / (double)lcg_m * 2.0 - 1.0;
    }

    double k_sum = 0.0;
    for (int i = 0; i < 100; i++) {
        double x = (double)i - 49.5;
        kernel[i] = exp(-x*x / 200.0);
        k_sum += kernel[i];
    }
    for (int i = 0; i < 100; i++) kernel[i] /= k_sum;

    for (int i = 0; i < 10099; i++) {
        double acc = 0.0;
        for (int j = 0; j < 100; j++) {
            int si = i - j;
            if (si >= 0 && si < 10000) acc += signal_arr[si] * kernel[j];
        }
        output[i] = acc;
    }

    double checksum = 0.0;
    for (int i = 0; i < 10099; i++) checksum += output[i];
    printf("%f\n", checksum);
    return 0;
}
