/*
 * axiom_rt_io.c -- File I/O runtime functions.
 *
 * Provides: axiom_file_read, axiom_file_write, axiom_file_size
 *
 * Included by axiom_rt.c -- do not compile separately.
 */

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
