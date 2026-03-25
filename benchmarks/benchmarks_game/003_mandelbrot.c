#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

static int mandelbrot_pixel(double cr, double ci, int max_iter) {
    double zr = 0.0, zi = 0.0;
    for (int iter = 0; iter < max_iter; iter++) {
        double zr2 = zr * zr;
        double zi2 = zi * zi;
        if (zr2 + zi2 > 4.0) return 0;
        double new_zr = zr2 - zi2 + cr;
        zi = 2.0 * zr * zi + ci;
        zr = new_zr;
    }
    return 1;
}

int main(void) {
    int width = 4000, height = 4000, max_iter = 50;
    int row_bytes = width / 8;
    int total_bytes = height * row_bytes;

    int *bitmap = (int *)calloc(total_bytes, sizeof(int));

    double inv_w = 2.0 / (double)width;
    double inv_h = 2.0 / (double)height;

    for (int py = 0; py < height; py++) {
        double ci = (double)py * inv_h - 1.0;
        for (int px_byte = 0; px_byte < row_bytes; px_byte++) {
            int byte_val = 0;
            for (int bit = 0; bit < 8; bit++) {
                int px = px_byte * 8 + bit;
                double cr = (double)px * inv_w - 1.5;
                int inside = mandelbrot_pixel(cr, ci, max_iter);
                byte_val = byte_val * 2 + inside;
            }
            bitmap[py * row_bytes + px_byte] = byte_val;
        }
    }

    int64_t checksum = 0;
    for (int i = 0; i < total_bytes; i++) {
        checksum += (int64_t)bitmap[i];
    }

    printf("%lld\n", (long long)checksum);
    free(bitmap);
    return 0;
}
