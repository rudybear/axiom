#include <stdio.h>
#include <math.h>

int main(void) {
    double result = 0.0;

    for (int rep = 0; rep < 500000; rep++) {
        double sum = 0.0, Mk = 1.0, Lk = 13591409.0, Xk = 1.0, sign = 1.0;

        for (int k = 0; k < 15; k++) {
            sum += sign * Mk * Lk / Xk;
            double k_f = (double)k, k1 = k_f + 1.0;
            Mk *= (6.0*k_f+1.0) * (2.0*k_f+1.0) * (6.0*k_f+5.0) / (k1*k1*k1);
            Lk += 545140134.0;
            Xk *= -262537412640768000.0;
            sign *= -1.0;
        }

        double pi_inv = 12.0 * sum / 426880.0 / sqrt(10005.0);
        result = 1.0 / pi_inv;
    }

    printf("%f\n", result);
    return 0;
}
