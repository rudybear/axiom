#include <stdio.h>
int main(void) {
    double sum = 0.0;
    for (int i = 1; i <= 1000000; i++) {
        double term = 1.0 / (double)i;
        if (i % 2 == 0) sum -= term;
        else sum += term;
    }
    printf("%f\n", sum);
    return 0;
}
