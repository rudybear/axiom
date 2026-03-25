#include <stdio.h>
#include <stdint.h>

static int classify_char(int c) {
    if (c >= 48 && c <= 57) return 1;
    if (c >= 65 && c <= 90) return 2;
    if (c >= 97 && c <= 122) return 2;
    if (c == 32 || c == 10) return 3;
    if (c == 40 || c == 41) return 4;
    if (c == 123 || c == 125) return 5;
    if (c == 59) return 6;
    if (c == 64) return 7;
    if (c == 63) return 8;
    return 9;
}

int main(void) {
    int n = 10000000;
    int counts[10] = {0};
    int64_t seed = 42;
    for (int i = 0; i < n; i++) {
        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        int c = (int)(seed % 128);
        int cls = classify_char(c);
        counts[cls]++;
    }
    int64_t checksum = 0;
    for (int i = 0; i < 10; i++) {
        checksum += (int64_t)counts[i] * (int64_t)i;
    }
    printf("%lld\n", (long long)checksum);
    return 0;
}
