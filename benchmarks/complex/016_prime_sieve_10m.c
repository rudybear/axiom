#include <stdio.h>

int main(void) {
    static int is_prime[1000001];
    int total_count = 0;

    for (int run = 0; run < 10; run++) {
        for (int i = 0; i <= 1000000; i++) is_prime[i] = 1;
        is_prime[0] = is_prime[1] = 0;

        for (int i = 2; i * i <= 1000000; i++) {
            if (is_prime[i]) {
                for (int j = i*i; j <= 1000000; j += i)
                    is_prime[j] = 0;
            }
        }

        int count = 0;
        for (int k = 0; k <= 1000000; k++) count += is_prime[k];
        total_count += count;
    }

    printf("%d\n", total_count);
    return 0;
}
