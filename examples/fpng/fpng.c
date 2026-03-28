// fpng core -- C reference: PNG row filters + slice-by-4 CRC32
// Matches the AXIOM port's algorithm for comparison

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>

#define BPP 4  // RGBA
#define CRC32_POLY 0xEDB88320u

// ---------------------------------------------------------------------------
// CRC32 tables
// ---------------------------------------------------------------------------
static uint32_t crc32_tables[4][256];

static void build_crc32_tables_sb4(void) {
    for (int i = 0; i < 256; i++) {
        uint32_t crc = (uint32_t)i;
        for (int j = 0; j < 8; j++) {
            if (crc & 1)
                crc = (crc >> 1) ^ CRC32_POLY;
            else
                crc >>= 1;
        }
        crc32_tables[0][i] = crc;
    }
    for (int i = 0; i < 256; i++) {
        uint32_t c = crc32_tables[0][i];
        for (int t = 1; t < 4; t++) {
            c = crc32_tables[0][c & 0xFF] ^ (c >> 8);
            crc32_tables[t][i] = c;
        }
    }
}

static inline uint32_t crc32_basic(const uint8_t *data, int len) {
    uint32_t crc = 0xFFFFFFFF;
    for (int i = 0; i < len; i++) {
        crc = (crc >> 8) ^ crc32_tables[0][(crc ^ data[i]) & 0xFF];
    }
    return crc ^ 0xFFFFFFFF;
}

static inline uint32_t crc32_sb4(const uint8_t *data, int len) {
    uint32_t crc = 0xFFFFFFFF;
    int i = 0;
    int aligned = len & ~3;
    while (i < aligned) {
        uint32_t x0 = (crc ^ data[i]) & 0xFF;
        uint32_t x1 = ((crc >> 8) ^ data[i+1]) & 0xFF;
        uint32_t x2 = ((crc >> 16) ^ data[i+2]) & 0xFF;
        uint32_t x3 = ((crc >> 24) ^ data[i+3]) & 0xFF;
        crc = crc32_tables[3][x0] ^ crc32_tables[2][x1] ^
              crc32_tables[1][x2] ^ crc32_tables[0][x3];
        i += 4;
    }
    while (i < len) {
        crc = (crc >> 8) ^ crc32_tables[0][(crc ^ data[i]) & 0xFF];
        i++;
    }
    return crc ^ 0xFFFFFFFF;
}

// ---------------------------------------------------------------------------
// Paeth predictor
// ---------------------------------------------------------------------------
static inline int paeth_predictor(int a, int b, int c) {
    int p = a + b - c;
    int pa = abs(p - a);
    int pb = abs(p - b);
    int pc = abs(p - c);
    if (pa <= pb && pa <= pc) return a;
    if (pb <= pc) return b;
    return c;
}

// ---------------------------------------------------------------------------
// PNG Filters
// ---------------------------------------------------------------------------
static inline void filter_sub(const uint8_t *row, uint8_t *out, int wb) {
    for (int i = 0; i < BPP; i++) out[i] = row[i];
    for (int i = BPP; i < wb; i++) out[i] = (uint8_t)(row[i] - row[i - BPP]);
}

static inline void filter_up(const uint8_t *row, const uint8_t *prev, uint8_t *out, int wb) {
    for (int i = 0; i < wb; i++) out[i] = (uint8_t)(row[i] - prev[i]);
}

static inline void filter_average(const uint8_t *row, const uint8_t *prev, uint8_t *out, int wb) {
    for (int i = 0; i < BPP; i++) out[i] = (uint8_t)(row[i] - (prev[i] >> 1));
    for (int i = BPP; i < wb; i++)
        out[i] = (uint8_t)(row[i] - ((row[i-BPP] + prev[i]) >> 1));
}

static inline void filter_paeth(const uint8_t *row, const uint8_t *prev, uint8_t *out, int wb) {
    for (int i = 0; i < BPP; i++)
        out[i] = (uint8_t)(row[i] - paeth_predictor(0, prev[i], 0));
    for (int i = BPP; i < wb; i++)
        out[i] = (uint8_t)(row[i] - paeth_predictor(row[i-BPP], prev[i], prev[i-BPP]));
}

// Unfilters
static inline void unfilter_sub(const uint8_t *filt, uint8_t *out, int wb) {
    for (int i = 0; i < BPP; i++) out[i] = filt[i];
    for (int i = BPP; i < wb; i++) out[i] = (uint8_t)(filt[i] + out[i - BPP]);
}

static inline void unfilter_up(const uint8_t *filt, const uint8_t *prev, uint8_t *out, int wb) {
    for (int i = 0; i < wb; i++) out[i] = (uint8_t)(filt[i] + prev[i]);
}

static inline void unfilter_average(const uint8_t *filt, const uint8_t *prev, uint8_t *out, int wb) {
    for (int i = 0; i < BPP; i++) out[i] = (uint8_t)(filt[i] + (prev[i] >> 1));
    for (int i = BPP; i < wb; i++)
        out[i] = (uint8_t)(filt[i] + ((out[i-BPP] + prev[i]) >> 1));
}

static inline void unfilter_paeth(const uint8_t *filt, const uint8_t *prev, uint8_t *out, int wb) {
    for (int i = 0; i < BPP; i++)
        out[i] = (uint8_t)(filt[i] + paeth_predictor(0, prev[i], 0));
    for (int i = BPP; i < wb; i++)
        out[i] = (uint8_t)(filt[i] + paeth_predictor(out[i-BPP], prev[i], prev[i-BPP]));
}

static int sum_abs(const uint8_t *data, int len) {
    int sum = 0;
    for (int i = 0; i < len; i++) {
        int v = data[i];
        sum += (v > 127) ? (256 - v) : v;
    }
    return sum;
}

static int filter_image(const uint8_t *image, int width, int height,
                         uint8_t *filtered, uint8_t *scratch, uint8_t *prev_buf) {
    int wb = width * BPP;
    int total = 0;
    memset(prev_buf, 0, wb);

    for (int y = 0; y < height; y++) {
        const uint8_t *row = image + y * wb;
        int out_off = y * (wb + 1);
        int best = 0, best_sum = 0x7FFFFFFF;

        // None
        int s = sum_abs(row, wb);
        if (s < best_sum) { best_sum = s; best = 0; }

        // Sub
        filter_sub(row, scratch, wb);
        s = sum_abs(scratch, wb);
        if (s < best_sum) { best_sum = s; best = 1; }

        // Up
        filter_up(row, prev_buf, scratch, wb);
        s = sum_abs(scratch, wb);
        if (s < best_sum) { best_sum = s; best = 2; }

        // Paeth
        filter_paeth(row, prev_buf, scratch, wb);
        s = sum_abs(scratch, wb);
        if (s < best_sum) { best_sum = s; best = 4; }

        filtered[out_off] = (uint8_t)best;
        switch (best) {
            case 0: memcpy(filtered + out_off + 1, row, wb); break;
            case 1: filter_sub(row, filtered + out_off + 1, wb); break;
            case 2: filter_up(row, prev_buf, filtered + out_off + 1, wb); break;
            case 4: filter_paeth(row, prev_buf, filtered + out_off + 1, wb); break;
        }

        memcpy(prev_buf, row, wb);
        total += wb + 1;
    }
    return total;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
static int test_filters(void) {
    printf("--- Test: PNG filter round-trip ---\n");
    int wb = 32;
    uint8_t row[32], prev[32], filt[32], restored[32];

    for (int i = 0; i < wb; i++) {
        row[i] = (uint8_t)((i * 37 + 100) & 0xFF);
        prev[i] = (uint8_t)((i * 53 + 50) & 0xFF);
    }

    int pass = 1;

    filter_sub(row, filt, wb);
    unfilter_sub(filt, restored, wb);
    if (memcmp(row, restored, wb) != 0) { printf("FAIL: Sub\n"); pass = 0; }
    else printf("PASS: Sub filter round-trip\n");

    filter_up(row, prev, filt, wb);
    unfilter_up(filt, prev, restored, wb);
    if (memcmp(row, restored, wb) != 0) { printf("FAIL: Up\n"); pass = 0; }
    else printf("PASS: Up filter round-trip\n");

    filter_average(row, prev, filt, wb);
    unfilter_average(filt, prev, restored, wb);
    if (memcmp(row, restored, wb) != 0) { printf("FAIL: Average\n"); pass = 0; }
    else printf("PASS: Average filter round-trip\n");

    filter_paeth(row, prev, filt, wb);
    unfilter_paeth(filt, prev, restored, wb);
    if (memcmp(row, restored, wb) != 0) { printf("FAIL: Paeth\n"); pass = 0; }
    else printf("PASS: Paeth filter round-trip\n");

    return pass;
}

static int test_crc32(void) {
    printf("--- Test: CRC32 (basic + slice-by-4) ---\n");
    build_crc32_tables_sb4();

    const uint8_t *data = (const uint8_t *)"123456789";
    uint32_t rb = crc32_basic(data, 9);
    uint32_t rs = crc32_sb4(data, 9);

    int pass = 1;
    if (rb != 0xCBF43926) { printf("FAIL: basic = 0x%08X\n", rb); pass = 0; }
    else printf("PASS: CRC32 basic correct\n");

    if (rs != 0xCBF43926) { printf("FAIL: sb4 = 0x%08X\n", rs); pass = 0; }
    else printf("PASS: CRC32 slice-by-4 correct\n");

    uint8_t large[256];
    for (int i = 0; i < 256; i++) large[i] = (uint8_t)i;
    if (crc32_basic(large, 256) != crc32_sb4(large, 256)) {
        printf("FAIL: large data mismatch\n"); pass = 0;
    } else printf("PASS: CRC32 large data basic == slice-by-4\n");

    return pass;
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------
static void bench_filter(void) {
    printf("\n--- Benchmark: PNG filter 1024x1024 RGBA ---\n");
    int width = 1024, height = 1024;
    int wb = width * BPP;
    int img_size = wb * height;

    uint8_t *image = (uint8_t *)malloc(img_size);
    for (int y = 0; y < height; y++)
        for (int x = 0; x < width; x++) {
            int off = (y * width + x) * BPP;
            image[off] = (uint8_t)(x & 0xFF);
            image[off+1] = (uint8_t)(y & 0xFF);
            image[off+2] = (uint8_t)((x+y) & 0xFF);
            image[off+3] = 255;
        }

    int filt_size = (wb + 1) * height;
    uint8_t *filtered = (uint8_t *)malloc(filt_size);
    uint8_t *scratch = (uint8_t *)malloc(wb);
    uint8_t *prev_buf = (uint8_t *)malloc(wb);

    int iterations = 20;
    int checksum = 0;

    struct timespec ts0, ts1;
    clock_gettime(CLOCK_MONOTONIC, &ts0);
    for (int iter = 0; iter < iterations; iter++) {
        checksum += filter_image(image, width, height, filtered, scratch, prev_buf);
    }
    clock_gettime(CLOCK_MONOTONIC, &ts1);
    long ms = (ts1.tv_sec - ts0.tv_sec) * 1000 + (ts1.tv_nsec - ts0.tv_nsec) / 1000000;

    printf("Elapsed: %ld ms\n", ms);
    printf("Checksum (prevent DCE): %d\n", checksum);
    if (ms > 0) {
        long total_mb = (long)iterations * img_size / 1048576;
        printf("Throughput: %ld MB/s\n", total_mb * 1000 / ms);
    }

    free(image); free(filtered); free(scratch); free(prev_buf);
}

static void bench_crc32(void) {
    printf("\n--- Benchmark: CRC32 (basic vs slice-by-4) ---\n");
    build_crc32_tables_sb4();

    int data_len = 4096;
    uint8_t *data = (uint8_t *)malloc(data_len);
    for (int i = 0; i < data_len; i++) data[i] = (uint8_t)((i * 37 + 13) & 0xFF);

    int iterations = 200000;

    uint32_t cs_basic = 0;
    struct timespec ts0, ts1;
    clock_gettime(CLOCK_MONOTONIC, &ts0);
    for (int iter = 0; iter < iterations; iter++)
        cs_basic += crc32_basic(data, data_len);
    clock_gettime(CLOCK_MONOTONIC, &ts1);
    long ms_basic = (ts1.tv_sec - ts0.tv_sec) * 1000 + (ts1.tv_nsec - ts0.tv_nsec) / 1000000;

    printf("Basic CRC32: %ld ms", ms_basic);
    if (ms_basic > 0)
        printf(" (%ld MB/s)", (long)iterations * data_len * 1000 / (ms_basic * 1048576));
    printf("\n");

    uint32_t cs_sb4 = 0;
    struct timespec ts2, ts3;
    clock_gettime(CLOCK_MONOTONIC, &ts2);
    for (int iter = 0; iter < iterations; iter++)
        cs_sb4 += crc32_sb4(data, data_len);
    clock_gettime(CLOCK_MONOTONIC, &ts3);
    long ms_sb4 = (ts3.tv_sec - ts2.tv_sec) * 1000 + (ts3.tv_nsec - ts2.tv_nsec) / 1000000;

    printf("Slice-by-4:  %ld ms", ms_sb4);
    if (ms_sb4 > 0)
        printf(" (%ld MB/s)", (long)iterations * data_len * 1000 / (ms_sb4 * 1048576));
    printf("\n");

    printf("Checksum basic (prevent DCE): %u\n", cs_basic);
    printf("Checksum sb4 (prevent DCE): %u\n", cs_sb4);

    free(data);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
int main(void) {
    printf("=== fpng Core: PNG Filters + Fast CRC32 ===\n\n");

    build_crc32_tables_sb4();
    int pass1 = test_filters();
    int pass2 = test_crc32();

    if (pass1 && pass2)
        printf("\nAll tests passed.\n");
    else {
        printf("\nSome tests FAILED.\n");
        return 1;
    }

    bench_filter();
    bench_crc32();

    printf("\n=== fpng complete ===\n");
    return 0;
}
