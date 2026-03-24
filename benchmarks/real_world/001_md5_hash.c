#include <stdio.h>
#include <stdint.h>

static inline uint32_t rotl32(uint32_t x, int n) {
    return (x << n) | (x >> (32 - n));
}

int main(void) {
    int blocks = 10000;
    uint64_t mod32 = 4294967296ULL;

    uint64_t a0 = 1732584193ULL;
    uint64_t b0 = 4023233417ULL;
    uint64_t c0 = 2562383102ULL;
    uint64_t d0 = 271733878ULL;

    int s[64] = {
        7,12,17,22,7,12,17,22,7,12,17,22,7,12,17,22,
        5,9,14,20,5,9,14,20,5,9,14,20,5,9,14,20,
        4,11,16,23,4,11,16,23,4,11,16,23,4,11,16,23,
        6,10,15,21,6,10,15,21,6,10,15,21,6,10,15,21
    };

    uint64_t K[64] = {
        3614090360ULL,3905402710ULL,606105819ULL,3250441966ULL,
        4118548399ULL,1200080426ULL,2821735955ULL,4249261313ULL,
        1770035416ULL,2336552879ULL,4294925233ULL,2304563134ULL,
        1804603682ULL,4254626195ULL,2792965006ULL,1236535329ULL,
        4129170786ULL,3225465664ULL,643717713ULL,3921069994ULL,
        3593408605ULL,38016083ULL,3634488961ULL,3889429448ULL,
        568446438ULL,3275163606ULL,4107603335ULL,1163531501ULL,
        2850285829ULL,4243563512ULL,1735328473ULL,2368359562ULL,
        4294588738ULL,2272392833ULL,1839030562ULL,4259657740ULL,
        2763975236ULL,1272893353ULL,4139469664ULL,3200236656ULL,
        681279174ULL,3936430074ULL,3572445317ULL,76029189ULL,
        3654602809ULL,3873151461ULL,530742520ULL,3299628645ULL,
        4096336452ULL,1126891415ULL,2878612391ULL,4237533241ULL,
        1700485571ULL,2399980690ULL,4293915773ULL,2240044497ULL,
        1873313359ULL,4264355552ULL,2734768916ULL,1309151649ULL,
        4149444226ULL,3174756917ULL,718787259ULL,3951481745ULL
    };

    int g_idx[64];
    for (int i = 0; i < 16; i++) g_idx[i] = i;
    for (int i = 16; i < 32; i++) g_idx[i] = (5*i+1) % 16;
    for (int i = 32; i < 48; i++) g_idx[i] = (3*i+5) % 16;
    for (int i = 48; i < 64; i++) g_idx[i] = (7*i) % 16;

    uint64_t M[16];

    for (int block = 0; block < blocks; block++) {
        uint64_t seed = (uint64_t)block * 1103515245ULL + 12345ULL;
        for (int i = 0; i < 16; i++) {
            seed = (seed * 6364136223846793005ULL + 1442695040888963407ULL) % mod32;
            M[i] = seed;
        }

        uint64_t a = a0, b = b0, c = c0, d = d0;

        for (int i = 0; i < 64; i++) {
            uint32_t b32 = (uint32_t)b, c32 = (uint32_t)c, d32 = (uint32_t)d;
            uint32_t f_val;
            if (i < 16) {
                // F = (B AND C) OR (NOT B AND D)
                f_val = (b32 & c32) | (~b32 & d32);
            } else if (i < 32) {
                // G = (D AND B) OR (NOT D AND C)
                f_val = (d32 & b32) | (~d32 & c32);
            } else if (i < 48) {
                // H = B XOR C XOR D
                f_val = b32 ^ c32 ^ d32;
            } else {
                // I = C XOR (B OR NOT D)
                f_val = c32 ^ (b32 | ~d32);
            }

            int g = g_idx[i];
            uint64_t temp = (a + f_val + K[i] + M[g]) % mod32;
            int shift = s[i];

            uint32_t rotated = rotl32((uint32_t)temp, shift);

            uint64_t new_b = (b + rotated) % mod32;
            a = d;
            d = c;
            c = b;
            b = new_b;
        }

        a0 = (a0 + a) % mod32;
        b0 = (b0 + b) % mod32;
        c0 = (c0 + c) % mod32;
        d0 = (d0 + d) % mod32;
    }

    uint64_t checksum = (a0 + b0 + c0 + d0) % mod32;
    printf("%lld\n", (long long)checksum);
    return 0;
}
