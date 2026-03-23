#include <stdio.h>
int main(void) {
    double r = 3.2, x = 0.5;
    for (int i = 0; i < 1000; i++) x = r * x * (1.0 - x);
    printf("%f\n", x);
    return 0;
}
