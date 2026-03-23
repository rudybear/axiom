#include <stdio.h>
int main(void) {
    double sum = 0.0;
    for (int i = 1; i <= 1000; i++) sum += 1.0 / (double)i;
    printf("%f\n", sum);
    return 0;
}
