#include <stdio.h>
int main(void) {
    double pi = 3.14159265358979, total = 0.0;
    for (int r = 1; r < 50; r++) total += pi * (double)r * (double)r;
    printf("%f\n", total);
    return 0;
}
