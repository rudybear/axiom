#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

static int image[262144], output[262144];

#define CAS(a,b) if(w[a]>w[b]){int t=w[a];w[a]=w[b];w[b]=t;}

int main(void) {
    int width=512, height=512, total=262144;

    int64_t seed=54321, lcg_a=1103515245LL, lcg_c=12345LL, lcg_m=2147483648LL;
    for (int i = 0; i < total; i++) {
        seed = (lcg_a*seed+lcg_c) % lcg_m;
        int y = i/width, x = i%width;
        int base = (x+y)/4; if (base>255) base=255;
        int64_t noise = seed%100;
        if (noise < 5) image[i] = 0;
        else if (noise < 10) image[i] = 255;
        else image[i] = base;
    }

    for (int y = 1; y < height-1; y++) {
        for (int x = 1; x < width-1; x++) {
            int w[9];
            w[0]=image[(y-1)*width+(x-1)]; w[1]=image[(y-1)*width+x]; w[2]=image[(y-1)*width+(x+1)];
            w[3]=image[y*width+(x-1)]; w[4]=image[y*width+x]; w[5]=image[y*width+(x+1)];
            w[6]=image[(y+1)*width+(x-1)]; w[7]=image[(y+1)*width+x]; w[8]=image[(y+1)*width+(x+1)];

            CAS(0,1); CAS(3,4); CAS(6,7); CAS(1,2); CAS(4,5); CAS(7,8);
            CAS(0,1); CAS(3,4); CAS(6,7);
            CAS(0,3); CAS(1,4); CAS(2,5); CAS(3,6); CAS(4,7); CAS(5,8);
            CAS(1,3); CAS(2,6); CAS(2,3); CAS(5,7);
            CAS(4,6); CAS(3,4); CAS(4,5); CAS(5,6);

            output[y*width+x] = w[4];
        }
    }

    for (int x = 0; x < width; x++) { output[x]=image[x]; output[(height-1)*width+x]=image[(height-1)*width+x]; }
    for (int y = 0; y < height; y++) { output[y*width]=image[y*width]; output[y*width+width-1]=image[y*width+width-1]; }

    int64_t checksum = 0;
    for (int i = 0; i < total; i++) checksum += output[i];
    int64_t diff_sum = 0;
    for (int i = 0; i < total; i++) diff_sum += abs(output[i]-image[i]);
    checksum = checksum*100 + diff_sum;
    printf("%lld\n", (long long)checksum);
    return 0;
}
