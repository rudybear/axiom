#include <stdio.h>
static int pascal(int row, int col) {
    if (col == 0 || col == row) return 1;
    return pascal(row-1, col-1) + pascal(row-1, col);
}
int main(void) { printf("%d\n", pascal(5, 2)); return 0; }
