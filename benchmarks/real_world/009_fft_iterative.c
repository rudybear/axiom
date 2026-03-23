#include <stdio.h>
#include <math.h>

static double re[8192], im[8192];

int main(void) {
    int n = 8192;
    double pi2 = 6.28318530717959;
    double pi = 3.14159265358979;

    for (int i = 0; i < n; i++) {
        double t = (double)i / (double)n;
        double a1 = pi2 * 100.0 * t;
        double a2 = pi2 * 250.0 * t;
        double a1r = a1 - (double)(int)(a1/pi2) * pi2;
        if (a1r > pi) a1r -= pi2;
        if (a1r < -pi) a1r += pi2;
        double a2r = a2 - (double)(int)(a2/pi2) * pi2;
        if (a2r > pi) a2r -= pi2;
        if (a2r < -pi) a2r += pi2;

        double a1_3=a1r*a1r*a1r, a1_5=a1_3*a1r*a1r, a1_7=a1_5*a1r*a1r;
        double s1 = a1r - a1_3/6.0 + a1_5/120.0 - a1_7/5040.0;
        double a2_3=a2r*a2r*a2r, a2_5=a2_3*a2r*a2r, a2_7=a2_5*a2r*a2r;
        double s2 = a2r - a2_3/6.0 + a2_5/120.0 - a2_7/5040.0;

        re[i] = s1 + 0.5 * s2;
        im[i] = 0.0;
    }

    int log_n = 13;
    for (int i = 0; i < n; i++) {
        int rev = 0, tmp = i;
        for (int b = 0; b < log_n; b++) { rev = rev*2 + tmp%2; tmp /= 2; }
        if (rev > i) {
            double tr = re[i], ti = im[i];
            re[i] = re[rev]; im[i] = im[rev];
            re[rev] = tr; im[rev] = ti;
        }
    }

    for (int stage_size = 2; stage_size <= n; stage_size *= 2) {
        int half = stage_size / 2;
        double angle_step = -pi2 / (double)stage_size;

        for (int group = 0; group < n; group += stage_size) {
            for (int k = 0; k < half; k++) {
                double angle = angle_step * (double)k;
                double ar = angle - (double)(int)(angle/pi2) * pi2;
                if (ar > pi) ar -= pi2;
                if (ar < -pi) ar += pi2;
                double ar2 = ar*ar;
                double cos_a = 1.0 - ar2/2.0 + ar2*ar2/24.0 - ar2*ar2*ar2/720.0 + ar2*ar2*ar2*ar2/40320.0;
                double sin_a = ar - ar*ar2/6.0 + ar*ar2*ar2/120.0 - ar*ar2*ar2*ar2/5040.0;

                int even_idx = group + k, odd_idx = group + k + half;
                double tr = cos_a*re[odd_idx] - sin_a*im[odd_idx];
                double ti = sin_a*re[odd_idx] + cos_a*im[odd_idx];

                re[odd_idx] = re[even_idx] - tr;
                im[odd_idx] = im[even_idx] - ti;
                re[even_idx] = re[even_idx] + tr;
                im[even_idx] = im[even_idx] + ti;
            }
        }
    }

    double checksum = 0.0;
    for (int i = 0; i < n; i++) checksum += sqrt(re[i]*re[i] + im[i]*im[i]);
    printf("%.6f\n", checksum);
    return 0;
}
