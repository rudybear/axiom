#include <stdio.h>
int main(void) {
    double x = 1.0;
    for (int i = 2; i <= 10000000; i++) {
        double d = (double)(2 * i - 1);
        if (i % 2 == 0) x -= 1.0 / d;
        else x += 1.0 / d;
    }
    printf("%f\n", x * 4.0);
    return 0;
}
