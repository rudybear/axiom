#include <stdio.h>
#include <stdint.h>

int main(void) {
    static int phi[1000001];
    int n = 1000000;

    for (int i = 0; i <= 1000000; i++) phi[i] = i;

    for (int i = 2; i <= 1000000; i++) {
        if (phi[i] == i) {
            for (int j = i; j <= 1000000; j += i)
                phi[j] = phi[j] / i * (i - 1);
        }
    }

    int64_t total = 0;
    for (int i = 1; i <= 1000000; i++) total += phi[i];
    printf("%lld\n", (long long)total);
    return 0;
}
