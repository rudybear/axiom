#include <stdio.h>
int main(void) {
    double e = 1.0, factorial = 1.0;
    for (int i = 1; i <= 20; i++) {
        factorial *= (double)i;
        e += 1.0 / factorial;
    }
    printf("%f\n", e);
    return 0;
}
