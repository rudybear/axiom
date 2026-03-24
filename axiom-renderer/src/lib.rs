// axiom-renderer/src/lib.rs
//
// C ABI exports for the AXIOM renderer. AXIOM programs (compiled via the
// axiom-driver pipeline) link against the resulting cdylib (axiom_renderer.dll
// on Windows). The function signatures match those in axiom_rt.c so the two
// backends are interchangeable.
//
// Thread-safety: the renderer is stored in a global Mutex. All functions must
// be called from the main thread (winit requirement), but the Mutex protects
// against accidental concurrent access.

use std::ffi::{c_char, c_double, c_float, c_int, c_uint, CStr};
use std::sync::Mutex;

mod renderer;

// ---------------------------------------------------------------------------
// Global renderer state
// ---------------------------------------------------------------------------

static RENDERER: Mutex<Option<renderer::Renderer>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn with_renderer<F, R>(default: R, f: F) -> R
where
    F: FnOnce(&mut renderer::Renderer) -> R,
{
    match RENDERER.lock() {
        Ok(mut guard) => match guard.as_mut() {
            Some(r) => f(r),
            None => {
                eprintln!("[AXIOM Renderer] Error: renderer not initialized");
                default
            }
        },
        Err(e) => {
            eprintln!("[AXIOM Renderer] Lock error: {e}");
            default
        }
    }
}

// ---------------------------------------------------------------------------
// C ABI exports
// ---------------------------------------------------------------------------

/// Create a renderer context with a window of the given dimensions.
/// Returns a non-null opaque handle on success, null on failure.
/// (We return 1/0 as a pointer since the actual state is global.)
#[no_mangle]
pub unsafe extern "C" fn axiom_renderer_create(
    width: c_int,
    height: c_int,
    title: *const c_char,
) -> *mut std::ffi::c_void {
    let title_str = if title.is_null() {
        "AXIOM"
    } else {
        CStr::from_ptr(title).to_str().unwrap_or("AXIOM")
    };

    let w = if width > 0 { width as u32 } else { 800 };
    let h = if height > 0 { height as u32 } else { 600 };

    match renderer::Renderer::new(w, h, title_str) {
        Ok(r) => {
            match RENDERER.lock() {
                Ok(mut guard) => {
                    *guard = Some(r);
                    // Return a sentinel non-null pointer (the global is the real state)
                    1usize as *mut std::ffi::c_void
                }
                Err(e) => {
                    eprintln!("[AXIOM Renderer] Lock error: {e}");
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            eprintln!("[AXIOM Renderer] Error creating renderer: {e}");
            std::ptr::null_mut()
        }
    }
}

/// Destroy the renderer context.
#[no_mangle]
pub unsafe extern "C" fn axiom_renderer_destroy(_renderer: *mut std::ffi::c_void) {
    match RENDERER.lock() {
        Ok(mut guard) => {
            if let Some(mut r) = guard.take() {
                r.destroy();
            }
        }
        Err(e) => eprintln!("[AXIOM Renderer] Lock error: {e}"),
    }
}

/// Begin a new frame. Returns 1 if OK, 0 if the window should close.
#[no_mangle]
pub unsafe extern "C" fn axiom_renderer_begin_frame(
    _renderer: *mut std::ffi::c_void,
) -> c_int {
    with_renderer(0, |r| if r.begin_frame() { 1 } else { 0 })
}

/// End the current frame (submit and present).
#[no_mangle]
pub unsafe extern "C" fn axiom_renderer_end_frame(
    _renderer: *mut std::ffi::c_void,
) {
    with_renderer((), |r| r.end_frame());
}

/// Returns 1 if the window should close, 0 otherwise.
#[no_mangle]
pub unsafe extern "C" fn axiom_renderer_should_close(
    _renderer: *mut std::ffi::c_void,
) -> c_int {
    with_renderer(1, |r| if r.should_close() { 1 } else { 0 })
}

/// Clear the framebuffer to the given color (0xRRGGBB).
#[no_mangle]
pub unsafe extern "C" fn axiom_renderer_clear(
    _renderer: *mut std::ffi::c_void,
    color: c_uint,
) {
    with_renderer((), |r| r.clear(color));
}

/// Draw colored points.
///
/// - x_arr, y_arr: arrays of f64 pixel coordinates
/// - colors: array of u32 (0xRRGGBB)
/// - count: number of points
#[no_mangle]
pub unsafe extern "C" fn axiom_renderer_draw_points(
    _renderer: *mut std::ffi::c_void,
    x_arr: *const c_double,
    y_arr: *const c_double,
    colors: *const c_uint,
    count: c_int,
) {
    if x_arr.is_null() || y_arr.is_null() || colors.is_null() || count <= 0 {
        return;
    }
    let n = count as usize;
    let xs = std::slice::from_raw_parts(x_arr, n);
    let ys = std::slice::from_raw_parts(y_arr, n);
    let cs = std::slice::from_raw_parts(colors, n);

    with_renderer((), |r| r.draw_points(xs, ys, cs, n));
}

/// Draw colored triangles.
///
/// - positions: array of f32, [x0,y0, x1,y1, x2,y2, ...] in pixel coordinates
/// - colors_f: array of f32, [r0,g0,b0, r1,g1,b1, ...] in [0,1] range (may be null)
/// - vertex_count: number of vertices (must be a multiple of 3)
#[no_mangle]
pub unsafe extern "C" fn axiom_renderer_draw_triangles(
    _renderer: *mut std::ffi::c_void,
    positions: *const c_float,
    colors_f: *const c_float,
    vertex_count: c_int,
) {
    if positions.is_null() || vertex_count <= 0 {
        return;
    }
    let n = vertex_count as usize;
    let pos = std::slice::from_raw_parts(positions, n * 2);
    let cols = if colors_f.is_null() {
        None
    } else {
        Some(std::slice::from_raw_parts(colors_f, n * 3))
    };

    with_renderer((), |r| r.draw_triangles(pos, cols, n));
}

/// Get elapsed time in seconds since renderer creation.
#[no_mangle]
pub unsafe extern "C" fn axiom_renderer_get_time(
    _renderer: *mut std::ffi::c_void,
) -> c_double {
    with_renderer(0.0, |r| r.get_time())
}
