#include <stdio.h>
int main(void) {
    double phi = 1.0;
    for (int i = 0; i < 100; i++) phi = 1.0 + 1.0 / phi;
    printf("%f\n", phi);
    return 0;
}
