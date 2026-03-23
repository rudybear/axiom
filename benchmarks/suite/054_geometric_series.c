#include <stdio.h>
int main(void) {
    double sum = 0.0, term = 1.0;
    for (int i = 0; i < 20; i++) { sum += term; term /= 2.0; }
    printf("%f\n", sum);
    return 0;
}
