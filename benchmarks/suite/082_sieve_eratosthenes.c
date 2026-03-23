#include <stdio.h>
int main(void) {
    int sieve[10001];
    for (int i = 0; i <= 10000; i++) sieve[i] = 1;
    sieve[0] = sieve[1] = 0;
    for (int i = 2; i * i <= 10000; i++)
        if (sieve[i])
            for (int j = i * i; j <= 10000; j += i) sieve[j] = 0;
    int count = 0;
    for (int i = 0; i <= 10000; i++) count += sieve[i];
    printf("%d\n", count);
    return 0;
}
