#include <stdio.h>
int main(void) {
    double a = 1.4, b = 0.3, x = 0.0, y = 0.0;
    for (int i = 0; i < 1000; i++) {
        double new_x = 1.0 - a * x * x + y;
        y = b * x;
        x = new_x;
    }
    printf("%f\n", x);
    return 0;
}
