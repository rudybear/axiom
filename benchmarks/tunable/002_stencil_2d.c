#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

static void stencil_step(double *src, double *dst, int nx, int ny, int block_x, int block_y) {
    int num_bx = (nx - 2) / block_x;
    int num_by = (ny - 2) / block_y;

    for (int bj = 0; bj < num_by; bj++) {
        for (int bi = 0; bi < num_bx; bi++) {
            int i_start = bi * block_x + 1;
            int j_start = bj * block_y + 1;
            int i_end = i_start + block_x;
            int j_end = j_start + block_y;

            for (int i = i_start; i < i_end; i++) {
                for (int j = j_start; j < j_end; j++) {
                    double center = src[i * ny + j];
                    double north = src[(i-1) * ny + j];
                    double south = src[(i+1) * ny + j];
                    double west = src[i * ny + (j-1)];
                    double east = src[i * ny + (j+1)];
                    dst[i * ny + j] = (center + north + south + west + east) / 5.0;
                }
            }
        }
    }

    /* Handle remainder cells */
    int covered_x = num_bx * block_x + 1;
    int covered_y = num_by * block_y + 1;
    for (int i = 1; i < nx - 1; i++) {
        for (int j = 1; j < ny - 1; j++) {
            if (i < covered_x && j < covered_y && i >= 1 && j >= 1) continue;
            double center = src[i * ny + j];
            double north = src[(i-1) * ny + j];
            double south = src[(i+1) * ny + j];
            double west = src[i * ny + (j-1)];
            double east = src[i * ny + (j+1)];
            dst[i * ny + j] = (center + north + south + west + east) / 5.0;
        }
    }
}

int main(void) {
    int nx = 2000, ny = 2000, n_iter = 100;
    int block_x = 64, block_y = 64;
    int total = nx * ny;

    double *grid_a = (double *)calloc(total, sizeof(double));
    double *grid_b = (double *)calloc(total, sizeof(double));

    for (int i = 0; i < nx; i++) {
        for (int j = 0; j < ny; j++) {
            double dx = (double)(i - nx/2);
            double dy = (double)(j - ny/2);
            if (dx*dx + dy*dy < 10000.0)
                grid_a[i * ny + j] = 100.0;
        }
    }

    for (int iter = 0; iter < n_iter; iter++) {
        if (iter % 2 == 0)
            stencil_step(grid_a, grid_b, nx, ny, block_x, block_y);
        else
            stencil_step(grid_b, grid_a, nx, ny, block_x, block_y);
    }

    double checksum = 0.0;
    for (int idx = 0; idx < total / 97; idx++) {
        checksum += grid_a[idx * 97];
    }

    printf("%.6f\n", checksum);
    free(grid_a); free(grid_b);
    return 0;
}
