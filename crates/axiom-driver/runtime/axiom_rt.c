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

/* ── Split runtime modules ────────────────────────────────────────── */
/* Each section is in its own file for organization. They are included  */
/* here to maintain a single compilation unit.                         */

#include "axiom_rt_io.c"
#include "axiom_rt_core.c"

#include "axiom_rt_coroutines.c"



#include "axiom_rt_threading.c"



/* ── Input State (globals) ───────────────────────────────────────────── */
/* Declared here so the Win32 window procedure can update them, and the  */
/* input API functions (further below) can read them.                    */
static int axiom_key_state[256] = {0};
static int axiom_mouse_x = 0;
static int axiom_mouse_y = 0;
static int axiom_mouse_buttons[3] = {0};

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
 *   axiom_renderer_draw_triangles(r, pos, col, n)  Draw n vertices as tris (double*)
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
                                   const double *positions,
                                   const double *colors_f,
                                   int vertex_count) {
    if (!renderer || !positions) return;
    AxiomRenderer *r = (AxiomRenderer *)renderer;
    int w = r->width;
    int h = r->height;
    unsigned int *fb = r->framebuffer;

    /* Each triangle is 3 vertices, each vertex has 2 doubles (x, y)
       in the positions array, and 3 doubles (r, g, b) in the colors array. */
    int tri_count = vertex_count / 3;
    int t;
    for (t = 0; t < tri_count; t++) {
        int base_p = t * 6;  /* 3 vertices * 2 coords */
        int base_c = t * 9;  /* 3 vertices * 3 color channels */

        int x0 = (int)(positions[base_p + 0] + 0.5);
        int y0 = (int)(positions[base_p + 1] + 0.5);
        int x1 = (int)(positions[base_p + 2] + 0.5);
        int y1 = (int)(positions[base_p + 3] + 0.5);
        int x2 = (int)(positions[base_p + 4] + 0.5);
        int y2 = (int)(positions[base_p + 5] + 0.5);

        /* Flat color from first vertex (for simplicity). */
        unsigned int cr = 255, cg = 255, cb = 255;
        if (colors_f) {
            cr = (unsigned int)(colors_f[base_c + 0] * 255.0);
            cg = (unsigned int)(colors_f[base_c + 1] * 255.0);
            cb = (unsigned int)(colors_f[base_c + 2] * 255.0);
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
                                   const double *positions,
                                   const double *colors,
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

#include "axiom_rt_vec.c"

#include "axiom_rt_strings.c"

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

/* When using the wgpu renderer DLL, input functions come from the DLL.
   Only compile the C runtime input functions when NOT using the DLL. */
#ifndef AXIOM_USE_WGPU_RENDERER
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
#endif /* !AXIOM_USE_WGPU_RENDERER */

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
#include <mmsystem.h>
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

/* ── Crash Handler (debug mode only) ────────────────────────────── */

#ifdef AXIOM_DEBUG_MODE

#if defined(_WIN32)
#include <windows.h>
#include <dbghelp.h>

static LONG WINAPI axiom_crash_handler(EXCEPTION_POINTERS *ep) {
    fprintf(stderr, "\n=== AXIOM CRASH ===\n");
    fprintf(stderr, "Exception code: 0x%08lX\n",
            (unsigned long)ep->ExceptionRecord->ExceptionCode);

    HANDLE process = GetCurrentProcess();
    SymInitialize(process, NULL, TRUE);

    CONTEXT *ctx = ep->ContextRecord;
    STACKFRAME64 frame;
    memset(&frame, 0, sizeof(frame));
    frame.AddrPC.Offset    = ctx->Rip;
    frame.AddrPC.Mode      = AddrModeFlat;
    frame.AddrFrame.Offset = ctx->Rbp;
    frame.AddrFrame.Mode   = AddrModeFlat;
    frame.AddrStack.Offset = ctx->Rsp;
    frame.AddrStack.Mode   = AddrModeFlat;

    fprintf(stderr, "Stack trace:\n");
    for (int i = 0; i < 32; i++) {
        if (!StackWalk64(IMAGE_FILE_MACHINE_AMD64, process,
                         GetCurrentThread(), &frame, ctx, NULL,
                         SymFunctionTableAccess64, SymGetModuleBase64,
                         NULL))
            break;

        char symbol_buf[sizeof(SYMBOL_INFO) + 256];
        SYMBOL_INFO *symbol = (SYMBOL_INFO *)symbol_buf;
        symbol->SizeOfStruct = sizeof(SYMBOL_INFO);
        symbol->MaxNameLen   = 255;

        DWORD64 displacement;
        if (SymFromAddr(process, frame.AddrPC.Offset, &displacement,
                        symbol)) {
            fprintf(stderr, "  [%d] %s + 0x%llx\n", i, symbol->Name,
                    (unsigned long long)displacement);
        } else {
            fprintf(stderr, "  [%d] 0x%llx\n", i,
                    (unsigned long long)frame.AddrPC.Offset);
        }
    }

    SymCleanup(process);
    return EXCEPTION_EXECUTE_HANDLER;
}

void axiom_install_crash_handler(void) {
    SetUnhandledExceptionFilter(axiom_crash_handler);
}

#else /* POSIX */

#include <signal.h>
#if defined(__GLIBC__)
#include <execinfo.h>
#endif

static void axiom_crash_handler(int sig) {
    fprintf(stderr, "\n=== AXIOM CRASH (signal %d) ===\n", sig);
#if defined(__GLIBC__)
    void *frames[64];
    int n = backtrace(frames, 64);
    char **symbols = backtrace_symbols(frames, n);
    fprintf(stderr, "Stack trace:\n");
    for (int i = 0; i < n; i++) {
        fprintf(stderr, "  [%d] %s\n", i, symbols[i]);
    }
    free(symbols);
#else
    fprintf(stderr, "(stack trace not available on this platform)\n");
#endif
    _exit(128 + sig);
}

void axiom_install_crash_handler(void) {
    signal(SIGSEGV, axiom_crash_handler);
    signal(SIGABRT, axiom_crash_handler);
    signal(SIGFPE,  axiom_crash_handler);
}

#endif /* _WIN32 */

#else /* !AXIOM_DEBUG_MODE -- provide a stub so the linker is happy */

void axiom_install_crash_handler(void) {
    /* no-op when debug mode is not enabled */
}

#endif /* AXIOM_DEBUG_MODE */
