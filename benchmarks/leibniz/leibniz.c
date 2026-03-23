/* Leibniz Pi approximation benchmark — C reference.
 * From github.com/niklas-heer/speed-comparison
 *
 * Compile: clang -O2 -o leibniz leibniz.c
 */
#include <stdio.h>

int main(void) {
    int rounds = 100000000;
    double x = 1.0;
    for (int i = 2; i <= rounds; i++) {
        double d = (double)(2 * i - 1);
        if (i % 2 == 0) {
            x -= 1.0 / d;
        } else {
            x += 1.0 / d;
        }
    }
    double pi = x * 4.0;
    printf("%f\n", pi);
    return 0;
}
