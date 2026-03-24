#include <stdio.h>
#include <stdint.h>

static inline uint32_t rotl32(uint32_t x, int n) {
    return (x << n) | (x >> (32 - n));
}

int main(void) {
    int nblocks = 50000;
    uint64_t mod32 = 4294967296ULL;

    uint64_t state[16] = {
        1634760805, 857760878, 2036477234, 1797285236,
        66051, 67438087, 134810123, 202182159,
        269554195, 336926231, 404298267, 471670303,
        0, 0, 100663296, 1207959552
    };

    uint64_t working[16];
    uint64_t checksum = 0;

    for (int block = 0; block < nblocks; block++) {
        state[12] = (uint64_t)block;

        for (int i = 0; i < 16; i++) working[i] = state[i];

        for (int round = 0; round < 10; round++) {
            /* Column rounds */
            /* Column 1: 0,4,8,12 */
            working[0] = (working[0] + working[4]) % mod32;
            working[12] = rotl32((uint32_t)(working[12] ^ working[0]), 16);
            working[8] = (working[8] + working[12]) % mod32;
            working[4] = rotl32((uint32_t)(working[4] ^ working[8]), 12);
            working[0] = (working[0] + working[4]) % mod32;
            working[12] = rotl32((uint32_t)(working[12] ^ working[0]), 8);
            working[8] = (working[8] + working[12]) % mod32;
            working[4] = rotl32((uint32_t)(working[4] ^ working[8]), 7);

            /* Column 2: 1,5,9,13 */
            working[1] = (working[1] + working[5]) % mod32;
            working[13] = rotl32((uint32_t)(working[13] ^ working[1]), 16);
            working[9] = (working[9] + working[13]) % mod32;
            working[5] = rotl32((uint32_t)(working[5] ^ working[9]), 12);
            working[1] = (working[1] + working[5]) % mod32;
            working[13] = rotl32((uint32_t)(working[13] ^ working[1]), 8);
            working[9] = (working[9] + working[13]) % mod32;
            working[5] = rotl32((uint32_t)(working[5] ^ working[9]), 7);

            /* Column 3: 2,6,10,14 */
            working[2] = (working[2] + working[6]) % mod32;
            working[14] = rotl32((uint32_t)(working[14] ^ working[2]), 16);
            working[10] = (working[10] + working[14]) % mod32;
            working[6] = rotl32((uint32_t)(working[6] ^ working[10]), 12);
            working[2] = (working[2] + working[6]) % mod32;
            working[14] = rotl32((uint32_t)(working[14] ^ working[2]), 8);
            working[10] = (working[10] + working[14]) % mod32;
            working[6] = rotl32((uint32_t)(working[6] ^ working[10]), 7);

            /* Column 4: 3,7,11,15 */
            working[3] = (working[3] + working[7]) % mod32;
            working[15] = rotl32((uint32_t)(working[15] ^ working[3]), 16);
            working[11] = (working[11] + working[15]) % mod32;
            working[7] = rotl32((uint32_t)(working[7] ^ working[11]), 12);
            working[3] = (working[3] + working[7]) % mod32;
            working[15] = rotl32((uint32_t)(working[15] ^ working[3]), 8);
            working[11] = (working[11] + working[15]) % mod32;
            working[7] = rotl32((uint32_t)(working[7] ^ working[11]), 7);

            /* Diagonal rounds */
            /* Diagonal 1: 0,5,10,15 */
            working[0] = (working[0] + working[5]) % mod32;
            working[15] = rotl32((uint32_t)(working[15] ^ working[0]), 16);
            working[10] = (working[10] + working[15]) % mod32;
            working[5] = rotl32((uint32_t)(working[5] ^ working[10]), 12);
            working[0] = (working[0] + working[5]) % mod32;
            working[15] = rotl32((uint32_t)(working[15] ^ working[0]), 8);
            working[10] = (working[10] + working[15]) % mod32;
            working[5] = rotl32((uint32_t)(working[5] ^ working[10]), 7);

            /* Diagonal 2: 1,6,11,12 */
            working[1] = (working[1] + working[6]) % mod32;
            working[12] = rotl32((uint32_t)(working[12] ^ working[1]), 16);
            working[11] = (working[11] + working[12]) % mod32;
            working[6] = rotl32((uint32_t)(working[6] ^ working[11]), 12);
            working[1] = (working[1] + working[6]) % mod32;
            working[12] = rotl32((uint32_t)(working[12] ^ working[1]), 8);
            working[11] = (working[11] + working[12]) % mod32;
            working[6] = rotl32((uint32_t)(working[6] ^ working[11]), 7);

            /* Diagonal 3: 2,7,8,13 */
            working[2] = (working[2] + working[7]) % mod32;
            working[13] = rotl32((uint32_t)(working[13] ^ working[2]), 16);
            working[8] = (working[8] + working[13]) % mod32;
            working[7] = rotl32((uint32_t)(working[7] ^ working[8]), 12);
            working[2] = (working[2] + working[7]) % mod32;
            working[13] = rotl32((uint32_t)(working[13] ^ working[2]), 8);
            working[8] = (working[8] + working[13]) % mod32;
            working[7] = rotl32((uint32_t)(working[7] ^ working[8]), 7);

            /* Diagonal 4: 3,4,9,14 */
            working[3] = (working[3] + working[4]) % mod32;
            working[14] = rotl32((uint32_t)(working[14] ^ working[3]), 16);
            working[9] = (working[9] + working[14]) % mod32;
            working[4] = rotl32((uint32_t)(working[4] ^ working[9]), 12);
            working[3] = (working[3] + working[4]) % mod32;
            working[14] = rotl32((uint32_t)(working[14] ^ working[3]), 8);
            working[9] = (working[9] + working[14]) % mod32;
            working[4] = rotl32((uint32_t)(working[4] ^ working[9]), 7);
        }

        for (int i = 0; i < 16; i++) working[i] = (working[i]+state[i])%mod32;
        for (int i = 0; i < 16; i++) checksum = (checksum+working[i])%mod32;
    }

    printf("%lld\n", (long long)checksum);
    return 0;
}
