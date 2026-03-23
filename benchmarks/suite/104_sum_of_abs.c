#include <stdio.h>
#include <stdlib.h>
int main(void) {
    int sum = 0;
    for (int i = -50; i <= 50; i++) sum += abs(i);
    printf("%d\n", sum);
    return 0;
}
