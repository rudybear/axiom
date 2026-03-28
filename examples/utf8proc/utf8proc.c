// UTF-8 processing -- C reference implementation
// Compile: gcc -O3 -march=native -ffast-math -o utf8proc_c utf8proc.c

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
static uint64_t clock_ns(void) {
    LARGE_INTEGER freq, cnt;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&cnt);
    return (uint64_t)((double)cnt.QuadPart / freq.QuadPart * 1e9);
}
#else
#include <time.h>
static uint64_t clock_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + ts.tv_nsec;
}
#endif

// UTF-8 decode
static int utf8_decode(const uint8_t *data, int pos, int data_len, int *out_len) {
    if (pos >= data_len) { *out_len = 0; return -1; }
    uint8_t b0 = data[pos];
    if (b0 < 128) { *out_len = 1; return b0; }
    if ((b0 & 0xE0) == 0xC0) {
        if (pos+1 >= data_len) { *out_len = 1; return -1; }
        uint8_t b1 = data[pos+1];
        if ((b1 & 0xC0) != 0x80) { *out_len = 1; return -1; }
        int cp = ((b0 & 0x1F) << 6) | (b1 & 0x3F);
        if (cp < 0x80) { *out_len = 2; return -1; }
        *out_len = 2; return cp;
    }
    if ((b0 & 0xF0) == 0xE0) {
        if (pos+2 >= data_len) { *out_len = 1; return -1; }
        uint8_t b1 = data[pos+1], b2 = data[pos+2];
        if ((b1&0xC0)!=0x80 || (b2&0xC0)!=0x80) { *out_len = 1; return -1; }
        int cp = ((b0&0x0F)<<12) | ((b1&0x3F)<<6) | (b2&0x3F);
        if (cp < 0x800) { *out_len = 3; return -1; }
        if (cp >= 0xD800 && cp <= 0xDFFF) { *out_len = 3; return -1; }
        *out_len = 3; return cp;
    }
    if ((b0 & 0xF8) == 0xF0) {
        if (pos+3 >= data_len) { *out_len = 1; return -1; }
        uint8_t b1=data[pos+1], b2=data[pos+2], b3=data[pos+3];
        if ((b1&0xC0)!=0x80||(b2&0xC0)!=0x80||(b3&0xC0)!=0x80) { *out_len=1; return -1; }
        int cp = ((b0&0x07)<<18)|((b1&0x3F)<<12)|((b2&0x3F)<<6)|(b3&0x3F);
        if (cp < 0x10000 || cp > 0x10FFFF) { *out_len = 4; return -1; }
        *out_len = 4; return cp;
    }
    *out_len = 1; return -1;
}

static int utf8_validate(const uint8_t *data, int data_len) {
    int pos = 0;
    while (pos < data_len) {
        int consumed;
        int cp = utf8_decode(data, pos, data_len, &consumed);
        if (cp == -1 || consumed <= 0) return 0;
        pos += consumed;
    }
    return 1;
}

static int is_ascii_letter(int cp) {
    return (cp >= 'A' && cp <= 'Z') || (cp >= 'a' && cp <= 'z') ||
           (cp >= 0xC0 && cp <= 0x024F && cp != 0xD7 && cp != 0xF7) ||
           (cp >= 0x0370 && cp <= 0x03FF) || (cp >= 0x0400 && cp <= 0x04FF) ||
           (cp >= 0x4E00 && cp <= 0x9FFF);
}
static int is_digit(int cp) { return cp >= '0' && cp <= '9'; }
static int is_whitespace(int cp) {
    return cp==32||cp==9||cp==10||cp==13||cp==12||cp==0x0B||
           cp==0xA0||cp==0x2000||cp==0x2001||cp==0x2002||cp==0x2003||
           cp==0x2028||cp==0x2029||cp==0x3000;
}

static int case_fold(int cp) {
    if (cp >= 'A' && cp <= 'Z') return cp + 32;
    if (cp >= 0xC0 && cp <= 0xDE && cp != 0xD7) return cp + 32;
    if (cp >= 0x0391 && cp <= 0x03A9 && cp != 0x03A2) return cp + 32;
    if (cp >= 0x0410 && cp <= 0x042F) return cp + 32;
    return cp;
}

static int utf8_encode(int cp, uint8_t *out, int pos) {
    if (cp < 0x80) { out[pos] = cp; return 1; }
    if (cp < 0x800) { out[pos] = 0xC0|(cp>>6); out[pos+1] = 0x80|(cp&0x3F); return 2; }
    if (cp < 0x10000) { out[pos]=0xE0|(cp>>12); out[pos+1]=0x80|((cp>>6)&0x3F); out[pos+2]=0x80|(cp&0x3F); return 3; }
    out[pos]=0xF0|(cp>>18); out[pos+1]=0x80|((cp>>12)&0x3F); out[pos+2]=0x80|((cp>>6)&0x3F); out[pos+3]=0x80|(cp&0x3F); return 4;
}

static int utf8_process(const uint8_t *data, int data_len,
                        uint8_t *folded_out, int max_out, int *stats) {
    stats[0]=stats[1]=stats[2]=stats[3]=stats[4]=0;
    int pos=0, out_pos=0, codepoints=0;
    while (pos < data_len) {
        int consumed;
        int cp = utf8_decode(data, pos, data_len, &consumed);
        if (cp == -1 || consumed <= 0) return -1;
        if (is_ascii_letter(cp)) stats[0]++;
        else if (is_digit(cp)) stats[1]++;
        else if (is_whitespace(cp)) stats[2]++;
        else stats[3]++;
        int folded = case_fold(cp);
        if (out_pos < max_out - 4)
            out_pos += utf8_encode(folded, folded_out, out_pos);
        codepoints++;
        pos += consumed;
    }
    stats[4] = out_pos;
    return codepoints;
}

int main(void) {
    // Test 1: ASCII
    const uint8_t test1[] = "Hello, World! 123";
    int test1_len = 17;
    printf("Test 1 (ASCII): valid=%d %s\n", utf8_validate(test1, test1_len),
           utf8_validate(test1, test1_len) ? "PASS" : "FAIL");

    // Test 2: Multi-byte (cafe with e-acute)
    const uint8_t test2[] = {0x63, 0x61, 0x66, 0xC3, 0xA9};
    printf("Test 2 (multi-byte): valid=%d %s\n", utf8_validate(test2, 5),
           utf8_validate(test2, 5) ? "PASS" : "FAIL");

    // Test 3: Invalid
    const uint8_t test3[] = {0xC3, 0x28};
    printf("Test 3 (invalid): valid=%d %s\n", utf8_validate(test3, 2),
           !utf8_validate(test3, 2) ? "PASS (correctly rejected)" : "FAIL");

    // Test 4: Classification
    uint8_t fold_out[64];
    int stats[5];
    int cp_count = utf8_process(test1, test1_len, fold_out, 64, stats);
    printf("Test 4 (process): codepoints=%d, letters=%d, digits=%d, whitespace=%d, other=%d\n",
           cp_count, stats[0], stats[1], stats[2], stats[3]);
    printf("%s: classification correct\n",
           (cp_count==17 && stats[0]==10 && stats[1]==3 && stats[2]==2 && stats[3]==2) ? "PASS":"FAIL");
    printf("Case fold 'H' -> %d %s\n", fold_out[0], fold_out[0]==104?"PASS":"FAIL");

    // Test 5: Euro sign
    const uint8_t test5[] = {0xE2, 0x82, 0xAC};
    int out_len;
    int euro_cp = utf8_decode(test5, 0, 3, &out_len);
    printf("Test 5 (euro sign): codepoint=%d %s\n", euro_cp, euro_cp==0x20AC?"PASS":"FAIL");

    // Benchmark
    int big_len = 1048576;
    uint8_t *big_data = malloc(big_len);
    for (int i = 0; i < big_len; i++) big_data[i] = (uint8_t)(i % 94 + 32);
    uint8_t *big_fold = malloc(big_len + 256);
    int big_stats[5];

    int iterations = 50;
    uint64_t start = clock_ns();
    int check = 0;
    for (int iter = 0; iter < iterations; iter++)
        check += utf8_validate(big_data, big_len);
    uint64_t validate_ns = clock_ns() - start;
    printf("Benchmark: validate 1MB x %d iterations\n", iterations);
    printf("Validate elapsed (ns): %llu\n", (unsigned long long)validate_ns);
    printf("Validate check: %d\n", check);

    uint64_t start2 = clock_ns();
    int check2 = 0;
    for (int iter = 0; iter < iterations; iter++)
        check2 += utf8_process(big_data, big_len, big_fold, big_len+256, big_stats);
    uint64_t process_ns = clock_ns() - start2;
    printf("Process elapsed (ns): %llu\n", (unsigned long long)process_ns);
    printf("Process check: %d\n", check2);

    if (validate_ns > 0)
        printf("Validate throughput: ~%llu MB/s\n", (unsigned long long)(iterations*1000000000ULL/validate_ns));
    if (process_ns > 0)
        printf("Process throughput: ~%llu MB/s\n", (unsigned long long)(iterations*1000000000ULL/process_ns));

    free(big_data);
    free(big_fold);
    return 0;
}
