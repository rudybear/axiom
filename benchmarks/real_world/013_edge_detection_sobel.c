#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

static int image[262144], edges[262144];

int main(void) {
    int width = 512, height = 512, total = 262144;

    int64_t seed = 42, lcg_a = 1103515245LL, lcg_c = 12345LL, lcg_m = 2147483648LL;
    for (int i = 0; i < total; i++) {
        seed = (lcg_a * seed + lcg_c) % lcg_m;
        image[i] = (int)(seed % 256);
    }

    for (int y = 0; y < height; y++) {
        for (int x = 0; x < width; x++) {
            int idx = y*width + x;
            if (y % 32 < 4) image[idx] = 200;
            if (x % 64 < 4) image[idx] = 180;
            int dx = x-256, dy = y-256;
            if (dx*dx+dy*dy > 14400 && dx*dx+dy*dy < 16900) image[idx] = 240;
        }
    }

    int64_t edge_sum = 0;
    int max_edge = 0;

    for (int y = 1; y < height-1; y++) {
        for (int x = 1; x < width-1; x++) {
            int p00=image[(y-1)*width+(x-1)], p01=image[(y-1)*width+x], p02=image[(y-1)*width+(x+1)];
            int p10=image[y*width+(x-1)], p12=image[y*width+(x+1)];
            int p20=image[(y+1)*width+(x-1)], p21=image[(y+1)*width+x], p22=image[(y+1)*width+(x+1)];

            int gx = -p00+p02-2*p10+2*p12-p20+p22;
            int gy = -p00-2*p01-p02+p20+2*p21+p22;
            int mag = abs(gx)+abs(gy);
            if (mag > 255) mag = 255;

            edges[y*width+x] = mag;
            edge_sum += mag;
            if (mag > max_edge) max_edge = mag;
        }
    }

    int threshold = 50, above_count = 0;
    for (int i = 0; i < total; i++) if (edges[i] > threshold) above_count++;

    int hist[16] = {0};
    for (int i = 0; i < total; i++) { int bin = edges[i]/16; if (bin>15) bin=15; hist[bin]++; }

    int64_t hist_sum = 0;
    for (int i = 0; i < 16; i++) hist_sum += (int64_t)hist[i] * (i+1);

    int64_t checksum = edge_sum + (int64_t)max_edge*10000 + above_count + hist_sum;
    printf("%lld\n", (long long)checksum);
    return 0;
}
