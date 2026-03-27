/*
 * axiom_rt_coroutines.c -- Stackful coroutine runtime.
 *
 * Provides: axiom_coro_create, axiom_coro_resume, axiom_coro_yield,
 *           axiom_coro_is_done, axiom_coro_destroy
 *
 * Included by axiom_rt.c -- do not compile separately.
 */

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
