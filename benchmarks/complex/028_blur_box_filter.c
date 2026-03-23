#include <stdio.h>
#include <stdint.h>

int main(void) {
    static double img[250000], tmp[250000];

    int64_t seed = 11111, lcg_a = 1103515245, lcg_c = 12345, lcg_m = 2147483648LL;
    for (int i = 0; i < 250000; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        img[i] = (double)seed / (double)lcg_m * 255.0;
    }

    for (int pass = 0; pass < 10; pass++) {
        for (int i = 0; i < 500; i++)
            for (int j = 2; j < 498; j++)
                tmp[i*500+j] = (img[i*500+j-2]+img[i*500+j-1]+img[i*500+j]+img[i*500+j+1]+img[i*500+j+2]) / 5.0;
        for (int i = 2; i < 498; i++)
            for (int j = 0; j < 500; j++)
                img[i*500+j] = (tmp[(i-2)*500+j]+tmp[(i-1)*500+j]+tmp[i*500+j]+tmp[(i+1)*500+j]+tmp[(i+2)*500+j]) / 5.0;
    }

    double checksum = 0.0;
    for (int i = 0; i < 250000; i++) checksum += img[i];
    printf("%f\n", checksum);
    return 0;
}
