#include <stdio.h>
static int julia(double zr, double zi, double cr, double ci, int max_iter) {
    for (int iter = 0; iter < max_iter; iter++) {
        double zr2 = zr * zr, zi2 = zi * zi;
        if (zr2 + zi2 > 4.0) return iter;
        double new_zr = zr2 - zi2 + cr;
        zi = 2.0 * zr * zi + ci;
        zr = new_zr;
    }
    return max_iter;
}
int main(void) { printf("%d\n", julia(0.5, 0.5, -0.7, 0.27015, 1000)); return 0; }
