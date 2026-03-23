#include <stdio.h>
#include <math.h>
int main(void) {
    double a = 1.0, b = -5.0, c = 6.0;
    double disc = b*b - 4.0*a*c;
    printf("%f\n", (-b + sqrt(disc)) / (2.0*a));
    return 0;
}
