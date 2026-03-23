#include <stdio.h>
#include <math.h>
#include <stdint.h>

int main(void) {
    int nblocks = 50000;
    double pi = 3.14159265358979;

    double cos_table[64];
    for (int u = 0; u < 8; u++) {
        for (int x = 0; x < 8; x++) {
            double angle = pi * (double)(2*x+1) * (double)u / 16.0;
            double a = angle;
            double twopi = 2.0 * pi;
            a = a - (double)(int)(a/twopi) * twopi;
            if (a > pi) a -= twopi;
            if (a < -pi) a += twopi;
            double a2 = a*a;
            cos_table[u*8+x] = 1.0 - a2/2.0 + a2*a2/24.0 - a2*a2*a2/720.0 + a2*a2*a2*a2/40320.0;
        }
    }

    double c0 = 1.0 / sqrt(2.0);
    double block_in[64], block_out[64];

    int64_t seed = 42, lcg_a = 1103515245LL, lcg_c = 12345LL, lcg_m = 2147483648LL;
    double total_dc = 0.0, total_energy = 0.0;

    for (int blk = 0; blk < nblocks; blk++) {
        for (int i = 0; i < 64; i++) {
            seed = (lcg_a*seed+lcg_c) % lcg_m;
            block_in[i] = (double)(seed % 256) - 128.0;
        }

        for (int u = 0; u < 8; u++) {
            for (int v = 0; v < 8; v++) {
                double sum = 0.0;
                for (int x = 0; x < 8; x++)
                    for (int y = 0; y < 8; y++)
                        sum += block_in[x*8+y] * cos_table[u*8+x] * cos_table[v*8+y];
                double cu = (u==0) ? c0 : 1.0;
                double cv = (v==0) ? c0 : 1.0;
                block_out[u*8+v] = 0.25 * cu * cv * sum;
            }
        }

        total_dc += block_out[0];
        for (int i = 0; i < 64; i++) total_energy += block_out[i]*block_out[i];
    }

    int quant[64] = {
        16,11,10,16,24,40,51,61,12,12,14,19,26,58,60,55,
        14,13,16,24,40,57,69,56,14,17,22,29,51,87,80,62,
        18,22,37,56,68,109,103,77,24,35,55,64,81,104,113,92,
        49,64,78,87,103,121,120,101,72,92,95,98,112,100,103,99
    };

    int nonzero_count = 0;
    for (int i = 0; i < 64; i++) {
        int quantized = (int)(block_out[i] / (double)quant[i]);
        if (quantized != 0) nonzero_count++;
    }

    double checksum = total_dc + total_energy / 1000000.0 + (double)nonzero_count;
    printf("%.6f\n", checksum);
    return 0;
}
