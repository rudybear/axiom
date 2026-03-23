#include <stdio.h>
#include <stdint.h>
int main(void) {
    int64_t sum = 0;
    for (int i = 1; i <= 100; i++) sum += (int64_t)i * i * i;
    printf("%lld\n", (long long)sum);
    return 0;
}
