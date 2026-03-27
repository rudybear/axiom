/*
 * axiom_rt_trace.c -- Execution trace recording for time-travel debugging.
 *
 * When AXIOM_RECORD_MODE is defined (via --record), these functions log
 * function entry/exit events to a JSONL file (one JSON object per line).
 * When AXIOM_RECORD_MODE is NOT defined, all functions are no-ops so there
 * is zero overhead in normal builds.
 *
 * Trace format (JSON Lines):
 *   {"type":"enter","func":"<name>","ns":<nanoseconds>}
 *   {"type":"exit","func":"<name>","ns":<nanoseconds>}
 */

#ifdef AXIOM_RECORD_MODE

static FILE *axiom_trace_file = NULL;

/* Get current time in nanoseconds (platform-specific). */
static long long axiom_trace_time_ns(void) {
#if defined(_WIN32)
    /* Use QueryPerformanceCounter on Windows for high-resolution timing. */
    static long long freq = 0;
    LARGE_INTEGER li;
    if (freq == 0) {
        LARGE_INTEGER f;
        QueryPerformanceFrequency(&f);
        freq = f.QuadPart;
    }
    QueryPerformanceCounter(&li);
    return (long long)((double)li.QuadPart / (double)freq * 1000000000.0);
#else
    struct timespec ts;
    timespec_get(&ts, TIME_UTC);
    return (long long)ts.tv_sec * 1000000000LL + (long long)ts.tv_nsec;
#endif
}

void axiom_trace_init(const char *output_path) {
    axiom_trace_file = fopen(output_path, "w");
}

void axiom_trace_enter(const char *func_name) {
    if (!axiom_trace_file) return;
    long long ns = axiom_trace_time_ns();
    fprintf(axiom_trace_file, "{\"type\":\"enter\",\"func\":\"%s\",\"ns\":%lld}\n",
            func_name, ns);
}

void axiom_trace_exit(const char *func_name) {
    if (!axiom_trace_file) return;
    long long ns = axiom_trace_time_ns();
    fprintf(axiom_trace_file, "{\"type\":\"exit\",\"func\":\"%s\",\"ns\":%lld}\n",
            func_name, ns);
}

void axiom_trace_close(void) {
    if (axiom_trace_file) {
        fclose(axiom_trace_file);
        axiom_trace_file = NULL;
    }
}

#else
/* No-op stubs when recording is disabled. */
void axiom_trace_init(const char *p) { (void)p; }
void axiom_trace_enter(const char *f) { (void)f; }
void axiom_trace_exit(const char *f) { (void)f; }
void axiom_trace_close(void) {}
#endif
