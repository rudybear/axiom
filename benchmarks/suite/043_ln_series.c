#include <stdio.h>
static double ln_approx(double x, int terms) {
    double y = (x - 1.0) / (x + 1.0), result = 0.0, power = y;
    for (int i = 0; i < terms; i++) {
        int n = 2 * i + 1;
        result += power / (double)n;
        power *= y * y;
    }
    return 2.0 * result;
}
int main(void) { printf("%f\n", ln_approx(2.0, 50)); return 0; }
