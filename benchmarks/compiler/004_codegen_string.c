#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

static int emit_opcode(int *buf, int pos, int opcode) {
    if (opcode == 0) {
        buf[pos]=97; buf[pos+1]=100; buf[pos+2]=100;
        return 3;
    }
    if (opcode == 1) {
        buf[pos]=115; buf[pos+1]=117; buf[pos+2]=98;
        return 3;
    }
    if (opcode == 2) {
        buf[pos]=109; buf[pos+1]=117; buf[pos+2]=108;
        return 3;
    }
    if (opcode == 3) {
        buf[pos]=108; buf[pos+1]=111; buf[pos+2]=97; buf[pos+3]=100;
        return 4;
    }
    if (opcode == 4) {
        buf[pos]=115; buf[pos+1]=116; buf[pos+2]=111; buf[pos+3]=114; buf[pos+4]=101;
        return 5;
    }
    if (opcode == 5) {
        buf[pos]=99; buf[pos+1]=97; buf[pos+2]=108; buf[pos+3]=108;
        return 4;
    }
    if (opcode == 6) {
        buf[pos]=98; buf[pos+1]=114;
        return 2;
    }
    if (opcode == 7) {
        buf[pos]=114; buf[pos+1]=101; buf[pos+2]=116;
        return 3;
    }
    buf[pos]=105; buf[pos+1]=99; buf[pos+2]=109; buf[pos+3]=112;
    return 4;
}

static int emit_register(int *buf, int pos, int reg_num) {
    buf[pos] = 37;
    buf[pos+1] = 114;
    buf[pos+2] = 48 + (reg_num / 10) % 10;
    buf[pos+3] = 48 + reg_num % 10;
    buf[pos+4] = 32;
    return 5;
}

int main(void) {
    int n_instructions = 2000000;
    int buf_size = n_instructions * 16;
    int *buf = (int *)calloc(buf_size, sizeof(int));

    int64_t seed = 42;
    int pos = 0;

    for (int i = 0; i < n_instructions; i++) {
        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        int opcode = (int)(seed % 9);

        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        int dst_reg = (int)(seed % 100);
        int written = emit_register(buf, pos, dst_reg);
        pos += written;

        buf[pos] = 61; buf[pos+1] = 32;
        pos += 2;

        int op_len = emit_opcode(buf, pos, opcode);
        pos += op_len;

        buf[pos] = 32;
        pos += 1;

        seed = (1103515245LL * seed + 12345LL) % 2147483648LL;
        int src_reg = (int)(seed % 100);
        written = emit_register(buf, pos, src_reg);
        pos += written;

        buf[pos] = 10;
        pos += 1;
    }

    int64_t checksum = 0;
    for (int i = 0; i < pos; i++) {
        checksum = (checksum * 31 + (int64_t)buf[i]) % 1000000007LL;
    }

    printf("%lld\n", (long long)checksum);
    free(buf);
    return 0;
}
