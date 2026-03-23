#include <stdio.h>
static int digital_root(int n) {
    int x = n;
    while (x >= 10) {
        int sum = 0, tmp = x;
        while (tmp > 0) { sum += tmp % 10; tmp /= 10; }
        x = sum;
    }
    return x;
}
int main(void) { printf("%d\n", digital_root(942)); return 0; }
