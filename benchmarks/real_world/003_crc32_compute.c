#include <stdio.h>
#include <stdint.h>

int main(void) {
    uint64_t mod32 = 4294967296ULL;
    uint32_t poly = 0xEDB88320U; /* 3988292384 */

    /* Build CRC-32 lookup table (256 entries) */
    uint32_t table[256];
    for (int i = 0; i < 256; i++) {
        uint32_t crc = (uint32_t)i;
        for (int j = 0; j < 8; j++) {
            uint32_t low_bit = crc & 1;
            crc >>= 1;
            if (low_bit) {
                crc ^= poly;
            }
        }
        table[i] = crc;
    }

    /* Process pseudo-random bytes */
    int nbytes = 500000;
    uint32_t crc = 0xFFFFFFFFU;
    uint64_t seed = 12345;
    uint64_t lcg_a = 1103515245ULL;
    uint64_t lcg_c = 12345ULL;
    uint64_t lcg_m = 2147483648ULL;

    for (int i = 0; i < nbytes; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        uint32_t byte_val = (uint32_t)(seed & 0xFF);

        /* CRC update: crc = table[(crc ^ byte) & 0xFF] ^ (crc >> 8) */
        uint32_t idx = (crc ^ byte_val) & 0xFF;
        crc = table[idx] ^ (crc >> 8);
    }

    /* Final XOR with 0xFFFFFFFF */
    uint32_t final_crc = crc ^ 0xFFFFFFFFU;

    printf("%lld\n", (long long)(uint64_t)final_crc);
    return 0;
}
