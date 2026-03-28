// Heatshrink LZSS compression -- C reference implementation
// Compile: gcc -O3 -o heatshrink_c heatshrink.c

#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <stdlib.h>
#include <time.h>

#define WINDOW_BITS 8
#define LOOKAHEAD_BITS 4
#define WINDOW_SIZE (1 << WINDOW_BITS)
#define LOOKAHEAD_SIZE (1 << LOOKAHEAD_BITS)
#define MIN_MATCH_LEN 2

// Bit writer
typedef struct {
    uint8_t *buf;
    int byte_pos;
    int bit_pos;
    uint8_t cur_byte;
} BitWriter;

void bw_init(BitWriter *bw, uint8_t *buf) {
    bw->buf = buf; bw->byte_pos = 0; bw->bit_pos = 0; bw->cur_byte = 0;
}

void bw_write(BitWriter *bw, int value, int count) {
    for (int i = count - 1; i >= 0; i--) {
        int bit = (value >> i) & 1;
        bw->cur_byte |= (bit << (7 - bw->bit_pos));
        bw->bit_pos++;
        if (bw->bit_pos >= 8) {
            bw->buf[bw->byte_pos++] = bw->cur_byte;
            bw->bit_pos = 0;
            bw->cur_byte = 0;
        }
    }
}

int bw_flush(BitWriter *bw) {
    if (bw->bit_pos > 0) {
        bw->buf[bw->byte_pos++] = bw->cur_byte;
        bw->bit_pos = 0;
        bw->cur_byte = 0;
    }
    return bw->byte_pos;
}

// Bit reader
typedef struct {
    const uint8_t *buf;
    int byte_pos;
    int bit_pos;
} BitReader;

void br_init(BitReader *br, const uint8_t *buf) {
    br->buf = buf; br->byte_pos = 0; br->bit_pos = 0;
}

int br_read(BitReader *br, int count) {
    int result = 0;
    for (int i = 0; i < count; i++) {
        int bit = (br->buf[br->byte_pos] >> (7 - br->bit_pos)) & 1;
        result = (result << 1) | bit;
        br->bit_pos++;
        if (br->bit_pos >= 8) {
            br->bit_pos = 0;
            br->byte_pos++;
        }
    }
    return result;
}

// Find best match
void find_best_match(const uint8_t *input, int input_len, int pos,
                     int *out_offset, int *out_length) {
    int best_len = 0, best_off = 0;
    int search_start = pos - WINDOW_SIZE;
    if (search_start < 0) search_start = 0;

    for (int scan = search_start; scan < pos; scan++) {
        int match_len = 0;
        int max_len = LOOKAHEAD_SIZE;
        if (pos + max_len > input_len) max_len = input_len - pos;
        while (match_len < max_len && input[scan + match_len] == input[pos + match_len])
            match_len++;
        if (match_len >= MIN_MATCH_LEN && match_len > best_len) {
            best_len = match_len;
            best_off = pos - scan;
        }
    }
    *out_offset = best_off;
    *out_length = best_len;
}

int compress(const uint8_t *input, int input_len, uint8_t *output) {
    BitWriter bw;
    bw_init(&bw, output);
    int pos = 0;
    while (pos < input_len) {
        int off, len;
        find_best_match(input, input_len, pos, &off, &len);
        if (len >= MIN_MATCH_LEN) {
            bw_write(&bw, 1, 1);
            bw_write(&bw, off - 1, WINDOW_BITS);
            bw_write(&bw, len - MIN_MATCH_LEN, LOOKAHEAD_BITS);
            pos += len;
        } else {
            bw_write(&bw, 0, 1);
            bw_write(&bw, input[pos], 8);
            pos++;
        }
    }
    return bw_flush(&bw);
}

int decompress(const uint8_t *input, int input_bits, uint8_t *output,
               int max_output, int expected_len) {
    BitReader br;
    br_init(&br, input);
    int out_pos = 0;
    while (out_pos < expected_len) {
        int tag = br_read(&br, 1);
        if (tag == 0) {
            int byte_val = br_read(&br, 8);
            if (out_pos < max_output) output[out_pos] = (uint8_t)byte_val;
            out_pos++;
        } else {
            int offset = br_read(&br, WINDOW_BITS) + 1;
            int length = br_read(&br, LOOKAHEAD_BITS) + MIN_MATCH_LEN;
            for (int j = 0; j < length; j++) {
                if (out_pos < max_output) output[out_pos] = output[out_pos - offset];
                out_pos++;
            }
        }
    }
    return out_pos;
}

void fill_repeating(uint8_t *buf, int len) {
    for (int i = 0; i < len; i++)
        buf[i] = 'A' + (i % 3);
}

void fill_mixed(uint8_t *buf, int len) {
    for (int i = 0; i < len; i++) {
        int section = i / 32;
        int offset = i % 32;
        if (section % 2 == 0) buf[i] = 'A' + (offset % 4);
        else buf[i] = (uint8_t)((i * 37 + 13) & 0xFF);
    }
}

int main(void) {
    printf("=== Heatshrink C Reference ===\n");
    printf("Window: %d bytes, Lookahead: %d bytes\n", WINDOW_SIZE, LOOKAHEAD_SIZE);

    // Test 1
    int test_len = 256;
    uint8_t *test_data = malloc(test_len);
    fill_repeating(test_data, test_len);
    uint8_t *comp = malloc(test_len * 2);
    uint8_t *decomp = malloc(test_len);

    printf("\n--- Test 1: Repeating pattern (256 bytes) ---\n");
    int csz = compress(test_data, test_len, comp);
    printf("Original: %d, Compressed: %d, Ratio: %d%%\n", test_len, csz, csz*100/test_len);
    int dsz = decompress(comp, csz*8, decomp, test_len, test_len);
    printf("Round-trip: %s\n", (dsz==test_len && memcmp(test_data,decomp,test_len)==0) ? "PASS" : "FAIL");
    free(test_data); free(comp); free(decomp);

    // Test 2
    int test_len2 = 512;
    uint8_t *test_data2 = malloc(test_len2);
    fill_mixed(test_data2, test_len2);
    uint8_t *comp2 = malloc(test_len2 * 2);
    uint8_t *decomp2 = malloc(test_len2);

    printf("\n--- Test 2: Mixed pattern (512 bytes) ---\n");
    int csz2 = compress(test_data2, test_len2, comp2);
    printf("Original: %d, Compressed: %d, Ratio: %d%%\n", test_len2, csz2, csz2*100/test_len2);
    int dsz2 = decompress(comp2, csz2*8, decomp2, test_len2, test_len2);
    printf("Round-trip: %s\n", (dsz2==test_len2 && memcmp(test_data2,decomp2,test_len2)==0) ? "PASS" : "FAIL");
    free(test_data2); free(comp2); free(decomp2);

    // Benchmark
    int bench_len = 1024;
    uint8_t *bench_data = malloc(bench_len);
    fill_repeating(bench_data, bench_len);
    uint8_t *bench_comp = malloc(bench_len * 2);
    uint8_t *bench_decomp = malloc(bench_len);
    int bench_iters = 10000;

    printf("\n--- Benchmark: 1KB x 10K compress/decompress ---\n");
    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    uint32_t checksum = 0;
    for (int i = 0; i < bench_iters; i++) {
        int c = compress(bench_data, bench_len, bench_comp);
        int d = decompress(bench_comp, c*8, bench_decomp, bench_len, bench_len);
        checksum += c + d;
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    long elapsed_ms = (t1.tv_sec-t0.tv_sec)*1000 + (t1.tv_nsec-t0.tv_nsec)/1000000;
    printf("Elapsed: %ld ms\n", elapsed_ms);
    printf("Checksum: %u\n", checksum);
    if (elapsed_ms > 0) {
        long total_kb = ((long)bench_iters * bench_len * 2) / 1024;
        printf("Throughput: %ld KB/s\n", (total_kb * 1000) / elapsed_ms);
    }

    free(bench_data); free(bench_comp); free(bench_decomp);
    printf("\n=== Heatshrink C complete ===\n");
    return 0;
}
