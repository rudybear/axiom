#include <stdio.h>
static double newton_cbrt(double x, int iterations) {
    double guess = x / 3.0;
    for (int i = 0; i < iterations; i++)
        guess = (2.0 * guess + x / (guess * guess)) / 3.0;
    return guess;
}
int main(void) { printf("%f\n", newton_cbrt(27.0, 30)); return 0; }
