#include <stdio.h>
static int mandelbrot(double cr, double ci, int max_iter) {
    double zr = 0.0, zi = 0.0;
    for (int iter = 0; iter < max_iter; iter++) {
        double zr2 = zr * zr, zi2 = zi * zi;
        if (zr2 + zi2 > 4.0) return iter;
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
    }
    return max_iter;
}
int main(void) { printf("%d\n", mandelbrot(0.5, 0.5, 1000)); return 0; }
