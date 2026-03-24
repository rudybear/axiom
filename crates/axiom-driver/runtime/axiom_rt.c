/*
 * axiom_rt.c -- Tiny C runtime for AXIOM I/O primitives, coroutines,
 * threading primitives, and a parallel job dispatch system.
 *
 * Provides file I/O, command-line arguments, a nanosecond clock,
 * stackful coroutines via OS fibers (Windows) or ucontext (POSIX),
 * thread creation/join, atomics, mutexes, and a thread-pool job system.
 * Linked only when the AXIOM program uses runtime builtins.
 */

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

#endif /* _WIN32 / POSIX -- threading */
