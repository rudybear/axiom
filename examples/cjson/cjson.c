// cJSON parser core -- C reference implementation
// Matches the AXIOM port's counting/checksum approach

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <time.h>

typedef struct {
    int pos;
    int obj_count, arr_count, str_count, num_count, bool_count, null_count;
    int total;
    int error;
    int checksum;
    int max_depth, cur_depth;
} ParseState;

static void state_init(ParseState *s) {
    memset(s, 0, sizeof(*s));
}

static inline int is_ws(uint8_t c) {
    return c == ' ' || c == '\t' || c == '\n' || c == '\r';
}

static inline int is_digit(uint8_t c) {
    return c >= '0' && c <= '9';
}

static void skip_ws(const uint8_t *json, ParseState *s, int len) {
    while (s->pos < len && is_ws(json[s->pos])) s->pos++;
}

static void parse_value(const uint8_t *json, ParseState *s, int len);

static void parse_string(const uint8_t *json, ParseState *s, int len) {
    if (s->pos >= len || json[s->pos] != '"') { s->error = 1; return; }
    s->pos++; // skip opening quote
    while (s->pos < len) {
        uint8_t c = json[s->pos];
        if (c == '"') { s->pos++; return; }
        if (c == '\\') s->pos += 2;
        else s->pos++;
    }
    s->error = 1;
}

static void parse_number(const uint8_t *json, ParseState *s, int len) {
    int sign = 1, value = 0, has_digits = 0;
    if (s->pos < len && json[s->pos] == '-') { sign = -1; s->pos++; }
    while (s->pos < len && is_digit(json[s->pos])) {
        value = value * 10 + (json[s->pos] - '0');
        s->pos++;
        has_digits = 1;
    }
    if (!has_digits) { s->error = 1; return; }
    if (s->pos < len && json[s->pos] == '.') {
        s->pos++;
        while (s->pos < len && is_digit(json[s->pos])) s->pos++;
    }
    if (s->pos < len && (json[s->pos] == 'e' || json[s->pos] == 'E')) {
        s->pos++;
        if (s->pos < len && (json[s->pos] == '+' || json[s->pos] == '-')) s->pos++;
        while (s->pos < len && is_digit(json[s->pos])) s->pos++;
    }
    s->checksum += value * sign;
}

static void parse_object(const uint8_t *json, ParseState *s, int len) {
    if (s->pos >= len || json[s->pos] != '{') { s->error = 1; return; }
    s->pos++;
    s->cur_depth++;
    if (s->cur_depth > s->max_depth) s->max_depth = s->cur_depth;
    s->obj_count++;
    s->total++;
    skip_ws(json, s, len);

    if (s->pos < len && json[s->pos] == '}') {
        s->pos++;
        s->cur_depth--;
        return;
    }

    while (!s->error) {
        skip_ws(json, s, len);
        if (s->pos >= len || json[s->pos] != '"') { s->error = 1; break; }
        parse_string(json, s, len);
        skip_ws(json, s, len);
        if (s->pos >= len || json[s->pos] != ':') { s->error = 1; break; }
        s->pos++;
        skip_ws(json, s, len);
        parse_value(json, s, len);
        skip_ws(json, s, len);
        if (s->pos >= len) { s->error = 1; break; }
        if (json[s->pos] == ',') { s->pos++; continue; }
        if (json[s->pos] == '}') { s->pos++; break; }
        s->error = 1;
    }
    s->cur_depth--;
}

static void parse_array(const uint8_t *json, ParseState *s, int len) {
    if (s->pos >= len || json[s->pos] != '[') { s->error = 1; return; }
    s->pos++;
    s->cur_depth++;
    if (s->cur_depth > s->max_depth) s->max_depth = s->cur_depth;
    s->arr_count++;
    s->total++;
    skip_ws(json, s, len);

    if (s->pos < len && json[s->pos] == ']') {
        s->pos++;
        s->cur_depth--;
        return;
    }

    while (!s->error) {
        skip_ws(json, s, len);
        parse_value(json, s, len);
        skip_ws(json, s, len);
        if (s->pos >= len) { s->error = 1; break; }
        if (json[s->pos] == ',') { s->pos++; continue; }
        if (json[s->pos] == ']') { s->pos++; break; }
        s->error = 1;
    }
    s->cur_depth--;
}

static void parse_value(const uint8_t *json, ParseState *s, int len) {
    skip_ws(json, s, len);
    if (s->pos >= len) { s->error = 1; return; }
    uint8_t c = json[s->pos];

    if (c == '{') { parse_object(json, s, len); return; }
    if (c == '[') { parse_array(json, s, len); return; }
    if (c == '"') { parse_string(json, s, len); s->str_count++; s->total++; return; }
    if (is_digit(c) || c == '-') { parse_number(json, s, len); s->num_count++; s->total++; return; }
    if (s->pos + 4 <= len && memcmp(json + s->pos, "true", 4) == 0) {
        s->pos += 4; s->bool_count++; s->total++; return;
    }
    if (s->pos + 5 <= len && memcmp(json + s->pos, "false", 5) == 0) {
        s->pos += 5; s->bool_count++; s->total++; return;
    }
    if (s->pos + 4 <= len && memcmp(json + s->pos, "null", 4) == 0) {
        s->pos += 4; s->null_count++; s->total++; return;
    }
    s->error = 1;
}

static int build_test_json(uint8_t *buf) {
    const char *json = "{\"name\":\"AXIOM\",\"version\":42,\"active\":true,"
                       "\"tags\":[\"fast\",\"ai\",\"llvm\"],"
                       "\"config\":{\"opt_level\":3,\"debug\":false},"
                       "\"data\":null,\"scores\":[100,200,300]}";
    int len = (int)strlen(json);
    memcpy(buf, json, len);
    return len;
}

static int build_bench_json(uint8_t *buf, int repeats) {
    uint8_t one_doc[512];
    int one_len = build_test_json(one_doc);
    int p = 0;
    buf[p++] = '[';
    for (int r = 0; r < repeats; r++) {
        if (r > 0) buf[p++] = ',';
        memcpy(buf + p, one_doc, one_len);
        p += one_len;
    }
    buf[p++] = ']';
    return p;
}

int main(void) {
    printf("=== cJSON C Reference ===\n");

    uint8_t json_buf[1024];
    int json_len = build_test_json(json_buf);
    printf("JSON length: %d bytes\n", json_len);

    ParseState s;
    state_init(&s);
    parse_value(json_buf, &s, json_len);

    printf("Parse: %s\n", s.error ? "ERROR" : "OK");
    printf("Total: %d, Checksum: %d\n", s.total, s.checksum);
    if (!s.error && s.total == 16 && s.checksum == 645)
        printf("PASS: Correct parse\n");
    else
        printf("FAIL\n");

    // Benchmark
    int repeats = 10;
    uint8_t *bench_json = (uint8_t *)malloc(8192);
    int bench_len = build_bench_json(bench_json, repeats);
    int iterations = 100000;

    printf("\nBenchmark: %d byte JSON x %d parses\n", bench_len, iterations);

    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int total_values = 0;

    for (int i = 0; i < iterations; i++) {
        state_init(&s);
        parse_value(bench_json, &s, bench_len);
        total_values += s.total;
    }

    clock_gettime(CLOCK_MONOTONIC, &t1);
    long elapsed_ms = (t1.tv_sec - t0.tv_sec) * 1000 +
                      (t1.tv_nsec - t0.tv_nsec) / 1000000;

    printf("Elapsed: %ld ms\n", elapsed_ms);
    printf("Total values: %d\n", total_values);
    if (elapsed_ms > 0) {
        long total_mb = ((long)iterations * bench_len) / 1048576;
        printf("Throughput: %ld MB/s\n", total_mb * 1000 / elapsed_ms);
    }

    free(bench_json);
    printf("=== cJSON C complete ===\n");
    return 0;
}
