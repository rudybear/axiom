/*
 * QOI (Quite OK Image Format) reference implementation in C
 * For comparison with AXIOM port
 *
 * Spec: https://qoiformat.org/qoi-specification.pdf
 */

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
static uint64_t clock_ns_val(void) {
    LARGE_INTEGER freq, count;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&count);
    return (uint64_t)((double)count.QuadPart / freq.QuadPart * 1e9);
}
#else
#include <time.h>
static uint64_t clock_ns_val(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + ts.tv_nsec;
}
#endif

#define QOI_OP_INDEX 0x00
#define QOI_OP_DIFF  0x40
#define QOI_OP_LUMA  0x80
#define QOI_OP_RUN   0xC0
#define QOI_OP_RGB   0xFE
#define QOI_OP_RGBA  0xFF
#define QOI_MAGIC    0x716F6966
#define QOI_HEADER_SIZE 14
#define QOI_END_MARKER_SIZE 8

typedef struct { uint8_t r, g, b, a; } qoi_rgba_t;

static int qoi_color_hash(qoi_rgba_t c) {
    return (c.r * 3 + c.g * 5 + c.b * 7 + c.a * 11) % 64;
}

static void write_be32(uint8_t *buf, int offset, uint32_t val) {
    buf[offset]     = (val >> 24) & 0xFF;
    buf[offset + 1] = (val >> 16) & 0xFF;
    buf[offset + 2] = (val >> 8) & 0xFF;
    buf[offset + 3] = val & 0xFF;
}

static uint32_t read_be32(const uint8_t *buf, int offset) {
    return ((uint32_t)buf[offset] << 24) | ((uint32_t)buf[offset+1] << 16) |
           ((uint32_t)buf[offset+2] << 8) | (uint32_t)buf[offset+3];
}

static int qoi_encode(const uint8_t *pixels, int w, int h, uint8_t *out) {
    int total_pixels = w * h;
    qoi_rgba_t index[64];
    memset(index, 0, sizeof(index));

    /* Header */
    write_be32(out, 0, QOI_MAGIC);
    write_be32(out, 4, (uint32_t)w);
    write_be32(out, 8, (uint32_t)h);
    out[12] = 4; /* channels */
    out[13] = 0; /* colorspace */
    int p = QOI_HEADER_SIZE;

    qoi_rgba_t prev = {0, 0, 0, 255};
    int run = 0;

    for (int px_idx = 0; px_idx < total_pixels; px_idx++) {
        int off = px_idx * 4;
        qoi_rgba_t px = { pixels[off], pixels[off+1], pixels[off+2], pixels[off+3] };

        if (px.r == prev.r && px.g == prev.g && px.b == prev.b && px.a == prev.a) {
            run++;
            if (run == 62 || px_idx == total_pixels - 1) {
                out[p++] = QOI_OP_RUN | (run - 1);
                run = 0;
            }
        } else {
            if (run > 0) {
                out[p++] = QOI_OP_RUN | (run - 1);
                run = 0;
            }

            int hash_idx = qoi_color_hash(px);

            if (index[hash_idx].r == px.r && index[hash_idx].g == px.g &&
                index[hash_idx].b == px.b && index[hash_idx].a == px.a) {
                out[p++] = QOI_OP_INDEX | hash_idx;
            } else {
                index[hash_idx] = px;

                if (px.a == prev.a) {
                    int dr = (int)px.r - (int)prev.r;
                    int dg = (int)px.g - (int)prev.g;
                    int db = (int)px.b - (int)prev.b;

                    if (dr >= -2 && dr <= 1 && dg >= -2 && dg <= 1 && db >= -2 && db <= 1) {
                        out[p++] = QOI_OP_DIFF | ((dr+2)<<4) | ((dg+2)<<2) | (db+2);
                    } else {
                        int dr_dg = dr - dg;
                        int db_dg = db - dg;
                        if (dg >= -32 && dg <= 31 && dr_dg >= -8 && dr_dg <= 7 &&
                            db_dg >= -8 && db_dg <= 7) {
                            out[p++] = QOI_OP_LUMA | (dg + 32);
                            out[p++] = ((dr_dg + 8) << 4) | (db_dg + 8);
                        } else {
                            out[p++] = QOI_OP_RGB;
                            out[p++] = px.r;
                            out[p++] = px.g;
                            out[p++] = px.b;
                        }
                    }
                } else {
                    out[p++] = QOI_OP_RGBA;
                    out[p++] = px.r;
                    out[p++] = px.g;
                    out[p++] = px.b;
                    out[p++] = px.a;
                }
            }
            prev = px;
        }
    }

    /* End marker */
    for (int i = 0; i < 7; i++) out[p++] = 0;
    out[p++] = 1;

    return p;
}

static int qoi_decode(const uint8_t *data, int data_len, uint8_t *out_pixels) {
    uint32_t magic = read_be32(data, 0);
    if (magic != QOI_MAGIC) {
        printf("ERROR: Invalid QOI magic number\n");
        return 0;
    }

    int w = (int)read_be32(data, 4);
    int h = (int)read_be32(data, 8);
    int total_pixels = w * h;
    int p = QOI_HEADER_SIZE;

    qoi_rgba_t index[64];
    memset(index, 0, sizeof(index));

    qoi_rgba_t px = {0, 0, 0, 255};
    int run = 0;
    int px_written = 0;

    while (px_written < total_pixels) {
        if (run > 0) {
            run--;
        } else {
            uint8_t tag = data[p++];

            if (tag == QOI_OP_RGB) {
                px.r = data[p]; px.g = data[p+1]; px.b = data[p+2];
                p += 3;
            } else if (tag == QOI_OP_RGBA) {
                px.r = data[p]; px.g = data[p+1]; px.b = data[p+2]; px.a = data[p+3];
                p += 4;
            } else if ((tag & 0xC0) == QOI_OP_INDEX) {
                px = index[tag & 0x3F];
            } else if ((tag & 0xC0) == QOI_OP_DIFF) {
                px.r += ((tag >> 4) & 0x03) - 2;
                px.g += ((tag >> 2) & 0x03) - 2;
                px.b += (tag & 0x03) - 2;
            } else if ((tag & 0xC0) == QOI_OP_LUMA) {
                int dg = (tag & 0x3F) - 32;
                uint8_t next = data[p++];
                int dr_dg = ((next >> 4) & 0x0F) - 8;
                int db_dg = (next & 0x0F) - 8;
                px.r += dg + dr_dg;
                px.g += dg;
                px.b += dg + db_dg;
            } else {
                run = tag & 0x3F;
            }

            index[qoi_color_hash(px)] = px;
        }

        int out_off = px_written * 4;
        out_pixels[out_off]   = px.r;
        out_pixels[out_off+1] = px.g;
        out_pixels[out_off+2] = px.b;
        out_pixels[out_off+3] = px.a;
        px_written++;
    }

    return px_written;
}

static void generate_test_image(uint8_t *pixels, int w, int h) {
    for (int y = 0; y < h; y++) {
        for (int x = 0; x < w; x++) {
            int off = (y * w + x) * 4;
            pixels[off]   = (uint8_t)(x & 0xFF);      /* R */
            pixels[off+1] = (uint8_t)(y & 0xFF);      /* G */
            pixels[off+2] = (uint8_t)((x+y)/2 & 0xFF); /* B */
            pixels[off+3] = 255;                        /* A */
        }
    }
}

int main(void) {
    int w = 256, h = 256;
    int total_pixels = w * h;
    int raw_size = total_pixels * 4;

    printf("=== QOI Image Codec Test (C reference) ===\n");
    printf("Image size: 256x256 RGBA\n");

    uint8_t *pixels = (uint8_t *)malloc(raw_size);
    int max_encoded = QOI_HEADER_SIZE + total_pixels * 5 + QOI_END_MARKER_SIZE;
    uint8_t *encoded = (uint8_t *)malloc(max_encoded);
    uint8_t *decoded = (uint8_t *)malloc(raw_size);

    printf("Generating test image...\n");
    generate_test_image(pixels, w, h);

    printf("Encoding with QOI...\n");
    int encoded_size = qoi_encode(pixels, w, h, encoded);
    printf("Raw size:     %d bytes\n", raw_size);
    printf("Encoded size: %d bytes\n", encoded_size);
    printf("Compression ratio: %d.%02dx\n", raw_size * 100 / encoded_size / 100,
           (raw_size * 100 / encoded_size) % 100);

    printf("Decoding QOI...\n");
    int decoded_pixels = qoi_decode(encoded, encoded_size, decoded);
    printf("Decoded pixels: %d\n", decoded_pixels);

    printf("Verifying round-trip...\n");
    int errors = 0;
    for (int i = 0; i < total_pixels * 4; i++) {
        if (pixels[i] != decoded[i]) {
            errors++;
            if (errors <= 5) {
                printf("  Mismatch at byte %d: original=%d decoded=%d\n",
                       i, pixels[i], decoded[i]);
            }
        }
    }
    if (errors == 0)
        printf("PASS: Round-trip verified, all pixels match!\n");
    else
        printf("FAIL: %d byte mismatches found\n", errors);

    /* Benchmark */
    int bench_iters = 1000;
    printf("\nBenchmark: %d encode/decode cycles\n", bench_iters);

    uint64_t start_enc = clock_ns_val();
    uint64_t enc_checksum = 0;
    for (int i = 0; i < bench_iters; i++) {
        int sz = qoi_encode(pixels, w, h, encoded);
        enc_checksum += sz;
    }
    uint64_t end_enc = clock_ns_val();
    uint64_t enc_ns = end_enc - start_enc;

    uint64_t start_dec = clock_ns_val();
    uint64_t dec_checksum = 0;
    for (int i = 0; i < bench_iters; i++) {
        int npx = qoi_decode(encoded, encoded_size, decoded);
        dec_checksum += npx;
    }
    uint64_t end_dec = clock_ns_val();
    uint64_t dec_ns = end_dec - start_dec;

    uint64_t total_bytes = (uint64_t)raw_size * bench_iters;
    printf("Encode: %lld ns, ~%lld MB/s\n", (long long)enc_ns,
           (long long)(total_bytes * 1000 / enc_ns));
    printf("Decode: %lld ns, ~%lld MB/s\n", (long long)dec_ns,
           (long long)(total_bytes * 1000 / dec_ns));
    printf("Encode checksum: %lld\n", (long long)enc_checksum);
    printf("Decode checksum: %lld\n", (long long)dec_checksum);

    free(pixels);
    free(encoded);
    free(decoded);

    printf("\n=== QOI test complete ===\n");
    return 0;
}
