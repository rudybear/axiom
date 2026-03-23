#include <stdio.h>
int main(void) {
    int data[50], hist[5] = {0};
    for (int i = 0; i < 50; i++) data[i] = i % 5;
    for (int i = 0; i < 50; i++) hist[data[i]]++;
    printf("%d\n", hist[0]);
    return 0;
}
