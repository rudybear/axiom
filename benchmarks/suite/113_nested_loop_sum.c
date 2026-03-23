#include <stdio.h>
int main(void) {
    int total = 0;
    for (int i = 0; i < 10; i++)
        for (int j = 0; j <= i; j++)
            total += i;
    printf("%d\n", total);
    return 0;
}
