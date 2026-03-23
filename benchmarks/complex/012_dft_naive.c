#include <stdio.h>
#include <math.h>

int main(void) {
    static double x_re[2048], x_im[2048], X_re[2048], X_im[2048];
    int n = 2048;
    double pi2 = 6.28318530717959;

    for (int i = 0; i < 2048; i++) {
        double t = (double)i / 2048.0;
        double a1 = pi2 * 50.0 * t;
        double a2 = pi2 * 120.0 * t;
        double a1r = a1 - (double)((int)(a1 / pi2)) * pi2;
        double a2r = a2 - (double)((int)(a2 / pi2)) * pi2;
        double s1 = a1r - a1r*a1r*a1r/6.0 + a1r*a1r*a1r*a1r*a1r/120.0 - a1r*a1r*a1r*a1r*a1r*a1r*a1r/5040.0;
        double s2 = a2r - a2r*a2r*a2r/6.0 + a2r*a2r*a2r*a2r*a2r/120.0 - a2r*a2r*a2r*a2r*a2r*a2r*a2r/5040.0;
        x_re[i] = s1 + 0.5 * s2;
        x_im[i] = 0.0;
    }

    double n_f = (double)n;
    for (int k = 0; k < 2048; k++) {
        double sum_re = 0.0, sum_im = 0.0;
        for (int ni = 0; ni < 2048; ni++) {
            double angle = pi2 * (double)k * (double)ni / n_f;
            double ar = angle - (double)((int)(angle / pi2)) * pi2;
            double ar2 = ar * ar;
            double cos_a = 1.0 - ar2/2.0 + ar2*ar2/24.0 - ar2*ar2*ar2/720.0 + ar2*ar2*ar2*ar2/40320.0;
            double sin_a = ar - ar*ar2/6.0 + ar*ar2*ar2/120.0 - ar*ar2*ar2*ar2/5040.0;
            sum_re += x_re[ni]*cos_a + x_im[ni]*sin_a;
            sum_im += -x_re[ni]*sin_a + x_im[ni]*cos_a;
        }
        X_re[k] = sum_re;
        X_im[k] = sum_im;
    }

    double checksum = 0.0;
    for (int k = 0; k < 2048; k++)
        checksum += sqrt(X_re[k]*X_re[k] + X_im[k]*X_im[k]);
    printf("%f\n", checksum);
    return 0;
}
