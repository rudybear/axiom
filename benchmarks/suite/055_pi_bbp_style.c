#include <stdio.h>
int main(void) {
    double pi = 0.0, sixteen_pow = 1.0;
    for (int k = 0; k < 20; k++) {
        double kf = (double)k;
        double term = 4.0/(8.0*kf+1.0) - 2.0/(8.0*kf+4.0) - 1.0/(8.0*kf+5.0) - 1.0/(8.0*kf+6.0);
        pi += term / sixteen_pow;
        sixteen_pow *= 16.0;
    }
    printf("%f\n", pi);
    return 0;
}
