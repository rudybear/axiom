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
int main(void) {
    int total = 0;
    for (int y = 0; y < 20; y++)
        for (int x = 0; x < 20; x++) {
            double cr = -2.0 + x * 0.15, ci = -1.5 + y * 0.15;
            total += mandelbrot(cr, ci, 100);
        }
    printf("%d\n", total);
    return 0;
}
