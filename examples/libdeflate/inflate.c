// libdeflate-style fast DEFLATE inflator -- C reference implementation
// Compile: gcc -O3 -march=native -ffast-math -o inflate_c inflate.c

#include <stdio.h>
#include <stdint.h>
#include <string.h>
#ifdef _WIN32
#include <windows.h>
static uint64_t clock_ns_func(void) {
    LARGE_INTEGER freq, cnt;
    QueryPerformanceFrequency(&freq);
    QueryPerformanceCounter(&cnt);
    return (uint64_t)((double)cnt.QuadPart / freq.QuadPart * 1e9);
}
#else
#include <time.h>
static uint64_t clock_ns_func(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000000000ULL + ts.tv_nsec;
}
#endif

#define FAST_BITS 9
#define FAST_MASK 511
#define TABLE_SIZE 512

typedef struct {
    int byte_pos;
    uint32_t bit_buf;
    int bits_left;
    int input_len;
} BitReader;

static void br_init(BitReader *br, int len) {
    br->byte_pos = 0; br->bit_buf = 0; br->bits_left = 0; br->input_len = len;
}

static void br_refill(BitReader *br, const uint8_t *input) {
    while (br->bits_left <= 24 && br->byte_pos < br->input_len) {
        br->bit_buf |= (uint32_t)input[br->byte_pos++] << br->bits_left;
        br->bits_left += 8;
    }
}

static int br_read(BitReader *br, const uint8_t *input, int n) {
    br_refill(br, input);
    int r = br->bit_buf & ((1 << n) - 1);
    br->bit_buf >>= n; br->bits_left -= n;
    return r;
}

static int br_peek(BitReader *br, int n) {
    return br->bit_buf & ((1 << n) - 1);
}

static void br_consume(BitReader *br, int n) {
    br->bit_buf >>= n; br->bits_left -= n;
}

static int reverse_bits(int val, int nbits) {
    int r = 0;
    for (int i = 0; i < nbits; i++) { r = (r << 1) | (val & 1); val >>= 1; }
    return r;
}

static void build_fixed_lit_table(int *table) {
    memset(table, 0, TABLE_SIZE * sizeof(int));
    for (int s = 0; s < 144; s++) {
        int rev = reverse_bits(48 + s, 8);
        table[rev & FAST_MASK] = (s << 4) | 8;
    }
    for (int s = 144; s < 256; s++) {
        int rev = reverse_bits(400 + (s-144), 9);
        table[rev & FAST_MASK] = (s << 4) | 9;
    }
    for (int s = 256; s < 280; s++) {
        int rev = reverse_bits(s - 256, 7);
        for (int step = 0; step < 4; step++)
            table[(rev | (step << 7)) & FAST_MASK] = (s << 4) | 7;
    }
    for (int s = 280; s < 288; s++) {
        int rev = reverse_bits(192 + (s-280), 8);
        table[rev & FAST_MASK] = (s << 4) | 8;
        table[(rev | (1 << 8)) & FAST_MASK] = (s << 4) | 8;
    }
}

static void build_fixed_dist_table(int *table) {
    memset(table, 0, TABLE_SIZE * sizeof(int));
    for (int s = 0; s < 30; s++) {
        int rev = reverse_bits(s, 5);
        for (int step = 0; step < 16; step++)
            table[(rev | (step << 5)) & FAST_MASK] = (s << 4) | 5;
    }
}

static const int len_base[] = {3,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258};
static const int len_extra[] = {0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,0};
static const int dist_base[] = {1,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,1025,1537,2049,3073,4097,6145,8193,12289,16385,24577};
static const int dist_extra[] = {0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13};

static int decode_symbol(BitReader *br, const uint8_t *input, const int *table) {
    br_refill(br, input);
    int entry = table[br_peek(br, FAST_BITS) & FAST_MASK];
    br_consume(br, entry & 15);
    return entry >> 4;
}

static int inflate_fixed_block(const uint8_t *input, int input_len,
                                uint8_t *output, int max_output,
                                BitReader *br, const int *lit_table, const int *dist_table) {
    int out_pos = 0;
    while (1) {
        int sym = decode_symbol(br, input, lit_table);
        if (sym < 256) {
            if (out_pos < max_output) output[out_pos++] = (uint8_t)sym;
        } else if (sym == 256) {
            break;
        } else {
            int li = sym - 257;
            int length = len_base[li];
            if (len_extra[li]) length += br_read(br, input, len_extra[li]);
            int ds = decode_symbol(br, input, dist_table);
            int distance = dist_base[ds];
            if (dist_extra[ds]) distance += br_read(br, input, dist_extra[ds]);
            int src = out_pos - distance;
            for (int j = 0; j < length && out_pos < max_output; j++)
                output[out_pos++] = output[src + j];
        }
    }
    return out_pos;
}

static int inflate_data(const uint8_t *input, int input_len,
                        uint8_t *output, int max_output) {
    BitReader br;
    br_init(&br, input_len);
    int lit_table[TABLE_SIZE], dist_table[TABLE_SIZE];
    build_fixed_lit_table(lit_table);
    build_fixed_dist_table(dist_table);

    int total_out = 0, last = 0;
    while (!last) {
        last = br_read(&br, input, 1);
        int btype = br_read(&br, input, 2);
        if (btype == 0) {
            br.bit_buf = 0; br.bits_left = 0;
            int lo = br_read(&br, input, 8), hi = br_read(&br, input, 8);
            int blen = lo | (hi << 8);
            br_read(&br, input, 16);
            for (int j = 0; j < blen && total_out < max_output; j++)
                output[total_out++] = input[br.byte_pos + j];
            br.byte_pos += blen;
        } else if (btype == 1) {
            total_out += inflate_fixed_block(input, input_len, output + total_out,
                                             max_output - total_out, &br, lit_table, dist_table);
        }
    }
    return total_out;
}

static int deflate_fixed(const uint8_t *input, int input_len,
                         uint8_t *output, int max_output) {
    int out_pos = 0, bit_buf = 3, bits_used = 3;
    for (int i = 0; i < input_len; i++) {
        int b = input[i], code, code_len;
        if (b < 144) { code = 48 + b; code_len = 8; }
        else { code = 400 + (b - 144); code_len = 9; }
        int rev = reverse_bits(code, code_len);
        bit_buf |= rev << bits_used;
        bits_used += code_len;
        while (bits_used >= 8) {
            if (out_pos < max_output) output[out_pos++] = bit_buf & 0xFF;
            bit_buf >>= 8; bits_used -= 8;
        }
    }
    int eob = reverse_bits(0, 7);
    bit_buf |= eob << bits_used;
    bits_used += 7;
    while (bits_used > 0) {
        if (out_pos < max_output) output[out_pos++] = bit_buf & 0xFF;
        bit_buf >>= 8; bits_used -= 8;
    }
    return out_pos;
}

int main(void) {
    const char *test = "Hello, World! Hello, World!";
    int test_len = 27;

    uint8_t compressed[256], decompressed[256];
    int comp_len = deflate_fixed((const uint8_t*)test, test_len, compressed, 256);
    printf("Compressed %d bytes -> %d bytes\n", test_len, comp_len);

    int decomp_len = inflate_data(compressed, comp_len, decompressed, 256);
    printf("Decompressed -> %d bytes\n", decomp_len);

    int pass = (decomp_len == test_len) && (memcmp(decompressed, test, test_len) == 0);
    printf("%s: round-trip verified\n", pass ? "PASS" : "FAIL");

    // Benchmark
    uint8_t big_data[1024], big_comp[2048], big_decomp[2048];
    for (int i = 0; i < 1024; i++) big_data[i] = (uint8_t)((i*7+13) & 0xFF);
    int big_comp_len = deflate_fixed(big_data, 1024, big_comp, 2048);
    printf("Big test: 1024 bytes -> %d compressed bytes\n", big_comp_len);

    int iterations = 100000, checksum = 0;
    uint64_t start = clock_ns_func();
    for (int i = 0; i < iterations; i++) {
        int n = inflate_data(big_comp, big_comp_len, big_decomp, 2048);
        checksum += n;
    }
    uint64_t elapsed = clock_ns_func() - start;
    printf("Benchmark: %d inflate operations\n", iterations);
    printf("Elapsed (ns): %llu\n", (unsigned long long)elapsed);
    printf("Checksum: %d\n", checksum);
    if (elapsed > 0) {
        uint64_t total = (uint64_t)iterations * 1024;
        printf("Throughput: ~%llu MB/s\n", (unsigned long long)(total * 1000 / elapsed));
    }
    return 0;
}
