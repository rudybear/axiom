#include <stdio.h>
static int tak(int x, int y, int z) {
    if (y >= x) return z;
    return tak(tak(x-1,y,z), tak(y-1,z,x), tak(z-1,x,y));
}
int main(void) { printf("%d\n", tak(18, 12, 6)); return 0; }
