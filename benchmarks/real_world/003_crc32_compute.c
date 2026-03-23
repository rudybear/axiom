#include <stdio.h>
#include <stdint.h>

int main(void) {
    uint64_t mod32 = 4294967296ULL;
    uint64_t poly = 3988292384ULL;

    uint64_t pow2[32];
    pow2[0] = 1;
    for (int i = 1; i < 32; i++) pow2[i] = pow2[i-1] * 2;

    uint64_t table[256];
    for (int i = 0; i < 256; i++) {
        uint64_t crc = (uint64_t)i;
        for (int j = 0; j < 8; j++) {
            uint64_t low_bit = crc % 2;
            crc = crc / 2;
            if (low_bit == 1) {
                uint64_t and_acc = 0;
                for (int bit = 0; bit < 32; bit++) {
                    uint64_t c_bit = (crc / pow2[bit]) % 2;
                    uint64_t p_bit = (poly / pow2[bit]) % 2;
                    and_acc += c_bit * p_bit * pow2[bit];
                }
                crc = crc + poly - 2 * and_acc;
                crc = ((crc % mod32) + mod32) % mod32;
            }
        }
        table[i] = crc;
    }

    int nbytes = 500000;
    uint64_t crc = mod32 - 1;
    uint64_t seed = 12345;
    uint64_t lcg_a = 1103515245ULL;
    uint64_t lcg_c = 12345ULL;
    uint64_t lcg_m = 2147483648ULL;

    for (int i = 0; i < nbytes; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        uint64_t byte_val = seed % 256;

        uint64_t low_crc = crc % 256;
        uint64_t acc = 0;
        for (int bit = 0; bit < 8; bit++) {
            uint64_t c_bit = (low_crc / pow2[bit]) % 2;
            uint64_t b_bit = (byte_val / pow2[bit]) % 2;
            acc += c_bit * b_bit * pow2[bit];
        }
        uint64_t idx = (low_crc + byte_val - 2 * acc + 512) % 256;
        uint64_t table_val = table[(int)idx];

        uint64_t shifted_crc = crc / 256;

        uint64_t xor_acc = 0;
        for (int bit = 0; bit < 32; bit++) {
            uint64_t t_bit = (table_val / pow2[bit]) % 2;
            uint64_t s_bit = (shifted_crc / pow2[bit]) % 2;
            xor_acc += t_bit * s_bit * pow2[bit];
        }
        crc = (table_val + shifted_crc - 2 * xor_acc + 2 * mod32) % mod32;
    }

    uint64_t mask = mod32 - 1;
    uint64_t xf_acc = 0;
    for (int bit = 0; bit < 32; bit++) {
        uint64_t c_bit = (crc / pow2[bit]) % 2;
        xf_acc += c_bit * pow2[bit];
    }
    uint64_t final_crc = (crc + mask - 2 * xf_acc + 2 * mod32) % mod32;

    printf("%lld\n", (long long)final_crc);
    return 0;
}
