#include <stdio.h>
#include <stdint.h>

static inline uint32_t rotr32(uint32_t x, int n) {
    return (x >> n) | (x << (32 - n));
}

int main(void) {
    int blocks = 10000;
    uint64_t mod32 = 4294967296ULL;

    uint64_t h0=1779033703, h1=3144134277, h2=1013904242, h3=2773480762;
    uint64_t h4=1359893119, h5=2600822924, h6=528734635, h7=1541459225;

    uint64_t k[64] = {
        1116352408,1899447441,3049323471,3921009573,
        961987163,1508970993,2453635748,2870763221,
        3624381080,310598401,607225278,1426881987,
        1925078388,2162078206,2614888103,3248222580,
        3835390401,4022224774,264347078,604807628,
        770255983,1249150122,1555081692,1996064986,
        2554220882,2821834349,2952996808,3210313671,
        3336571891,3584528711,113926993,338241895,
        666307205,773529912,1294757372,1396182291,
        1695183700,1986661051,2177026350,2456956037,
        2730485921,2820302411,3259730800,3345764771,
        3516065817,3600352804,4094571909,275423344,
        430227734,506948616,659060556,883997877,
        958139571,1322822218,1537002063,1747873779,
        1955562222,2024104815,2227730452,2361852424,
        2428436474,2756734187,3204031479,3329325298
    };

    uint64_t w[64];

    for (int block = 0; block < blocks; block++) {
        uint64_t seed = (uint64_t)block * 2654435761ULL + 1;
        for (int i = 0; i < 16; i++) {
            seed = (seed * 6364136223846793005ULL + (uint64_t)i * 1442695040888963407ULL) % mod32;
            w[i] = seed;
        }

        for (int i = 16; i < 64; i++) {
            uint32_t w15 = (uint32_t)w[i-15];
            uint32_t r7 = rotr32(w15, 7);
            uint32_t r18 = rotr32(w15, 18);
            uint32_t sh3 = w15 >> 3;
            uint32_t s0 = r7 ^ r18 ^ sh3;

            uint32_t w2 = (uint32_t)w[i-2];
            uint32_t r17 = rotr32(w2, 17);
            uint32_t r19 = rotr32(w2, 19);
            uint32_t sh10 = w2 >> 10;
            uint32_t s1 = r17 ^ r19 ^ sh10;

            w[i] = (w[i-16] + s0 + w[i-7] + s1) % mod32;
        }

        uint64_t a=h0, b=h1, c=h2, d=h3, e=h4, f=h5, g=h6, hh=h7;

        for (int i = 0; i < 64; i++) {
            uint32_t e32 = (uint32_t)e;
            uint32_t S1 = rotr32(e32, 6) ^ rotr32(e32, 11) ^ rotr32(e32, 25);

            uint32_t ch = (e32 & (uint32_t)f) ^ (~e32 & (uint32_t)g);

            uint64_t temp1 = (hh + S1 + ch + k[i] + w[i]) % mod32;

            uint32_t a32 = (uint32_t)a;
            uint32_t S0 = rotr32(a32, 2) ^ rotr32(a32, 13) ^ rotr32(a32, 22);

            uint32_t maj = (a32 & (uint32_t)b) ^ (a32 & (uint32_t)c) ^ ((uint32_t)b & (uint32_t)c);

            uint64_t temp2 = (S0 + maj) % mod32;

            hh = g; g = f; f = e;
            e = (d + temp1) % mod32;
            d = c; c = b; b = a;
            a = (temp1 + temp2) % mod32;
        }

        h0=(h0+a)%mod32; h1=(h1+b)%mod32; h2=(h2+c)%mod32; h3=(h3+d)%mod32;
        h4=(h4+e)%mod32; h5=(h5+f)%mod32; h6=(h6+g)%mod32; h7=(h7+hh)%mod32;
    }

    uint64_t checksum = (h0+h1+h2+h3+h4+h5+h6+h7) % mod32;
    printf("%lld\n", (long long)checksum);
    return 0;
}
