#include <stdio.h>
#include <math.h>
#include <stdint.h>

int main(void) {
    double coeffs[64];
    static double input[100000], output[100000];

    for (int i = 0; i < 64; i++) {
        double x = (double)i - 31.5;
        if (fabs(x) < 0.001) {
            coeffs[i] = 1.0;
        } else {
            double pi_x = 3.14159265358979 * x / 8.0;
            double s = pi_x;
            double term = pi_x;
            term = term * pi_x * pi_x / 6.0 * (-1.0);
            s += term;
            term = term * pi_x * pi_x / 20.0 * (-1.0);
            s += term;
            term = term * pi_x * pi_x / 42.0 * (-1.0);
            s += term;
            term = term * pi_x * pi_x / 72.0 * (-1.0);
            s += term;
            coeffs[i] = s / pi_x;
        }
        double angle = 2.0 * 3.14159265358979 * (double)i / 63.0;
        double a2 = angle * angle;
        double ca = 1.0 - a2/2.0 + a2*a2/24.0 - a2*a2*a2/720.0 + a2*a2*a2*a2/40320.0;
        double w = 0.54 - 0.46 * ca;
        coeffs[i] *= w;
    }

    int64_t seed = 54321, lcg_a = 1103515245, lcg_c = 12345, lcg_m = 2147483648LL;
    for (int i = 0; i < 100000; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        input[i] = (double)seed / (double)lcg_m * 2.0 - 1.0;
    }

    for (int i = 0; i < 100000; i++) {
        double acc = 0.0;
        for (int j = 0; j < 64; j++) {
            int idx = i - j;
            if (idx >= 0) acc += coeffs[j] * input[idx];
        }
        output[i] = acc;
    }

    double checksum = 0.0;
    for (int i = 0; i < 100000; i++) checksum += output[i];
    printf("%f\n", checksum);
    return 0;
}
