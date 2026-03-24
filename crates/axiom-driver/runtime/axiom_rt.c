/*
 * axiom_rt.c -- Tiny C runtime for AXIOM I/O primitives.
 *
 * Provides file I/O, command-line arguments, and a nanosecond clock.
 * Linked only when the AXIOM program uses I/O builtins.
 */

#include <stdio.h>
#include <stdlib.h>
#include <time.h>

/* ── File I/O ─────────────────────────────────────────────────────── */

/* Read entire file into a malloc'd buffer.  Writes byte count to *out_size.
   Returns NULL on failure (and sets *out_size to 0). */
void *axiom_file_read(const char *path, long long *out_size) {
    FILE *f = fopen(path, "rb");
    if (!f) {
        *out_size = 0;
        return NULL;
    }
    fseek(f, 0, SEEK_END);
    long long sz = (long long)ftell(f);
    fseek(f, 0, SEEK_SET);
    void *buf = malloc((size_t)sz);
    if (!buf) {
        fclose(f);
        *out_size = 0;
        return NULL;
    }
    fread(buf, 1, (size_t)sz, f);
    fclose(f);
    *out_size = sz;
    return buf;
}

/* Write `len` bytes from `data` to file at `path` (binary mode). */
void axiom_file_write(const char *path, const void *data, long long len) {
    FILE *f = fopen(path, "wb");
    if (f) {
        fwrite(data, 1, (size_t)len, f);
        fclose(f);
    }
}

/* Return the size of the file in bytes, or -1 on error. */
long long axiom_file_size(const char *path) {
    FILE *f = fopen(path, "rb");
    if (!f) return -1;
    fseek(f, 0, SEEK_END);
    long long sz = (long long)ftell(f);
    fclose(f);
    return sz;
}

/* ── Command-line arguments ───────────────────────────────────────── */

static int    axiom_argc_val = 0;
static char **axiom_argv_val = NULL;

void axiom_set_args(int argc, char **argv) {
    axiom_argc_val = argc;
    axiom_argv_val = argv;
}

int axiom_get_argc(void) {
    return axiom_argc_val;
}

const char *axiom_get_argv(int i) {
    if (i >= 0 && i < axiom_argc_val && axiom_argv_val)
        return axiom_argv_val[i];
    return "";
}

/* ── Clock ────────────────────────────────────────────────────────── */

/* Return wall-clock time in nanoseconds (monotonic where possible). */
long long axiom_clock_ns(void) {
#if defined(_WIN32)
    /* On Windows, use QueryPerformanceCounter for high-resolution timing. */
    /* But to avoid pulling in <windows.h>, fall back to clock(). */
    return (long long)clock() * (1000000000LL / CLOCKS_PER_SEC);
#elif defined(__APPLE__)
    /* macOS: clock_gettime is available since 10.12 */
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000000000LL + (long long)ts.tv_nsec;
#else
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000000000LL + (long long)ts.tv_nsec;
#endif
}
