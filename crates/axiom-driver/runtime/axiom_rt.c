/*
 * axiom_rt.c -- Tiny C runtime for AXIOM I/O primitives, coroutines,
 * threading primitives, a parallel job dispatch system, and a stub
 * rendering API (Vulkan FFI / Lux shader loading infrastructure).
 *
 * Provides file I/O, command-line arguments, a nanosecond clock,
 * stackful coroutines via OS fibers (Windows) or ucontext (POSIX),
 * thread creation/join, atomics, mutexes, a thread-pool job system,
 * and a renderer stub API designed for future Vulkan implementation.
 * Linked only when the AXIOM program uses runtime builtins.
 */

#define _CRT_SECURE_NO_WARNINGS
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <string.h>

#if !defined(_WIN32)
#include <unistd.h>
#endif

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

/* ── Coroutines ──────────────────────────────────────────────────── */
/*
 * Stackful coroutines for game logic.  Each coroutine gets its own stack
 * and can yield/resume cooperatively.
 *
 * On Windows: implemented via Win32 fibers (CreateFiber / SwitchToFiber).
 * On POSIX:   implemented via ucontext (makecontext / swapcontext).
 *
 * API:
 *   axiom_coro_create(func, arg)  -> handle (i32)
 *   axiom_coro_resume(handle)     -> yielded value (i32), or -1 if done
 *   axiom_coro_yield(value)       -> suspends, resumes caller
 *   axiom_coro_is_done(handle)    -> 1 if finished, 0 if still alive
 *   axiom_coro_destroy(handle)    -> frees resources
 */

#define AXIOM_CORO_MAX   64
#define AXIOM_CORO_STACK (64 * 1024)  /* 64 KB per coroutine stack */

typedef void (*AxiomCoroFunc)(int);

typedef struct {
    int          active;         /* slot is in use */
    int          done;           /* coroutine has returned */
    int          yielded_value;  /* value passed via yield */
    AxiomCoroFunc func;          /* user function (takes i32 arg) */
    int          arg;            /* argument to the function */
#if defined(_WIN32)
    void        *fiber;          /* coroutine fiber */
    void        *caller_fiber;   /* caller (main) fiber */
#else
    /* POSIX ucontext */
    char        *stack;          /* heap-allocated stack */
    /* We store ucontext_t inline.  The header is included below. */
#endif
} AxiomCoro;

static AxiomCoro axiom_coros[AXIOM_CORO_MAX];
static int axiom_current_coro = -1;  /* handle of currently running coro */

/* ---- Platform-specific implementation --------------------------------- */

#if defined(_WIN32)

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

/* Fiber entry: runs the user function, then marks the coro done and yields. */
static void CALLBACK axiom_coro_fiber_entry(LPVOID param) {
    int handle = (int)(intptr_t)param;
    AxiomCoro *c = &axiom_coros[handle];
    c->func(c->arg);
    c->done = 1;
    c->yielded_value = -1;
    /* Switch back to the caller fiber; the coroutine is finished. */
    SwitchToFiber(c->caller_fiber);
}

int axiom_coro_create(AxiomCoroFunc func, int arg) {
    int i;
    for (i = 0; i < AXIOM_CORO_MAX; i++) {
        if (!axiom_coros[i].active) {
            AxiomCoro *c = &axiom_coros[i];
            memset(c, 0, sizeof(AxiomCoro));
            c->active = 1;
            c->done   = 0;
            c->func   = func;
            c->arg    = arg;
            c->yielded_value = 0;
            c->fiber  = CreateFiber(
                AXIOM_CORO_STACK,
                axiom_coro_fiber_entry,
                (LPVOID)(intptr_t)i
            );
            if (!c->fiber) {
                c->active = 0;
                return -1;
            }
            return i;
        }
    }
    return -1;  /* no free slots */
}

int axiom_coro_resume(int handle) {
    if (handle < 0 || handle >= AXIOM_CORO_MAX) return -1;
    AxiomCoro *c = &axiom_coros[handle];
    if (!c->active || c->done) return -1;

    /* Convert main thread to fiber if not already done. */
    c->caller_fiber = ConvertThreadToFiber(NULL);
    if (!c->caller_fiber) {
        /* Already a fiber -- GetCurrentFiber instead. */
        c->caller_fiber = GetCurrentFiber();
    }

    axiom_current_coro = handle;
    SwitchToFiber(c->fiber);
    axiom_current_coro = -1;

    return c->yielded_value;
}

void axiom_coro_yield(int value) {
    if (axiom_current_coro < 0 || axiom_current_coro >= AXIOM_CORO_MAX) return;
    AxiomCoro *c = &axiom_coros[axiom_current_coro];
    c->yielded_value = value;
    SwitchToFiber(c->caller_fiber);
}

int axiom_coro_is_done(int handle) {
    if (handle < 0 || handle >= AXIOM_CORO_MAX) return 1;
    return axiom_coros[handle].done;
}

void axiom_coro_destroy(int handle) {
    if (handle < 0 || handle >= AXIOM_CORO_MAX) return;
    AxiomCoro *c = &axiom_coros[handle];
    if (c->fiber) {
        DeleteFiber(c->fiber);
        c->fiber = NULL;
    }
    c->active = 0;
}

#else /* POSIX ----------------------------------------------------------- */

#include <ucontext.h>

/* We store the ucontext alongside the coro struct via an auxiliary array
   to avoid bloating the struct definition (ucontext_t can be large). */
static ucontext_t axiom_coro_uctx[AXIOM_CORO_MAX];
static ucontext_t axiom_coro_caller_uctx[AXIOM_CORO_MAX];

/* ucontext entry: runs the user function, then marks done. */
static void axiom_coro_uctx_entry(int handle) {
    AxiomCoro *c = &axiom_coros[handle];
    c->func(c->arg);
    c->done = 1;
    c->yielded_value = -1;
    /* Swap back to caller; coroutine is finished. */
    swapcontext(&axiom_coro_uctx[handle], &axiom_coro_caller_uctx[handle]);
}

int axiom_coro_create(AxiomCoroFunc func, int arg) {
    int i;
    for (i = 0; i < AXIOM_CORO_MAX; i++) {
        if (!axiom_coros[i].active) {
            AxiomCoro *c = &axiom_coros[i];
            memset(c, 0, sizeof(AxiomCoro));
            c->active = 1;
            c->done   = 0;
            c->func   = func;
            c->arg    = arg;
            c->yielded_value = 0;

            c->stack = (char *)malloc(AXIOM_CORO_STACK);
            if (!c->stack) { c->active = 0; return -1; }

            getcontext(&axiom_coro_uctx[i]);
            axiom_coro_uctx[i].uc_stack.ss_sp   = c->stack;
            axiom_coro_uctx[i].uc_stack.ss_size  = AXIOM_CORO_STACK;
            axiom_coro_uctx[i].uc_link           = &axiom_coro_caller_uctx[i];
            makecontext(&axiom_coro_uctx[i],
                        (void (*)(void))axiom_coro_uctx_entry, 1, i);

            return i;
        }
    }
    return -1;
}

int axiom_coro_resume(int handle) {
    if (handle < 0 || handle >= AXIOM_CORO_MAX) return -1;
    AxiomCoro *c = &axiom_coros[handle];
    if (!c->active || c->done) return -1;

    axiom_current_coro = handle;
    swapcontext(&axiom_coro_caller_uctx[handle], &axiom_coro_uctx[handle]);
    axiom_current_coro = -1;

    return c->yielded_value;
}

void axiom_coro_yield(int value) {
    if (axiom_current_coro < 0 || axiom_current_coro >= AXIOM_CORO_MAX) return;
    int h = axiom_current_coro;
    axiom_coros[h].yielded_value = value;
    swapcontext(&axiom_coro_uctx[h], &axiom_coro_caller_uctx[h]);
}

int axiom_coro_is_done(int handle) {
    if (handle < 0 || handle >= AXIOM_CORO_MAX) return 1;
    return axiom_coros[handle].done;
}

void axiom_coro_destroy(int handle) {
    if (handle < 0 || handle >= AXIOM_CORO_MAX) return;
    AxiomCoro *c = &axiom_coros[handle];
    if (c->stack) {
        free(c->stack);
        c->stack = NULL;
    }
    c->active = 0;
}

#endif /* _WIN32 / POSIX */

/* ── Threading primitives + Job system ──────────────────────────────── */
/*
 * Provides thread creation/join, atomics, mutexes, and a simple thread
 * pool with work-stealing job dispatch for data-parallel workloads.
 *
 * On Windows: Win32 threads, CRITICAL_SECTION, CONDITION_VARIABLE
 * On POSIX:   pthreads, pthread_mutex, pthread_cond
 */

typedef void (*AxiomThreadFunc)(void*);

/* ---- Platform-specific thread handles ---------------------------------- */

#if defined(_WIN32)

/* windows.h already included above for coroutines; guard just in case */
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef _WINDOWS_
#include <windows.h>
#endif

/* ── Thread create / join ─────────────────────────────────────────── */

#define AXIOM_MAX_THREADS 64

typedef struct {
    HANDLE      handle;
    int         active;
    AxiomThreadFunc func;
    void       *arg;
} AxiomThread;

static AxiomThread axiom_threads[AXIOM_MAX_THREADS];

static DWORD WINAPI axiom_thread_entry(LPVOID param) {
    int id = (int)(intptr_t)param;
    AxiomThread *t = &axiom_threads[id];
    t->func(t->arg);
    return 0;
}

int axiom_thread_create(AxiomThreadFunc func, void *arg) {
    int i;
    for (i = 0; i < AXIOM_MAX_THREADS; i++) {
        if (!axiom_threads[i].active) {
            axiom_threads[i].active = 1;
            axiom_threads[i].func   = func;
            axiom_threads[i].arg    = arg;
            axiom_threads[i].handle = CreateThread(
                NULL, 0, axiom_thread_entry, (LPVOID)(intptr_t)i, 0, NULL
            );
            if (!axiom_threads[i].handle) {
                axiom_threads[i].active = 0;
                return -1;
            }
            return i;
        }
    }
    return -1;
}

void axiom_thread_join(int handle) {
    if (handle < 0 || handle >= AXIOM_MAX_THREADS) return;
    AxiomThread *t = &axiom_threads[handle];
    if (!t->active) return;
    WaitForSingleObject(t->handle, INFINITE);
    CloseHandle(t->handle);
    t->handle = NULL;
    t->active = 0;
}

/* ── Atomics ──────────────────────────────────────────────────────── */

int axiom_atomic_load(volatile int *ptr)  {
    return InterlockedCompareExchange((volatile LONG *)ptr, 0, 0);
}

void axiom_atomic_store(volatile int *ptr, int val) {
    InterlockedExchange((volatile LONG *)ptr, val);
}

int axiom_atomic_add(volatile int *ptr, int val) {
    return InterlockedExchangeAdd((volatile LONG *)ptr, val);
}

int axiom_atomic_cas(volatile int *ptr, int expected, int desired) {
    return InterlockedCompareExchange((volatile LONG *)ptr, desired, expected);
}

/* ── Mutex ────────────────────────────────────────────────────────── */

void *axiom_mutex_create(void) {
    CRITICAL_SECTION *cs = (CRITICAL_SECTION *)malloc(sizeof(CRITICAL_SECTION));
    if (!cs) return NULL;
    InitializeCriticalSection(cs);
    return cs;
}

void axiom_mutex_lock(void *mtx)    { EnterCriticalSection((CRITICAL_SECTION *)mtx); }
void axiom_mutex_unlock(void *mtx)  { LeaveCriticalSection((CRITICAL_SECTION *)mtx); }

void axiom_mutex_destroy(void *mtx) {
    if (!mtx) return;
    DeleteCriticalSection((CRITICAL_SECTION *)mtx);
    free(mtx);
}

/* ── Job system (thread pool) ─────────────────────────────────────── */

#define AXIOM_JOB_MAX_WORKERS 32
#define AXIOM_JOB_QUEUE_SIZE  256

typedef void (*AxiomJobFunc)(void*, int, int);

typedef struct {
    AxiomJobFunc func;
    void        *data;
    int          start;
    int          end;
} AxiomJob;

static HANDLE            axiom_worker_threads[AXIOM_JOB_MAX_WORKERS];
static int               axiom_num_workers = 0;
static volatile int      axiom_jobs_running = 0;  /* 1 = pool active */

static CRITICAL_SECTION  axiom_job_queue_cs;
static CONDITION_VARIABLE axiom_job_queue_cv;
static CONDITION_VARIABLE axiom_job_done_cv;

static AxiomJob          axiom_job_queue[AXIOM_JOB_QUEUE_SIZE];
static volatile int      axiom_job_head = 0;
static volatile int      axiom_job_tail = 0;
static volatile int      axiom_jobs_pending = 0;

static DWORD WINAPI axiom_worker_func(LPVOID param) {
    (void)param;
    while (1) {
        AxiomJob job;
        int got = 0;

        EnterCriticalSection(&axiom_job_queue_cs);
        while (axiom_job_head == axiom_job_tail && axiom_jobs_running) {
            SleepConditionVariableCS(&axiom_job_queue_cv, &axiom_job_queue_cs, INFINITE);
        }
        if (!axiom_jobs_running && axiom_job_head == axiom_job_tail) {
            LeaveCriticalSection(&axiom_job_queue_cs);
            return 0;
        }
        if (axiom_job_head != axiom_job_tail) {
            job = axiom_job_queue[axiom_job_head % AXIOM_JOB_QUEUE_SIZE];
            axiom_job_head++;
            got = 1;
        }
        LeaveCriticalSection(&axiom_job_queue_cs);

        if (got) {
            job.func(job.data, job.start, job.end);
            InterlockedDecrement((volatile LONG *)&axiom_jobs_pending);
            WakeConditionVariable(&axiom_job_done_cv);
        }
    }
}

void axiom_jobs_init(int num_workers) {
    int i;
    if (num_workers < 1) num_workers = 1;
    if (num_workers > AXIOM_JOB_MAX_WORKERS) num_workers = AXIOM_JOB_MAX_WORKERS;

    InitializeCriticalSection(&axiom_job_queue_cs);
    InitializeConditionVariable(&axiom_job_queue_cv);
    InitializeConditionVariable(&axiom_job_done_cv);

    axiom_job_head = 0;
    axiom_job_tail = 0;
    axiom_jobs_pending = 0;
    axiom_jobs_running = 1;
    axiom_num_workers = num_workers;

    for (i = 0; i < num_workers; i++) {
        axiom_worker_threads[i] = CreateThread(
            NULL, 0, axiom_worker_func, NULL, 0, NULL
        );
    }
}

void axiom_job_dispatch(AxiomJobFunc func, void *data, int total_items) {
    int chunk, start, end;
    if (total_items <= 0 || axiom_num_workers <= 0) return;

    chunk = (total_items + axiom_num_workers - 1) / axiom_num_workers;

    EnterCriticalSection(&axiom_job_queue_cs);
    for (start = 0; start < total_items; start += chunk) {
        end = start + chunk;
        if (end > total_items) end = total_items;

        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].func  = func;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].data  = data;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].start = start;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].end   = end;
        axiom_job_tail++;
        InterlockedIncrement((volatile LONG *)&axiom_jobs_pending);
    }
    LeaveCriticalSection(&axiom_job_queue_cs);
    WakeAllConditionVariable(&axiom_job_queue_cv);
}

void axiom_job_wait(void) {
    EnterCriticalSection(&axiom_job_queue_cs);
    while (axiom_jobs_pending > 0) {
        SleepConditionVariableCS(&axiom_job_done_cv, &axiom_job_queue_cs, INFINITE);
    }
    LeaveCriticalSection(&axiom_job_queue_cs);
}

void axiom_jobs_shutdown(void) {
    int i;
    EnterCriticalSection(&axiom_job_queue_cs);
    axiom_jobs_running = 0;
    LeaveCriticalSection(&axiom_job_queue_cs);
    WakeAllConditionVariable(&axiom_job_queue_cv);

    for (i = 0; i < axiom_num_workers; i++) {
        WaitForSingleObject(axiom_worker_threads[i], INFINITE);
        CloseHandle(axiom_worker_threads[i]);
        axiom_worker_threads[i] = NULL;
    }
    DeleteCriticalSection(&axiom_job_queue_cs);
    axiom_num_workers = 0;
}

int axiom_num_cores(void) {
    SYSTEM_INFO si;
    GetSystemInfo(&si);
    return (int)si.dwNumberOfProcessors;
}

/* ── Job handles & dependency graph ────────────────────────────────── */

#define AXIOM_MAX_JOB_HANDLES 256

typedef struct {
    volatile LONG complete;     /* 0 = pending, 1 = done */
    int           dependency;   /* -1 = none, else handle index to wait for */
    volatile LONG pending;      /* number of sub-jobs still running */
} AxiomJobHandle;

static AxiomJobHandle axiom_job_handles[AXIOM_MAX_JOB_HANDLES];
static volatile LONG  axiom_next_job_handle = 0;

static int axiom_alloc_handle(int dep) {
    LONG idx = InterlockedIncrement(&axiom_next_job_handle) - 1;
    idx = idx % AXIOM_MAX_JOB_HANDLES;
    axiom_job_handles[idx].complete = 0;
    axiom_job_handles[idx].dependency = dep;
    axiom_job_handles[idx].pending = 0;
    return (int)idx;
}

/* Wrapper that decrements the handle's pending counter and marks complete. */
typedef struct {
    AxiomJobFunc func;
    void        *data;
    int          start;
    int          end;
    int          handle;
} AxiomHandleJob;

static AxiomHandleJob axiom_handle_jobs[AXIOM_JOB_QUEUE_SIZE];
static volatile LONG  axiom_next_handle_job = 0;

static void axiom_handle_job_wrapper(void *arg, int start, int end) {
    AxiomHandleJob *hj = (AxiomHandleJob *)arg;
    hj->func(hj->data, start, end);
    if (InterlockedDecrement(&axiom_job_handles[hj->handle].pending) == 0) {
        InterlockedExchange(&axiom_job_handles[hj->handle].complete, 1);
        WakeAllConditionVariable(&axiom_job_done_cv);
    }
}

int axiom_job_dispatch_handle(AxiomJobFunc func, void *data, int total_items) {
    int handle = axiom_alloc_handle(-1);
    int chunk, start, end;
    if (total_items <= 0 || axiom_num_workers <= 0) {
        InterlockedExchange(&axiom_job_handles[handle].complete, 1);
        return handle;
    }
    chunk = (total_items + axiom_num_workers - 1) / axiom_num_workers;

    EnterCriticalSection(&axiom_job_queue_cs);
    for (start = 0; start < total_items; start += chunk) {
        LONG slot;
        end = start + chunk;
        if (end > total_items) end = total_items;

        InterlockedIncrement(&axiom_job_handles[handle].pending);

        slot = InterlockedIncrement(&axiom_next_handle_job) - 1;
        slot = slot % AXIOM_JOB_QUEUE_SIZE;
        axiom_handle_jobs[slot].func   = func;
        axiom_handle_jobs[slot].data   = data;
        axiom_handle_jobs[slot].start  = start;
        axiom_handle_jobs[slot].end    = end;
        axiom_handle_jobs[slot].handle = handle;

        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].func  = axiom_handle_job_wrapper;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].data  = &axiom_handle_jobs[slot];
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].start = start;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].end   = end;
        axiom_job_tail++;
        InterlockedIncrement((volatile LONG *)&axiom_jobs_pending);
    }
    LeaveCriticalSection(&axiom_job_queue_cs);
    WakeAllConditionVariable(&axiom_job_queue_cv);
    return handle;
}

int axiom_job_dispatch_after(AxiomJobFunc func, void *data, int total_items, int dep) {
    int handle;
    /* Wait for the dependency to complete first. */
    if (dep >= 0 && dep < AXIOM_MAX_JOB_HANDLES) {
        while (!axiom_job_handles[dep].complete) {
            EnterCriticalSection(&axiom_job_queue_cs);
            if (!axiom_job_handles[dep].complete) {
                SleepConditionVariableCS(&axiom_job_done_cv, &axiom_job_queue_cs, INFINITE);
            }
            LeaveCriticalSection(&axiom_job_queue_cs);
        }
    }
    handle = axiom_job_dispatch_handle(func, data, total_items);
    axiom_job_handles[handle].dependency = dep;
    return handle;
}

void axiom_job_wait_handle(int handle) {
    if (handle < 0 || handle >= AXIOM_MAX_JOB_HANDLES) return;
    EnterCriticalSection(&axiom_job_queue_cs);
    while (!axiom_job_handles[handle].complete) {
        SleepConditionVariableCS(&axiom_job_done_cv, &axiom_job_queue_cs, INFINITE);
    }
    LeaveCriticalSection(&axiom_job_queue_cs);
}

#else /* POSIX ----------------------------------------------------------- */

#include <pthread.h>

/* ── Thread create / join ─────────────────────────────────────────── */

#define AXIOM_MAX_THREADS 64

typedef struct {
    pthread_t    thread;
    int          active;
    AxiomThreadFunc func;
    void        *arg;
} AxiomThread;

static AxiomThread axiom_threads[AXIOM_MAX_THREADS];

static void *axiom_thread_entry(void *param) {
    int id = (int)(intptr_t)param;
    AxiomThread *t = &axiom_threads[id];
    t->func(t->arg);
    return NULL;
}

int axiom_thread_create(AxiomThreadFunc func, void *arg) {
    int i;
    for (i = 0; i < AXIOM_MAX_THREADS; i++) {
        if (!axiom_threads[i].active) {
            axiom_threads[i].active = 1;
            axiom_threads[i].func   = func;
            axiom_threads[i].arg    = arg;
            if (pthread_create(&axiom_threads[i].thread, NULL, axiom_thread_entry,
                               (void *)(intptr_t)i) != 0) {
                axiom_threads[i].active = 0;
                return -1;
            }
            return i;
        }
    }
    return -1;
}

void axiom_thread_join(int handle) {
    if (handle < 0 || handle >= AXIOM_MAX_THREADS) return;
    AxiomThread *t = &axiom_threads[handle];
    if (!t->active) return;
    pthread_join(t->thread, NULL);
    t->active = 0;
}

/* ── Atomics ──────────────────────────────────────────────────────── */

int axiom_atomic_load(volatile int *ptr) {
    return __atomic_load_n(ptr, __ATOMIC_SEQ_CST);
}

void axiom_atomic_store(volatile int *ptr, int val) {
    __atomic_store_n(ptr, val, __ATOMIC_SEQ_CST);
}

int axiom_atomic_add(volatile int *ptr, int val) {
    return __atomic_fetch_add(ptr, val, __ATOMIC_SEQ_CST);
}

int axiom_atomic_cas(volatile int *ptr, int expected, int desired) {
    int old = expected;
    __atomic_compare_exchange_n(ptr, &old, desired, 0,
                                __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
    return old;
}

/* ── Mutex ────────────────────────────────────────────────────────── */

void *axiom_mutex_create(void) {
    pthread_mutex_t *mtx = (pthread_mutex_t *)malloc(sizeof(pthread_mutex_t));
    if (!mtx) return NULL;
    pthread_mutex_init(mtx, NULL);
    return mtx;
}

void axiom_mutex_lock(void *mtx)    { pthread_mutex_lock((pthread_mutex_t *)mtx); }
void axiom_mutex_unlock(void *mtx)  { pthread_mutex_unlock((pthread_mutex_t *)mtx); }

void axiom_mutex_destroy(void *mtx) {
    if (!mtx) return;
    pthread_mutex_destroy((pthread_mutex_t *)mtx);
    free(mtx);
}

/* ── Job system (thread pool) ─────────────────────────────────────── */

#define AXIOM_JOB_MAX_WORKERS 32
#define AXIOM_JOB_QUEUE_SIZE  256

typedef void (*AxiomJobFunc)(void*, int, int);

typedef struct {
    AxiomJobFunc func;
    void        *data;
    int          start;
    int          end;
} AxiomJob;

static pthread_t         axiom_worker_threads[AXIOM_JOB_MAX_WORKERS];
static int               axiom_num_workers = 0;
static volatile int      axiom_jobs_running = 0;

static pthread_mutex_t   axiom_job_queue_mtx = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t    axiom_job_queue_cv  = PTHREAD_COND_INITIALIZER;
static pthread_cond_t    axiom_job_done_cv   = PTHREAD_COND_INITIALIZER;

static AxiomJob          axiom_job_queue[AXIOM_JOB_QUEUE_SIZE];
static volatile int      axiom_job_head = 0;
static volatile int      axiom_job_tail = 0;
static volatile int      axiom_jobs_pending = 0;

static void *axiom_worker_func(void *param) {
    (void)param;
    while (1) {
        AxiomJob job;
        int got = 0;

        pthread_mutex_lock(&axiom_job_queue_mtx);
        while (axiom_job_head == axiom_job_tail && axiom_jobs_running) {
            pthread_cond_wait(&axiom_job_queue_cv, &axiom_job_queue_mtx);
        }
        if (!axiom_jobs_running && axiom_job_head == axiom_job_tail) {
            pthread_mutex_unlock(&axiom_job_queue_mtx);
            return NULL;
        }
        if (axiom_job_head != axiom_job_tail) {
            job = axiom_job_queue[axiom_job_head % AXIOM_JOB_QUEUE_SIZE];
            axiom_job_head++;
            got = 1;
        }
        pthread_mutex_unlock(&axiom_job_queue_mtx);

        if (got) {
            job.func(job.data, job.start, job.end);
            __atomic_sub_fetch(&axiom_jobs_pending, 1, __ATOMIC_SEQ_CST);
            pthread_cond_signal(&axiom_job_done_cv);
        }
    }
}

void axiom_jobs_init(int num_workers) {
    int i;
    if (num_workers < 1) num_workers = 1;
    if (num_workers > AXIOM_JOB_MAX_WORKERS) num_workers = AXIOM_JOB_MAX_WORKERS;

    pthread_mutex_init(&axiom_job_queue_mtx, NULL);
    pthread_cond_init(&axiom_job_queue_cv, NULL);
    pthread_cond_init(&axiom_job_done_cv, NULL);

    axiom_job_head = 0;
    axiom_job_tail = 0;
    axiom_jobs_pending = 0;
    axiom_jobs_running = 1;
    axiom_num_workers = num_workers;

    for (i = 0; i < num_workers; i++) {
        pthread_create(&axiom_worker_threads[i], NULL, axiom_worker_func, NULL);
    }
}

void axiom_job_dispatch(AxiomJobFunc func, void *data, int total_items) {
    int chunk, start, end;
    if (total_items <= 0 || axiom_num_workers <= 0) return;

    chunk = (total_items + axiom_num_workers - 1) / axiom_num_workers;

    pthread_mutex_lock(&axiom_job_queue_mtx);
    for (start = 0; start < total_items; start += chunk) {
        end = start + chunk;
        if (end > total_items) end = total_items;

        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].func  = func;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].data  = data;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].start = start;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].end   = end;
        axiom_job_tail++;
        __atomic_add_fetch(&axiom_jobs_pending, 1, __ATOMIC_SEQ_CST);
    }
    pthread_mutex_unlock(&axiom_job_queue_mtx);
    pthread_cond_broadcast(&axiom_job_queue_cv);
}

void axiom_job_wait(void) {
    pthread_mutex_lock(&axiom_job_queue_mtx);
    while (axiom_jobs_pending > 0) {
        pthread_cond_wait(&axiom_job_done_cv, &axiom_job_queue_mtx);
    }
    pthread_mutex_unlock(&axiom_job_queue_mtx);
}

void axiom_jobs_shutdown(void) {
    int i;
    pthread_mutex_lock(&axiom_job_queue_mtx);
    axiom_jobs_running = 0;
    pthread_mutex_unlock(&axiom_job_queue_mtx);
    pthread_cond_broadcast(&axiom_job_queue_cv);

    for (i = 0; i < axiom_num_workers; i++) {
        pthread_join(axiom_worker_threads[i], NULL);
    }
    pthread_mutex_destroy(&axiom_job_queue_mtx);
    pthread_cond_destroy(&axiom_job_queue_cv);
    pthread_cond_destroy(&axiom_job_done_cv);
    axiom_num_workers = 0;
}

int axiom_num_cores(void) {
    long n = sysconf(_SC_NPROCESSORS_ONLN);
    return n > 0 ? (int)n : 1;
}

/* ── Job handles & dependency graph (POSIX) ────────────────────────── */

#define AXIOM_MAX_JOB_HANDLES 256

typedef struct {
    volatile int complete;      /* 0 = pending, 1 = done */
    int          dependency;    /* -1 = none, else handle index to wait for */
    volatile int pending;       /* number of sub-jobs still running */
} AxiomJobHandle;

static AxiomJobHandle axiom_job_handles[AXIOM_MAX_JOB_HANDLES];
static volatile int   axiom_next_job_handle = 0;

static int axiom_alloc_handle(int dep) {
    int idx = __atomic_fetch_add(&axiom_next_job_handle, 1, __ATOMIC_SEQ_CST);
    idx = idx % AXIOM_MAX_JOB_HANDLES;
    axiom_job_handles[idx].complete = 0;
    axiom_job_handles[idx].dependency = dep;
    axiom_job_handles[idx].pending = 0;
    return idx;
}

typedef struct {
    AxiomJobFunc func;
    void        *data;
    int          start;
    int          end;
    int          handle;
} AxiomHandleJob;

static AxiomHandleJob axiom_handle_jobs[AXIOM_JOB_QUEUE_SIZE];
static volatile int   axiom_next_handle_job = 0;

static void axiom_handle_job_wrapper(void *arg, int start, int end) {
    AxiomHandleJob *hj = (AxiomHandleJob *)arg;
    hj->func(hj->data, start, end);
    if (__atomic_sub_fetch(&axiom_job_handles[hj->handle].pending, 1, __ATOMIC_SEQ_CST) == 0) {
        __atomic_store_n(&axiom_job_handles[hj->handle].complete, 1, __ATOMIC_SEQ_CST);
        pthread_cond_broadcast(&axiom_job_done_cv);
    }
}

int axiom_job_dispatch_handle(AxiomJobFunc func, void *data, int total_items) {
    int handle = axiom_alloc_handle(-1);
    int chunk, start, end;
    if (total_items <= 0 || axiom_num_workers <= 0) {
        __atomic_store_n(&axiom_job_handles[handle].complete, 1, __ATOMIC_SEQ_CST);
        return handle;
    }
    chunk = (total_items + axiom_num_workers - 1) / axiom_num_workers;

    pthread_mutex_lock(&axiom_job_queue_mtx);
    for (start = 0; start < total_items; start += chunk) {
        int slot;
        end = start + chunk;
        if (end > total_items) end = total_items;

        __atomic_add_fetch(&axiom_job_handles[handle].pending, 1, __ATOMIC_SEQ_CST);

        slot = __atomic_fetch_add(&axiom_next_handle_job, 1, __ATOMIC_SEQ_CST);
        slot = slot % AXIOM_JOB_QUEUE_SIZE;
        axiom_handle_jobs[slot].func   = func;
        axiom_handle_jobs[slot].data   = data;
        axiom_handle_jobs[slot].start  = start;
        axiom_handle_jobs[slot].end    = end;
        axiom_handle_jobs[slot].handle = handle;

        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].func  = axiom_handle_job_wrapper;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].data  = &axiom_handle_jobs[slot];
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].start = start;
        axiom_job_queue[axiom_job_tail % AXIOM_JOB_QUEUE_SIZE].end   = end;
        axiom_job_tail++;
        __atomic_add_fetch(&axiom_jobs_pending, 1, __ATOMIC_SEQ_CST);
    }
    pthread_mutex_unlock(&axiom_job_queue_mtx);
    pthread_cond_broadcast(&axiom_job_queue_cv);
    return handle;
}

int axiom_job_dispatch_after(AxiomJobFunc func, void *data, int total_items, int dep) {
    int handle;
    /* Wait for the dependency to complete first. */
    if (dep >= 0 && dep < AXIOM_MAX_JOB_HANDLES) {
        while (!__atomic_load_n(&axiom_job_handles[dep].complete, __ATOMIC_SEQ_CST)) {
            pthread_mutex_lock(&axiom_job_queue_mtx);
            if (!__atomic_load_n(&axiom_job_handles[dep].complete, __ATOMIC_SEQ_CST)) {
                pthread_cond_wait(&axiom_job_done_cv, &axiom_job_queue_mtx);
            }
            pthread_mutex_unlock(&axiom_job_queue_mtx);
        }
    }
    handle = axiom_job_dispatch_handle(func, data, total_items);
    axiom_job_handles[handle].dependency = dep;
    return handle;
}

void axiom_job_wait_handle(int handle) {
    if (handle < 0 || handle >= AXIOM_MAX_JOB_HANDLES) return;
    pthread_mutex_lock(&axiom_job_queue_mtx);
    while (!__atomic_load_n(&axiom_job_handles[handle].complete, __ATOMIC_SEQ_CST)) {
        pthread_cond_wait(&axiom_job_done_cv, &axiom_job_queue_mtx);
    }
    pthread_mutex_unlock(&axiom_job_queue_mtx);
}

#endif /* _WIN32 / POSIX -- threading */

/* ── Renderer API ────────────────────────────────────────────────────── */
/* When AXIOM_USE_WGPU_RENDERER is defined, the renderer functions come  */
/* from the axiom_renderer.dll (wgpu-based). Skip the C stub.           */
#ifndef AXIOM_USE_WGPU_RENDERER
/*
 * Provides a rendering API that AXIOM programs call to create windows,
 * load SPIR-V shaders (compiled by Lux), build pipelines, and draw
 * geometry.
 *
 * On Windows: real windowed renderer using Win32 API + software
 * rasterization.  Creates an actual window, maintains a pixel
 * framebuffer, blits via StretchDIBits.  Implements edge-function
 * triangle rasterization and point drawing.
 *
 * On other platforms: headless stub that prints lifecycle events.
 *
 * API summary:
 *   axiom_renderer_create(w, h, title) -> ptr     Create a renderer context
 *   axiom_renderer_destroy(r)                      Destroy the renderer
 *   axiom_renderer_begin_frame(r) -> i32           Begin a frame (1=ok, 0=fail)
 *   axiom_renderer_end_frame(r)                    End a frame (present)
 *   axiom_renderer_should_close(r) -> i32          1 if window should close
 *   axiom_renderer_clear(r, color)                 Clear framebuffer
 *   axiom_renderer_draw_triangles(r, pos, col, n)  Draw n vertices as tris
 *   axiom_renderer_draw_points(r, x, y, col, n)   Draw n colored points
 *   axiom_renderer_get_time(r) -> f64              Elapsed time in seconds
 *   axiom_shader_load(r, path, stage) -> ptr       Load SPIR-V shader module
 *   axiom_pipeline_create(r, vert, frag) -> ptr    Create a graphics pipeline
 *   axiom_renderer_bind_pipeline(r, p)             Bind a pipeline for drawing
 */

/* Shader stage constants (matches Vulkan VkShaderStageFlagBits layout). */
#define AXIOM_SHADER_STAGE_VERTEX   0
#define AXIOM_SHADER_STAGE_FRAGMENT 1

/* Maximum number of loaded shader modules. */
#define AXIOM_MAX_SHADERS   64

/* Maximum number of pipelines. */
#define AXIOM_MAX_PIPELINES 32

/* ---- Shader module (loaded SPIR-V) ------------------------------------- */

typedef struct {
    int   active;
    int   stage;          /* 0 = vertex, 1 = fragment */
    char  path[512];
} AxiomShaderModule;

/* ---- Graphics pipeline ------------------------------------------------- */

typedef struct {
    int   active;
    int   vert_index;     /* index into shader_modules[] */
    int   frag_index;     /* index into shader_modules[] */
} AxiomPipeline;

static AxiomShaderModule axiom_shader_modules[AXIOM_MAX_SHADERS];
static AxiomPipeline     axiom_pipelines[AXIOM_MAX_PIPELINES];

/* ======================================================================== */
/* Win32 windowed software renderer                                         */
/* ======================================================================== */

#if defined(_WIN32)

/* windows.h is already included above for coroutines/threading. */

/* ---- Renderer state ---------------------------------------------------- */

typedef struct {
    int           width;
    int           height;
    char          title[256];
    int           should_close;
    int           frame_count;
    long long     start_time_ns;
    /* Win32 windowing */
    HWND          hwnd;
    HDC           hdc;
    BITMAPINFO    bmi;
    /* Software framebuffer: BGRA pixel array (0xAARRGGBB in little-endian) */
    unsigned int *framebuffer;
} AxiomRenderer;

/* Global renderer pointer for the window procedure callback. */
static AxiomRenderer *axiom_renderer_global = NULL;

static LRESULT CALLBACK axiom_wnd_proc(HWND hwnd, UINT msg,
                                        WPARAM wParam, LPARAM lParam) {
    switch (msg) {
    case WM_CLOSE:
    case WM_DESTROY:
        if (axiom_renderer_global) {
            axiom_renderer_global->should_close = 1;
        }
        return 0;
    case WM_KEYDOWN:
        axiom_key_state[wParam & 0xFF] = 1;
        if (wParam == VK_ESCAPE) {
            if (axiom_renderer_global) {
                axiom_renderer_global->should_close = 1;
            }
        }
        return 0;
    case WM_KEYUP:
        axiom_key_state[wParam & 0xFF] = 0;
        return 0;
    case WM_MOUSEMOVE:
        axiom_mouse_x = (int)(short)LOWORD(lParam);
        axiom_mouse_y = (int)(short)HIWORD(lParam);
        return 0;
    case WM_LBUTTONDOWN: axiom_mouse_buttons[0] = 1; return 0;
    case WM_LBUTTONUP:   axiom_mouse_buttons[0] = 0; return 0;
    case WM_RBUTTONDOWN: axiom_mouse_buttons[1] = 1; return 0;
    case WM_RBUTTONUP:   axiom_mouse_buttons[1] = 0; return 0;
    case WM_MBUTTONDOWN: axiom_mouse_buttons[2] = 1; return 0;
    case WM_MBUTTONUP:   axiom_mouse_buttons[2] = 0; return 0;
    }
    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

void *axiom_renderer_create(int width, int height, const char *title) {
    AxiomRenderer *r = (AxiomRenderer *)calloc(1, sizeof(AxiomRenderer));
    if (!r) return NULL;

    r->width  = width;
    r->height = height;
    r->should_close = 0;
    r->frame_count  = 0;
    r->start_time_ns = axiom_clock_ns();

    /* Copy title. */
    if (title) {
        size_t len = strlen(title);
        if (len >= sizeof(r->title)) len = sizeof(r->title) - 1;
        memcpy(r->title, title, len);
        r->title[len] = '\0';
    } else {
        strcpy(r->title, "AXIOM");
    }

    /* Allocate framebuffer. */
    r->framebuffer = (unsigned int *)calloc((size_t)(width * height),
                                            sizeof(unsigned int));
    if (!r->framebuffer) {
        free(r);
        return NULL;
    }

    /* Register window class (idempotent -- RegisterClassW returns 0 if
       already registered, but that is fine). */
    WNDCLASSW wc;
    memset(&wc, 0, sizeof(wc));
    wc.lpfnWndProc   = axiom_wnd_proc;
    wc.hInstance      = GetModuleHandleW(NULL);
    wc.lpszClassName  = L"AxiomRendererClass";
    wc.hCursor        = LoadCursorW(NULL, (LPCWSTR)IDC_ARROW);
    wc.hbrBackground  = (HBRUSH)GetStockObject(BLACK_BRUSH);
    RegisterClassW(&wc);

    /* Convert title to wide string. */
    wchar_t wtitle[256];
    MultiByteToWideChar(CP_UTF8, 0, r->title, -1, wtitle, 256);

    /* Compute window rect that gives us the desired *client* area. */
    RECT wr = { 0, 0, width, height };
    AdjustWindowRectEx(&wr, WS_OVERLAPPEDWINDOW, FALSE, 0);

    r->hwnd = CreateWindowExW(
        0, L"AxiomRendererClass", wtitle,
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        CW_USEDEFAULT, CW_USEDEFAULT,
        wr.right - wr.left, wr.bottom - wr.top,
        NULL, NULL, GetModuleHandleW(NULL), NULL
    );

    if (!r->hwnd) {
        free(r->framebuffer);
        free(r);
        return NULL;
    }

    r->hdc = GetDC(r->hwnd);
    axiom_renderer_global = r;

    /* Setup BITMAPINFO for StretchDIBits blitting. */
    memset(&r->bmi, 0, sizeof(BITMAPINFO));
    r->bmi.bmiHeader.biSize        = sizeof(BITMAPINFOHEADER);
    r->bmi.bmiHeader.biWidth       = width;
    r->bmi.bmiHeader.biHeight      = -height; /* negative = top-down */
    r->bmi.bmiHeader.biPlanes      = 1;
    r->bmi.bmiHeader.biBitCount    = 32;
    r->bmi.bmiHeader.biCompression = BI_RGB;

    printf("[AXIOM Renderer] Created %dx%d window: \"%s\" (Win32 software)\n",
           width, height, r->title);

    return r;
}

void axiom_renderer_destroy(void *renderer) {
    if (!renderer) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;

    printf("[AXIOM Renderer] Destroyed after %d frames: \"%s\"\n",
           r->frame_count, r->title);

    if (r->hdc && r->hwnd) {
        ReleaseDC(r->hwnd, r->hdc);
    }
    if (r->hwnd) {
        DestroyWindow(r->hwnd);
    }
    if (axiom_renderer_global == r) {
        axiom_renderer_global = NULL;
    }
    free(r->framebuffer);
    free(r);
}

/* ---- Frame operations -------------------------------------------------- */

int axiom_renderer_begin_frame(void *renderer) {
    if (!renderer) return 0;
    AxiomRenderer *r = (AxiomRenderer *)renderer;

    /* Pump Win32 message queue so the window stays responsive. */
    MSG msg;
    while (PeekMessageW(&msg, NULL, 0, 0, PM_REMOVE)) {
        if (msg.message == WM_QUIT) {
            r->should_close = 1;
            return 0;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    if (r->should_close) return 0;
    return 1;
}

void axiom_renderer_end_frame(void *renderer) {
    if (!renderer) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;

    /* Blit the software framebuffer to the window. */
    StretchDIBits(
        r->hdc,
        0, 0, r->width, r->height,          /* dest rect */
        0, 0, r->width, r->height,          /* src rect */
        r->framebuffer,
        &r->bmi,
        DIB_RGB_COLORS,
        SRCCOPY
    );
    GdiFlush();

    r->frame_count++;

    /* Print progress for first few frames and periodically. */
    if (r->frame_count <= 3 || r->frame_count % 50 == 0) {
        printf("[AXIOM Renderer] Frame %d presented\n", r->frame_count);
    }
}

int axiom_renderer_should_close(void *renderer) {
    if (!renderer) return 1;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    return r->should_close;
}

/* ---- Clear -------------------------------------------------------------- */

void axiom_renderer_clear(void *renderer, unsigned int color) {
    if (!renderer) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    int total = r->width * r->height;
    int i;
    /* Fast path for black (color == 0). */
    if (color == 0) {
        memset(r->framebuffer, 0, (size_t)total * sizeof(unsigned int));
    } else {
        for (i = 0; i < total; i++) {
            r->framebuffer[i] = color;
        }
    }
}

/* ---- Drawing: points ---------------------------------------------------- */

/* Draw colored points.  x_arr and y_arr are arrays of f64 positions,
   colors is an array of u32 (0xRRGGBB), count is the number of points. */
void axiom_renderer_draw_points(void *renderer,
                                const double *x_arr,
                                const double *y_arr,
                                const unsigned int *colors,
                                int count) {
    if (!renderer || !x_arr || !y_arr || !colors) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    int w = r->width;
    int h = r->height;
    unsigned int *fb = r->framebuffer;
    int i;

    for (i = 0; i < count; i++) {
        int px = (int)(x_arr[i] + 0.5);
        int py = (int)(y_arr[i] + 0.5);
        /* Draw a 2x2 point for visibility. */
        if (px >= 0 && px < w - 1 && py >= 0 && py < h - 1) {
            unsigned int c = colors[i] | 0xFF000000u; /* ensure opaque */
            fb[py * w + px]           = c;
            fb[py * w + px + 1]       = c;
            fb[(py + 1) * w + px]     = c;
            fb[(py + 1) * w + px + 1] = c;
        } else if (px >= 0 && px < w && py >= 0 && py < h) {
            /* Edge pixel: draw single point. */
            fb[py * w + px] = colors[i] | 0xFF000000u;
        }
    }
}

/* ---- Drawing: triangles ------------------------------------------------- */

/* Helper: integer min/max of 3 values. */
static int axiom_min3i(int a, int b, int c) {
    int m = a < b ? a : b;
    return m < c ? m : c;
}
static int axiom_max3i(int a, int b, int c) {
    int m = a > b ? a : b;
    return m > c ? m : c;
}

/* Edge function for triangle rasterization.
   Returns positive if (px,py) is on the left side of edge (ax,ay)->(bx,by). */
static int axiom_edge_func(int ax, int ay, int bx, int by, int px, int py) {
    return (bx - ax) * (py - ay) - (by - ay) * (px - ax);
}

void axiom_renderer_draw_triangles(void *renderer,
                                   const float *positions,
                                   const float *colors_f,
                                   int vertex_count) {
    if (!renderer || !positions) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    int w = r->width;
    int h = r->height;
    unsigned int *fb = r->framebuffer;

    /* Each triangle is 3 vertices, each vertex has 2 floats (x, y)
       in the positions array, and 3 floats (r, g, b) in the colors array. */
    int tri_count = vertex_count / 3;
    int t;
    for (t = 0; t < tri_count; t++) {
        int base_p = t * 6;  /* 3 vertices * 2 coords */
        int base_c = t * 9;  /* 3 vertices * 3 color channels */

        int x0 = (int)(positions[base_p + 0] + 0.5f);
        int y0 = (int)(positions[base_p + 1] + 0.5f);
        int x1 = (int)(positions[base_p + 2] + 0.5f);
        int y1 = (int)(positions[base_p + 3] + 0.5f);
        int x2 = (int)(positions[base_p + 4] + 0.5f);
        int y2 = (int)(positions[base_p + 5] + 0.5f);

        /* Flat color from first vertex (for simplicity). */
        unsigned int cr = 255, cg = 255, cb = 255;
        if (colors_f) {
            cr = (unsigned int)(colors_f[base_c + 0] * 255.0f);
            cg = (unsigned int)(colors_f[base_c + 1] * 255.0f);
            cb = (unsigned int)(colors_f[base_c + 2] * 255.0f);
            if (cr > 255) cr = 255;
            if (cg > 255) cg = 255;
            if (cb > 255) cb = 255;
        }
        unsigned int color = 0xFF000000u | (cr << 16) | (cg << 8) | cb;

        /* Bounding box, clipped to screen. */
        int minX = axiom_min3i(x0, x1, x2);
        int minY = axiom_min3i(y0, y1, y2);
        int maxX = axiom_max3i(x0, x1, x2);
        int maxY = axiom_max3i(y0, y1, y2);
        if (minX < 0) minX = 0;
        if (minY < 0) minY = 0;
        if (maxX >= w) maxX = w - 1;
        if (maxY >= h) maxY = h - 1;

        /* Compute twice the triangle area (for winding check). */
        int area2 = axiom_edge_func(x0, y0, x1, y1, x2, y2);
        if (area2 == 0) continue; /* degenerate triangle */

        /* Rasterize via edge functions. */
        int py, px;
        for (py = minY; py <= maxY; py++) {
            for (px = minX; px <= maxX; px++) {
                int e0 = axiom_edge_func(x0, y0, x1, y1, px, py);
                int e1 = axiom_edge_func(x1, y1, x2, y2, px, py);
                int e2 = axiom_edge_func(x2, y2, x0, y0, px, py);
                /* Accept pixel if all edge functions have same sign. */
                if ((e0 >= 0 && e1 >= 0 && e2 >= 0) ||
                    (e0 <= 0 && e1 <= 0 && e2 <= 0)) {
                    fb[py * w + px] = color;
                }
            }
        }
    }
}

/* ---- Time -------------------------------------------------------------- */

double axiom_renderer_get_time(void *renderer) {
    if (!renderer) return 0.0;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    long long now = axiom_clock_ns();
    return (double)(now - r->start_time_ns) / 1000000000.0;
}

/* ---- Shader loading (SPIR-V from Lux) --------------------------------- */

void *axiom_shader_load(void *renderer, const char *spv_path, int stage) {
    if (!renderer || !spv_path) return NULL;
    (void)renderer;

    int i;
    for (i = 0; i < AXIOM_MAX_SHADERS; i++) {
        if (!axiom_shader_modules[i].active) {
            AxiomShaderModule *s = &axiom_shader_modules[i];
            s->active = 1;
            s->stage  = stage;

            size_t len = strlen(spv_path);
            if (len >= sizeof(s->path)) len = sizeof(s->path) - 1;
            memcpy(s->path, spv_path, len);
            s->path[len] = '\0';

            const char *stage_name = (stage == AXIOM_SHADER_STAGE_VERTEX)
                                         ? "vertex"
                                         : (stage == AXIOM_SHADER_STAGE_FRAGMENT)
                                               ? "fragment"
                                               : "unknown";

            printf("[AXIOM Renderer] Loaded %s shader: \"%s\" (slot %d)\n",
                   stage_name, spv_path, i);
            return s;
        }
    }

    printf("[AXIOM Renderer] ERROR: no free shader slots\n");
    return NULL;
}

/* ---- Pipeline creation ------------------------------------------------- */

void *axiom_pipeline_create(void *renderer, void *vert_shader, void *frag_shader) {
    if (!renderer) return NULL;
    (void)renderer;

    int i;
    for (i = 0; i < AXIOM_MAX_PIPELINES; i++) {
        if (!axiom_pipelines[i].active) {
            AxiomPipeline *p = &axiom_pipelines[i];
            p->active = 1;

            if (vert_shader) {
                p->vert_index = (int)(((AxiomShaderModule *)vert_shader)
                                      - axiom_shader_modules);
            } else {
                p->vert_index = -1;
            }
            if (frag_shader) {
                p->frag_index = (int)(((AxiomShaderModule *)frag_shader)
                                      - axiom_shader_modules);
            } else {
                p->frag_index = -1;
            }

            printf("[AXIOM Renderer] Created pipeline %d "
                   "(vert=%d, frag=%d)\n",
                   i, p->vert_index, p->frag_index);
            return p;
        }
    }

    printf("[AXIOM Renderer] ERROR: no free pipeline slots\n");
    return NULL;
}

void axiom_renderer_bind_pipeline(void *renderer, void *pipeline) {
    if (!renderer || !pipeline) return;
    (void)renderer;
    (void)pipeline;
}

#else /* !_WIN32 -- headless stub for non-Windows platforms */

/* ---- Renderer state (headless stub) ------------------------------------ */

typedef struct {
    int   width;
    int   height;
    char  title[256];
    int   should_close;
    int   frame_count;
    long long start_time_ns;
} AxiomRenderer;

void *axiom_renderer_create(int width, int height, const char *title) {
    AxiomRenderer *r = (AxiomRenderer *)calloc(1, sizeof(AxiomRenderer));
    if (!r) return NULL;

    r->width  = width;
    r->height = height;
    r->should_close = 0;
    r->frame_count  = 0;
    r->start_time_ns = axiom_clock_ns();

    if (title) {
        size_t len = strlen(title);
        if (len >= sizeof(r->title)) len = sizeof(r->title) - 1;
        memcpy(r->title, title, len);
        r->title[len] = '\0';
    } else {
        r->title[0] = '\0';
    }

    printf("[AXIOM Renderer] Created %dx%d window: \"%s\" (headless stub)\n",
           width, height, r->title);
    return r;
}

void axiom_renderer_destroy(void *renderer) {
    if (!renderer) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    printf("[AXIOM Renderer] Destroyed after %d frames: \"%s\"\n",
           r->frame_count, r->title);
    free(r);
}

int axiom_renderer_begin_frame(void *renderer) {
    if (!renderer) return 0;
    return 1;
}

void axiom_renderer_end_frame(void *renderer) {
    if (!renderer) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    r->frame_count++;
    if (r->frame_count <= 3 || r->frame_count % 50 == 0) {
        printf("[AXIOM Renderer] Frame %d complete\n", r->frame_count);
    }
}

int axiom_renderer_should_close(void *renderer) {
    if (!renderer) return 1;
    return ((AxiomRenderer *)renderer)->should_close;
}

void axiom_renderer_clear(void *renderer, unsigned int color) {
    (void)renderer; (void)color;
}

void axiom_renderer_draw_points(void *renderer,
                                const double *x_arr,
                                const double *y_arr,
                                const unsigned int *colors,
                                int count) {
    (void)renderer; (void)x_arr; (void)y_arr; (void)colors; (void)count;
}

void axiom_renderer_draw_triangles(void *renderer,
                                   const float *positions,
                                   const float *colors,
                                   int vertex_count) {
    if (!renderer) return;
    (void)positions; (void)colors;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    if (r->frame_count == 0) {
        printf("[AXIOM Renderer] draw_triangles: %d vertices (stub)\n",
               vertex_count);
    }
}

double axiom_renderer_get_time(void *renderer) {
    if (!renderer) return 0.0;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    long long now = axiom_clock_ns();
    return (double)(now - r->start_time_ns) / 1000000000.0;
}

void *axiom_shader_load(void *renderer, const char *spv_path, int stage) {
    if (!renderer || !spv_path) return NULL;
    (void)renderer;
    int i;
    for (i = 0; i < AXIOM_MAX_SHADERS; i++) {
        if (!axiom_shader_modules[i].active) {
            AxiomShaderModule *s = &axiom_shader_modules[i];
            s->active = 1;
            s->stage  = stage;
            size_t len = strlen(spv_path);
            if (len >= sizeof(s->path)) len = sizeof(s->path) - 1;
            memcpy(s->path, spv_path, len);
            s->path[len] = '\0';
            printf("[AXIOM Renderer] Loaded %s shader: \"%s\" (slot %d, stub)\n",
                   (stage == 0) ? "vertex" : "fragment", spv_path, i);
            return s;
        }
    }
    return NULL;
}

void *axiom_pipeline_create(void *renderer, void *vert_shader, void *frag_shader) {
    if (!renderer) return NULL;
    (void)renderer;
    int i;
    for (i = 0; i < AXIOM_MAX_PIPELINES; i++) {
        if (!axiom_pipelines[i].active) {
            AxiomPipeline *p = &axiom_pipelines[i];
            p->active = 1;
            p->vert_index = vert_shader
                ? (int)(((AxiomShaderModule *)vert_shader) - axiom_shader_modules)
                : -1;
            p->frag_index = frag_shader
                ? (int)(((AxiomShaderModule *)frag_shader) - axiom_shader_modules)
                : -1;
            printf("[AXIOM Renderer] Created pipeline %d (stub)\n", i);
            return p;
        }
    }
    return NULL;
}

void axiom_renderer_bind_pipeline(void *renderer, void *pipeline) {
    (void)renderer; (void)pipeline;
}

#endif /* _WIN32 / headless stub */
#endif /* !AXIOM_USE_WGPU_RENDERER */

/* ── Vec (Dynamic Array) ─────────────────────────────────────────── */
/*
 * Growable array backed by heap allocation.
 * Layout: { ptr data, i32 len, i32 cap, i32 elem_size }
 *
 * API:
 *   axiom_vec_new(elem_size)        -> ptr to vec header
 *   axiom_vec_push_i32(v, val)      -> push i32, auto-grow
 *   axiom_vec_push_f64(v, val)      -> push f64, auto-grow
 *   axiom_vec_get_i32(v, index)     -> indexed read (i32)
 *   axiom_vec_get_f64(v, index)     -> indexed read (f64)
 *   axiom_vec_set_i32(v, index, val)-> indexed write (i32)
 *   axiom_vec_set_f64(v, index, val)-> indexed write (f64)
 *   axiom_vec_len(v)                -> current length
 *   axiom_vec_free(v)               -> free data + header
 */

typedef struct {
    void *data;
    int   len;
    int   cap;
    int   elem_size;
} AxiomVec;

#define AXIOM_VEC_INITIAL_CAP 16

static void axiom_vec_grow(AxiomVec *v) {
    int new_cap = v->cap * 2;
    if (new_cap < AXIOM_VEC_INITIAL_CAP) new_cap = AXIOM_VEC_INITIAL_CAP;
    void *new_data = realloc(v->data, (size_t)new_cap * (size_t)v->elem_size);
    if (!new_data) {
        fprintf(stderr, "axiom_vec_grow: out of memory\n");
        abort();
    }
    v->data = new_data;
    v->cap  = new_cap;
}

void *axiom_vec_new(int elem_size) {
    AxiomVec *v = (AxiomVec *)malloc(sizeof(AxiomVec));
    if (!v) {
        fprintf(stderr, "axiom_vec_new: out of memory\n");
        abort();
    }
    v->len       = 0;
    v->cap       = AXIOM_VEC_INITIAL_CAP;
    v->elem_size = elem_size;
    v->data      = malloc((size_t)v->cap * (size_t)elem_size);
    if (!v->data) {
        fprintf(stderr, "axiom_vec_new: out of memory\n");
        free(v);
        abort();
    }
    return v;
}

void axiom_vec_push_i32(void *vec, int val) {
    AxiomVec *v = (AxiomVec *)vec;
    if (v->len >= v->cap) axiom_vec_grow(v);
    ((int *)v->data)[v->len] = val;
    v->len++;
}

void axiom_vec_push_f64(void *vec, double val) {
    AxiomVec *v = (AxiomVec *)vec;
    if (v->len >= v->cap) axiom_vec_grow(v);
    ((double *)v->data)[v->len] = val;
    v->len++;
}

int axiom_vec_get_i32(void *vec, int index) {
    AxiomVec *v = (AxiomVec *)vec;
    if (index < 0 || index >= v->len) {
        fprintf(stderr, "axiom_vec_get_i32: index %d out of bounds (len=%d)\n",
                index, v->len);
        abort();
    }
    return ((int *)v->data)[index];
}

double axiom_vec_get_f64(void *vec, int index) {
    AxiomVec *v = (AxiomVec *)vec;
    if (index < 0 || index >= v->len) {
        fprintf(stderr, "axiom_vec_get_f64: index %d out of bounds (len=%d)\n",
                index, v->len);
        abort();
    }
    return ((double *)v->data)[index];
}

void axiom_vec_set_i32(void *vec, int index, int val) {
    AxiomVec *v = (AxiomVec *)vec;
    if (index < 0 || index >= v->len) {
        fprintf(stderr, "axiom_vec_set_i32: index %d out of bounds (len=%d)\n",
                index, v->len);
        abort();
    }
    ((int *)v->data)[index] = val;
}

void axiom_vec_set_f64(void *vec, int index, double val) {
    AxiomVec *v = (AxiomVec *)vec;
    if (index < 0 || index >= v->len) {
        fprintf(stderr, "axiom_vec_set_f64: index %d out of bounds (len=%d)\n",
                index, v->len);
        abort();
    }
    ((double *)v->data)[index] = val;
}

int axiom_vec_len(void *vec) {
    AxiomVec *v = (AxiomVec *)vec;
    return v->len;
}

void axiom_vec_free(void *vec) {
    AxiomVec *v = (AxiomVec *)vec;
    if (v) {
        free(v->data);
        free(v);
    }
}

/* ── String (Fat Pointer) ────────────────────────────────────────── */
/*
 * Strings are packed into an i64 as a fat pointer:
 *   - Upper 32 bits: length (i32)
 *   - Lower 32 bits: pointer (truncated to 32 bits on 32-bit, or index on 64-bit)
 *
 * Actually, on 64-bit systems we cannot pack a 64-bit pointer into 32 bits.
 * Instead we use a different strategy: store strings in a table and return
 * an index packed with the length.  But for simplicity and the common case
 * (string literals whose pointers are known), we use a small string table.
 *
 * Encoding: (len << 32) | table_index
 *
 * API:
 *   axiom_string_from_literal(ptr) -> i64 (packed len + index)
 *   axiom_string_len(s)            -> i32
 *   axiom_string_ptr(s)            -> ptr
 *   axiom_string_eq(a, b)          -> i32 (1 if equal, 0 otherwise)
 *   axiom_string_print(s)          -> void (prints to stdout)
 */

#define AXIOM_STRING_TABLE_MAX 4096

static const char *axiom_string_table[AXIOM_STRING_TABLE_MAX];
static int axiom_string_table_len_arr[AXIOM_STRING_TABLE_MAX];
static int axiom_string_table_count = 0;

long long axiom_string_from_literal(const char *lit) {
    int idx = axiom_string_table_count;
    if (idx >= AXIOM_STRING_TABLE_MAX) {
        fprintf(stderr, "axiom_string_from_literal: string table full\n");
        abort();
    }
    int len = (int)strlen(lit);
    axiom_string_table[idx] = lit;
    axiom_string_table_len_arr[idx] = len;
    axiom_string_table_count++;
    return ((long long)len << 32) | (long long)(unsigned int)idx;
}

int axiom_string_len(long long s) {
    return (int)(s >> 32);
}

const char *axiom_string_ptr(long long s) {
    int idx = (int)(s & 0xFFFFFFFF);
    if (idx < 0 || idx >= axiom_string_table_count) return "";
    return axiom_string_table[idx];
}

int axiom_string_eq(long long a, long long b) {
    int len_a = (int)(a >> 32);
    int len_b = (int)(b >> 32);
    if (len_a != len_b) return 0;
    int idx_a = (int)(a & 0xFFFFFFFF);
    int idx_b = (int)(b & 0xFFFFFFFF);
    if (idx_a < 0 || idx_a >= axiom_string_table_count) return 0;
    if (idx_b < 0 || idx_b >= axiom_string_table_count) return 0;
    return memcmp(axiom_string_table[idx_a], axiom_string_table[idx_b],
                  (size_t)len_a) == 0 ? 1 : 0;
}

void axiom_string_print(long long s) {
    int len = (int)(s >> 32);
    int idx = (int)(s & 0xFFFFFFFF);
    if (idx < 0 || idx >= axiom_string_table_count) return;
    fwrite(axiom_string_table[idx], 1, (size_t)len, stdout);
    fputc('\n', stdout);
}

/* ── Input System ────────────────────────────────────────────────── */
/*
 * Tracks keyboard and mouse state. Updated by the Win32 WndProc
 * (or headless stubs on other platforms).
 *
 * API:
 *   axiom_is_key_down(key_code)   -> i32 (1 = pressed, 0 = released)
 *   axiom_get_mouse_x()           -> i32 (cursor x in client coordinates)
 *   axiom_get_mouse_y()           -> i32 (cursor y in client coordinates)
 *   axiom_is_mouse_down(button)   -> i32 (0=left, 1=right, 2=middle)
 */

static int axiom_key_state[256] = {0};  /* 1 = pressed, 0 = released */
static int axiom_mouse_x = 0;
static int axiom_mouse_y = 0;
static int axiom_mouse_buttons[3] = {0}; /* left, right, middle */

int axiom_is_key_down(int key_code) {
    return axiom_key_state[key_code & 0xFF];
}

int axiom_get_mouse_x(void) {
    return axiom_mouse_x;
}

int axiom_get_mouse_y(void) {
    return axiom_mouse_y;
}

int axiom_is_mouse_down(int button) {
    if (button < 0 || button > 2) return 0;
    return axiom_mouse_buttons[button];
}

/* ── Audio (Minimal) ─────────────────────────────────────────────── */
/*
 * Minimal audio builtins using platform-specific APIs.
 *
 * API:
 *   axiom_play_beep(freq, duration_ms)   -> void (Windows Beep)
 *   axiom_play_sound(path)               -> void (Windows PlaySound)
 */

#if defined(_WIN32)
/* windows.h already included above */
#pragma comment(lib, "winmm.lib")

void axiom_play_beep(int freq, int duration_ms) {
    Beep((DWORD)freq, (DWORD)duration_ms);
}

void axiom_play_sound(const char *path) {
    if (!path) return;
    PlaySoundA(path, NULL, SND_FILENAME | SND_ASYNC);
}
#else
/* POSIX stub — no audio support yet */
void axiom_play_beep(int freq, int duration_ms) {
    (void)freq; (void)duration_ms;
    /* printf("[AXIOM Audio] beep(%d, %d) — not supported on this platform\n", freq, duration_ms); */
}

void axiom_play_sound(const char *path) {
    (void)path;
    /* printf("[AXIOM Audio] play_sound — not supported on this platform\n"); */
}
#endif

/* ── CPUID Feature Detection ─────────────────────────────────────── */
/*
 * Returns a bitmask of available CPU features:
 *   Bit 0: SSE4.2
 *   Bit 1: AVX
 *   Bit 2: AVX2
 *   Bit 3: AVX-512F
 */

#if defined(_WIN32)
#include <intrin.h>
int axiom_cpu_features(void) {
    int info[4];
    int features = 0;
    __cpuid(info, 1);
    if (info[2] & (1 << 20)) features |= 1;  /* SSE4.2 */
    if (info[2] & (1 << 28)) features |= 2;  /* AVX */
    __cpuidex(info, 7, 0);
    if (info[1] & (1 << 5))  features |= 4;  /* AVX2 */
    if (info[1] & (1 << 16)) features |= 8;  /* AVX-512F */
    return features;
}
#elif defined(__x86_64__) || defined(__i386__)
#include <cpuid.h>
int axiom_cpu_features(void) {
    unsigned int eax, ebx, ecx, edx;
    int features = 0;
    if (__get_cpuid(1, &eax, &ebx, &ecx, &edx)) {
        if (ecx & (1 << 20)) features |= 1;  /* SSE4.2 */
        if (ecx & (1 << 28)) features |= 2;  /* AVX */
    }
    if (__get_cpuid_count(7, 0, &eax, &ebx, &ecx, &edx)) {
        if (ebx & (1 << 5))  features |= 4;  /* AVX2 */
        if (ebx & (1 << 16)) features |= 8;  /* AVX-512F */
    }
    return features;
}
#else
/* Non-x86 platforms: no SIMD features detected */
int axiom_cpu_features(void) {
    return 0;
}
#endif
