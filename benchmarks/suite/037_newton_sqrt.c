#include <stdio.h>
static double newton_sqrt(double x, int iterations) {
    double guess = x / 2.0;
    for (int i = 0; i < iterations; i++)
        guess = (guess + x / guess) / 2.0;
    return guess;
}
int main(void) { printf("%f\n", newton_sqrt(10.0, 20)); return 0; }
