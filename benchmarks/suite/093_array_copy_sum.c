#include <stdio.h>
int main(void) {
    int src[100], dst[100];
    for (int i = 0; i < 100; i++) src[i] = i;
    for (int i = 0; i < 100; i++) dst[i] = src[i];
    int sum = 0;
    for (int i = 0; i < 100; i++) sum += dst[i];
    printf("%d\n", sum);
    return 0;
}
