#include <stdio.h>
int main(void) {
    int mat[25], trans[25];
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) mat[i*5+j] = i*5+j+1;
    for (int i = 0; i < 5; i++)
        for (int j = 0; j < 5; j++) trans[j*5+i] = mat[i*5+j];
    int sum = 0;
    for (int i = 0; i < 25; i++) sum += mat[i] + trans[i];
    printf("%d\n", sum);
    return 0;
}
