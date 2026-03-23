#include <stdio.h>
int main(void) {
    int sum = 0;
    for (int i = 1; i <= 100; i++) {
        if (i % 15 == 0) sum += 15;
        else if (i % 3 == 0) sum += 3;
        else if (i % 5 == 0) sum += 5;
        else sum += i;
    }
    printf("%d\n", sum);
    return 0;
}
