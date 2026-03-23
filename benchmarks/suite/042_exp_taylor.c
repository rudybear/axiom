#include <stdio.h>
static double exp_taylor(double x, int terms) {
    double result = 1.0, term = 1.0;
    for (int i = 1; i < terms; i++) {
        term *= x / (double)i;
        result += term;
    }
    return result;
}
int main(void) { printf("%f\n", exp_taylor(2.0, 25)); return 0; }
