// JPEG 8x8 IDCT -- C reference implementation (AAN algorithm from stb_image)
// Compile: gcc -O3 -march=native -ffast-math -o idct_c idct.c

#include <stdio.h>
#include <stdint.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
static uint64_t clock_ns(void) {
    LARGE_INTEGER freq, cnt;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&cnt);
    return (uint64_t)((double)cnt.QuadPart / freq.QuadPart * 1e9);
}
#else
#include <time.h>
static uint64_t clock_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + ts.tv_nsec;
}
#endif

// Fixed-point constants (12-bit precision)
#define FIX_0_298631336  1223
#define FIX_0_390180644  1598
#define FIX_0_541196100  2217
#define FIX_0_765366865  3135
#define FIX_0_899976223  3686
#define FIX_1_175875602  4816
#define FIX_1_501321110  6149
#define FIX_1_847759065  7568
#define FIX_1_961570560  8034
#define FIX_2_053119869  8410
#define FIX_2_562915447  10497
#define FIX_3_072711026  12585

#define SHIFT_BITS 12
#define DESCALE_COL 10
#define HALF_COL 512
#define DESCALE_ROW 17
#define HALF_ROW 65536

static inline int clamp_u8(int x) {
    if (x < 0) return 0;
    if (x > 255) return 255;
    return x;
}

static void idct_col(int32_t *data, int col) {
    int s0 = data[col + 0];
    int s1 = data[col + 8];
    int s2 = data[col + 16];
    int s3 = data[col + 24];
    int s4 = data[col + 32];
    int s5 = data[col + 40];
    int s6 = data[col + 48];
    int s7 = data[col + 56];

    if (!s1 && !s2 && !s3 && !s4 && !s5 && !s6 && !s7) {
        int dc = s0 << 2;
        data[col+0] = data[col+8] = data[col+16] = data[col+24] =
        data[col+32] = data[col+40] = data[col+48] = data[col+56] = dc;
        return;
    }

    int p2 = s2, p3 = s6;
    int p1 = (p2 + p3) * FIX_0_541196100;
    int t2 = p1 + p3 * (-FIX_1_847759065);
    int t3 = p1 + p2 * FIX_0_765366865;

    int t0 = (s0 + s4) << SHIFT_BITS;
    int t1 = (s0 - s4) << SHIFT_BITS;

    int x0 = t0 + t3, x3 = t0 - t3;
    int x1 = t1 + t2, x2 = t1 - t2;

    int t0b = s7, t1b = s5, t2b = s3, t3b = s1;
    int p3c = t0b + t2b, p4 = t1b + t3b;
    int p1c = t0b + t3b, p2c = t1b + t2b;
    int p5 = (p3c + p4) * FIX_1_175875602;

    t0b *= FIX_0_298631336; t1b *= FIX_2_053119869;
    t2b *= FIX_3_072711026; t3b *= FIX_1_501321110;
    p1c *= -FIX_0_899976223; p2c *= -FIX_2_562915447;
    p3c *= -FIX_1_961570560; p4 *= -FIX_0_390180644;
    p3c += p5; p4 += p5;

    int y0 = t0b + p1c + p3c, y1 = t1b + p2c + p4;
    int y2 = t2b + p2c + p3c, y3 = t3b + p1c + p4;

    data[col+0]  = (x0 + y3 + HALF_COL) >> DESCALE_COL;
    data[col+8]  = (x1 + y2 + HALF_COL) >> DESCALE_COL;
    data[col+16] = (x2 + y1 + HALF_COL) >> DESCALE_COL;
    data[col+24] = (x3 + y0 + HALF_COL) >> DESCALE_COL;
    data[col+32] = (x3 - y0 + HALF_COL) >> DESCALE_COL;
    data[col+40] = (x2 - y1 + HALF_COL) >> DESCALE_COL;
    data[col+48] = (x1 - y2 + HALF_COL) >> DESCALE_COL;
    data[col+56] = (x0 - y3 + HALF_COL) >> DESCALE_COL;
}

static void idct_row(int32_t *data, int row, uint8_t *out, int out_row) {
    int base = row * 8;
    int s0=data[base], s1=data[base+1], s2=data[base+2], s3=data[base+3];
    int s4=data[base+4], s5=data[base+5], s6=data[base+6], s7=data[base+7];

    if (!s1 && !s2 && !s3 && !s4 && !s5 && !s6 && !s7) {
        uint8_t dc = clamp_u8((s0 + HALF_ROW) >> DESCALE_ROW);
        int ob = out_row * 8;
        out[ob]=out[ob+1]=out[ob+2]=out[ob+3]=
        out[ob+4]=out[ob+5]=out[ob+6]=out[ob+7]=dc;
        return;
    }

    int p2=s2, p3=s6;
    int p1 = (p2+p3)*FIX_0_541196100;
    int t2 = p1 + p3*(-FIX_1_847759065);
    int t3 = p1 + p2*FIX_0_765366865;
    int t0 = (s0+s4)<<SHIFT_BITS, t1 = (s0-s4)<<SHIFT_BITS;
    int x0=t0+t3, x3=t0-t3, x1=t1+t2, x2=t1-t2;

    int t0b=s7, t1b=s5, t2b=s3, t3b=s1;
    int p3c=t0b+t2b, p4=t1b+t3b, p1c=t0b+t3b, p2c=t1b+t2b;
    int p5 = (p3c+p4)*FIX_1_175875602;
    t0b *= FIX_0_298631336; t1b *= FIX_2_053119869;
    t2b *= FIX_3_072711026; t3b *= FIX_1_501321110;
    p1c *= -FIX_0_899976223; p2c *= -FIX_2_562915447;
    p3c *= -FIX_1_961570560; p4 *= -FIX_0_390180644;
    p3c += p5; p4 += p5;
    int y0=t0b+p1c+p3c, y1=t1b+p2c+p4, y2=t2b+p2c+p3c, y3=t3b+p1c+p4;

    int ob = out_row * 8;
    out[ob+0] = clamp_u8((x0+y3+HALF_ROW)>>DESCALE_ROW);
    out[ob+1] = clamp_u8((x1+y2+HALF_ROW)>>DESCALE_ROW);
    out[ob+2] = clamp_u8((x2+y1+HALF_ROW)>>DESCALE_ROW);
    out[ob+3] = clamp_u8((x3+y0+HALF_ROW)>>DESCALE_ROW);
    out[ob+4] = clamp_u8((x3-y0+HALF_ROW)>>DESCALE_ROW);
    out[ob+5] = clamp_u8((x2-y1+HALF_ROW)>>DESCALE_ROW);
    out[ob+6] = clamp_u8((x1-y2+HALF_ROW)>>DESCALE_ROW);
    out[ob+7] = clamp_u8((x0-y3+HALF_ROW)>>DESCALE_ROW);
}

static void idct_8x8(int32_t *coefs, uint8_t *out, int32_t *temp) {
    memcpy(temp, coefs, 64 * sizeof(int32_t));
    for (int c = 0; c < 8; c++) idct_col(temp, c);
    for (int r = 0; r < 8; r++) idct_row(temp, r, out, r);
}

int main(void) {
    int32_t coefs[64] = {0};
    coefs[0] = 1024; coefs[1] = -30; coefs[8] = 50;
    coefs[2] = 20; coefs[9] = -10; coefs[16] = 15;

    uint8_t out[64];
    int32_t temp[64];
    idct_8x8(coefs, out, temp);

    printf("IDCT 8x8 Test Output (first row):\n");
    for (int i = 0; i < 8; i++) printf("%d ", out[i]);
    printf("\n");

    int center = out[0];
    int pass = (center >= 100 && center <= 170);
    for (int i = 0; i < 64; i++) if (out[i] > 255) pass = 0;
    printf("%s: IDCT output in expected range\n", pass ? "PASS" : "FAIL");

    // DC-only test
    memset(coefs, 0, sizeof(coefs));
    coefs[0] = 1024;
    idct_8x8(coefs, out, temp);
    printf("DC-only test: pixel[0] = %d (expected ~128)\n", out[0]);
    printf("%s: DC-only correct\n", (out[0] >= 126 && out[0] <= 130) ? "PASS" : "FAIL");

    // Benchmark
    int iterations = 1000000;
    int checksum = 0;
    coefs[0] = 1024; coefs[1] = -30; coefs[8] = 50;
    coefs[2] = 20; coefs[9] = -10; coefs[16] = 15;

    uint64_t start = clock_ns();
    for (int i = 0; i < iterations; i++) {
        coefs[0] = 1024 + (i % 64);
        idct_8x8(coefs, out, temp);
        checksum += out[0];
    }
    uint64_t end = clock_ns();
    uint64_t elapsed = end - start;

    printf("Benchmark: %d IDCT-8x8 transforms\n", iterations);
    printf("Elapsed (ns): %llu\n", (unsigned long long)elapsed);
    printf("Checksum: %d\n", checksum);
    if (elapsed > 0)
        printf("ns/op: %llu\n", (unsigned long long)(elapsed / iterations));
    return 0;
}
