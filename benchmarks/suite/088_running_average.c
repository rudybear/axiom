#include <stdio.h>
int main(void) {
    int arr[100];
    for (int i = 0; i < 100; i++) arr[i] = i;
    double sum = 0.0;
    for (int i = 0; i < 100; i++) sum += (double)arr[i];
    printf("%f\n", sum / 100.0);
    return 0;
}
