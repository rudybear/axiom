#include <stdio.h>
#include <math.h>

static double arctan_inv(long long d, int n_terms) {
    double x = 1.0 / (double)d;
    double x2 = x * x;
    double result = x;
    double term = x;
    double sign = -1.0;
    for (int i = 1; i < n_terms; i++) {
        term *= x2;
        double denom = (double)(2 * i + 1);
        result += sign * term / denom;
        sign = -sign;
    }
    return result;
}

static double compute_pi_machin(int n_terms) {
    double a1 = arctan_inv(5, n_terms);
    double a2 = arctan_inv(239, n_terms);
    return 4.0 * (4.0 * a1 - a2);
}

static double compute_pi_leibniz(int n_terms) {
    double result = 0.0;
    double sign = 1.0;
    for (int i = 0; i < n_terms; i++) {
        double denom = (double)(2 * i + 1);
        result += sign / denom;
        sign = -sign;
    }
    return result * 4.0;
}

static double compute_pi_nilakantha(int n_terms) {
    double result = 3.0;
    double sign = 1.0;
    for (int i = 1; i <= n_terms; i++) {
        double k = (double)(2 * i);
        double denom = k * (k + 1.0) * (k + 2.0);
        result += sign * 4.0 / denom;
        sign = -sign;
    }
    return result;
}

int main(void) {
    int n_iters = 1000;
    int n_terms_base = 500;

    double checksum = 0.0;
    double pi_ref = 3.14159265358979323;

    for (int iter = 0; iter < n_iters; iter++) {
        int n_terms = n_terms_base + iter;

        double pi_machin = compute_pi_machin(n_terms / 10 + 5);
        double err_machin = fabs(pi_machin - pi_ref);

        double pi_leibniz = compute_pi_leibniz(n_terms);
        double err_leibniz = fabs(pi_leibniz - pi_ref);

        double pi_nilakantha = compute_pi_nilakantha(n_terms / 2);
        double err_nilakantha = fabs(pi_nilakantha - pi_ref);

        checksum += err_machin * 1000000.0 + err_leibniz + err_nilakantha * 1000.0;
    }

    printf("%.6f\n", checksum);
    return 0;
}
