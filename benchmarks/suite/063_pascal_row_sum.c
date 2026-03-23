#include <stdio.h>
static int pascal(int row, int col) {
    if (col == 0 || col == row) return 1;
    return pascal(row-1, col-1) + pascal(row-1, col);
}
int main(void) {
    int sum = 0;
    for (int c = 0; c <= 10; c++) sum += pascal(10, c);
    printf("%d\n", sum);
    return 0;
}
