#include <stdio.h>
int main(void) {
    int mat[25];
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) mat[i*5+j] = i + j;
    int trace = 0;
    for (int i = 0; i < 5; i++) trace += mat[i*5+i];
    printf("%d\n", trace);
    return 0;
}
