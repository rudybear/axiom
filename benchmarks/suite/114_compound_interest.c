#include <stdio.h>
int main(void) {
    double amount = 1.0;
    for (int i = 0; i < 10; i++) amount *= 1.1;
    printf("%f\n", amount);
    return 0;
}
