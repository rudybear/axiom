#include <stdio.h>
int main(void) {
    int a[20], b[20];
    for (int i = 0; i < 20; i++) { a[i] = i + 1; b[i] = i + 1; }
    int dot = 0;
    for (int i = 0; i < 20; i++) dot += a[i] * b[i];
    printf("%d\n", dot);
    return 0;
}
