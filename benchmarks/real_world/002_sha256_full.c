#include <stdio.h>
#include <stdint.h>

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

    uint64_t pow2[32];
    pow2[0] = 1;
    for (int i = 1; i < 32; i++) pow2[i] = pow2[i-1] * 2;

    uint64_t w[64];

    for (int block = 0; block < blocks; block++) {
        uint64_t seed = (uint64_t)block * 2654435761ULL + 1;
        for (int i = 0; i < 16; i++) {
            seed = (seed * 6364136223846793005ULL + (uint64_t)i * 1442695040888963407ULL) % mod32;
            w[i] = seed;
        }

        for (int i = 16; i < 64; i++) {
            uint64_t w15 = w[i-15];
            uint64_t r7 = (w15/pow2[7] + w15*pow2[25]) % mod32;
            uint64_t r18 = (w15/pow2[18] + w15*pow2[14]) % mod32;
            uint64_t sh3 = w15/pow2[3];
            uint64_t xor1_and = (r7*r18) % mod32;
            uint64_t xor1 = (r7+r18 - 2*xor1_and + 2*mod32) % mod32;
            uint64_t xor2_and = (xor1*sh3) % mod32;
            uint64_t s0 = (xor1+sh3 - 2*xor2_and + 2*mod32) % mod32;

            uint64_t w2 = w[i-2];
            uint64_t r17 = (w2/pow2[17] + w2*pow2[15]) % mod32;
            uint64_t r19 = (w2/pow2[19] + w2*pow2[13]) % mod32;
            uint64_t sh10 = w2/pow2[10];
            uint64_t xor3_and = (r17*r19) % mod32;
            uint64_t xor3 = (r17+r19 - 2*xor3_and + 2*mod32) % mod32;
            uint64_t xor4_and = (xor3*sh10) % mod32;
            uint64_t s1 = (xor3+sh10 - 2*xor4_and + 2*mod32) % mod32;

            w[i] = (w[i-16] + s0 + w[i-7] + s1) % mod32;
        }

        uint64_t a=h0, b=h1, c=h2, d=h3, e=h4, f=h5, g=h6, hh=h7;

        for (int i = 0; i < 64; i++) {
            uint64_t re6 = (e/pow2[6] + e*pow2[26]) % mod32;
            uint64_t re11 = (e/pow2[11] + e*pow2[21]) % mod32;
            uint64_t re25 = (e/pow2[25] + e*pow2[7]) % mod32;
            uint64_t x1_and = (re6*re11) % mod32;
            uint64_t x1 = (re6+re11 - 2*x1_and + 2*mod32) % mod32;
            uint64_t x2_and = (x1*re25) % mod32;
            uint64_t S1 = (x1+re25 - 2*x2_and + 2*mod32) % mod32;

            uint64_t ef = (e*f) % mod32;
            uint64_t ne = (mod32-1-e) % mod32;
            uint64_t neg_val = (ne*g) % mod32;
            uint64_t ch_and = (ef*neg_val) % mod32;
            uint64_t ch = (ef+neg_val - 2*ch_and + 2*mod32) % mod32;

            uint64_t temp1 = (hh + S1 + ch + k[i] + w[i]) % mod32;

            uint64_t ra2 = (a/pow2[2] + a*pow2[30]) % mod32;
            uint64_t ra13 = (a/pow2[13] + a*pow2[19]) % mod32;
            uint64_t ra22 = (a/pow2[22] + a*pow2[10]) % mod32;
            uint64_t x3_and = (ra2*ra13) % mod32;
            uint64_t x3 = (ra2+ra13 - 2*x3_and + 2*mod32) % mod32;
            uint64_t x4_and = (x3*ra22) % mod32;
            uint64_t S0 = (x3+ra22 - 2*x4_and + 2*mod32) % mod32;

            uint64_t ab = (a*b) % mod32;
            uint64_t ac = (a*c) % mod32;
            uint64_t bc = (b*c) % mod32;
            uint64_t m1_and = (ab*ac) % mod32;
            uint64_t m1 = (ab+ac - 2*m1_and + 2*mod32) % mod32;
            uint64_t m2_and = (m1*bc) % mod32;
            uint64_t maj = (m1+bc - 2*m2_and + 2*mod32) % mod32;

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
