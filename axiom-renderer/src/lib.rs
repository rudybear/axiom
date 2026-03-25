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
pub mod lux_shaders;
pub mod camera;
pub mod gltf_load;
pub mod pbr;

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

// ---------------------------------------------------------------------------
// Lux shader integration — C ABI exports
// ---------------------------------------------------------------------------

/// Load a Lux-compiled SPIR-V shader from disk.
///
/// - `path`: null-terminated path to a `.spv` file
/// - `stage`: 0 = vertex, 1 = fragment
///
/// Returns an opaque handle to the loaded shader, or null on failure.
/// The handle must eventually be freed with `axiom_shader_destroy`.
#[no_mangle]
pub unsafe extern "C" fn axiom_shader_load(
    _renderer: *mut std::ffi::c_void,
    path: *const c_char,
    stage: c_int,
) -> *mut std::ffi::c_void {
    if path.is_null() {
        eprintln!("[Lux Shader] Error: null path");
        return std::ptr::null_mut();
    }

    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[Lux Shader] Error: invalid UTF-8 path: {e}");
            return std::ptr::null_mut();
        }
    };

    let shader_stage = match lux_shaders::ShaderStage::from_int(stage) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[Lux Shader] Error: {e}");
            return std::ptr::null_mut();
        }
    };

    match lux_shaders::load_shader(path_str, shader_stage) {
        Ok(shader) => Box::into_raw(Box::new(shader)) as *mut std::ffi::c_void,
        Err(e) => {
            eprintln!("[Lux Shader] Error loading {path_str}: {e}");
            std::ptr::null_mut()
        }
    }
}

/// Destroy a loaded shader handle.
#[no_mangle]
pub unsafe extern "C" fn axiom_shader_destroy(shader: *mut std::ffi::c_void) {
    if !shader.is_null() {
        drop(Box::from_raw(shader as *mut lux_shaders::LuxShader));
    }
}

/// Create a render pipeline from a vertex shader and fragment shader.
///
/// Both `vert_shader` and `frag_shader` must be handles returned by
/// `axiom_shader_load`. Returns an opaque pipeline handle (encoded as the
/// pipeline's numeric ID), or null on failure.
///
/// The returned handle is valid for the lifetime of the renderer — it does
/// not need to be manually freed.
#[no_mangle]
pub unsafe extern "C" fn axiom_pipeline_create(
    _renderer: *mut std::ffi::c_void,
    vert_shader: *mut std::ffi::c_void,
    frag_shader: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    if vert_shader.is_null() || frag_shader.is_null() {
        eprintln!("[Lux Shader] Error: null shader handle passed to pipeline_create");
        return std::ptr::null_mut();
    }

    let vert = &*(vert_shader as *const lux_shaders::LuxShader);
    let frag = &*(frag_shader as *const lux_shaders::LuxShader);

    with_renderer(std::ptr::null_mut(), |r| {
        let device = r.device();
        let format = r.surface_format();

        match lux_shaders::create_render_pipeline(device, format, vert, frag) {
            Ok(lux_pipeline) => {
                let id = r.add_lux_pipeline(lux_pipeline);
                // Return the pipeline ID as a pointer-sized handle
                id as usize as *mut std::ffi::c_void
            }
            Err(e) => {
                eprintln!("[Lux Shader] Error creating pipeline: {e}");
                std::ptr::null_mut()
            }
        }
    })
}

/// Bind a Lux pipeline for subsequent draw calls.
///
/// Pass the handle returned by `axiom_pipeline_create`.
/// Pass null (0) to revert to the built-in pipeline.
#[no_mangle]
pub unsafe extern "C" fn axiom_renderer_bind_pipeline(
    _renderer: *mut std::ffi::c_void,
    pipeline: *mut std::ffi::c_void,
) {
    let id = pipeline as u64;
    with_renderer((), |r| r.bind_pipeline(id));
}

// ---------------------------------------------------------------------------
// GPU PBR / glTF API — 10 new C ABI functions
// ---------------------------------------------------------------------------

/// Create a GPU renderer context with a window. Returns a non-null opaque
/// handle on success, null on failure.
#[no_mangle]
pub unsafe extern "C" fn gpu_init(
    w: c_int,
    h: c_int,
    title: *const c_char,
) -> *mut std::ffi::c_void {
    let title_str = if title.is_null() {
        "AXIOM"
    } else {
        CStr::from_ptr(title).to_str().unwrap_or("AXIOM")
    };

    let width = if w > 0 { w as u32 } else { 800 };
    let height = if h > 0 { h as u32 } else { 600 };

    match renderer::Renderer::new(width, height, title_str) {
        Ok(r) => {
            match RENDERER.lock() {
                Ok(mut guard) => {
                    *guard = Some(r);
                    1usize as *mut std::ffi::c_void
                }
                Err(e) => {
                    eprintln!("[AXIOM GPU] Lock error: {e}");
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            eprintln!("[AXIOM GPU] Error creating renderer: {e}");
            std::ptr::null_mut()
        }
    }
}

/// Destroy the GPU renderer, free all resources, close the window.
#[no_mangle]
pub unsafe extern "C" fn gpu_shutdown(_handle: *mut std::ffi::c_void) {
    match RENDERER.lock() {
        Ok(mut guard) => {
            if let Some(mut r) = guard.take() {
                r.destroy();
            }
        }
        Err(e) => eprintln!("[AXIOM GPU] Lock error: {e}"),
    }
}

/// Begin a new frame: poll window events, prepare for rendering.
/// Returns 1 if rendering can proceed, 0 if the window was closed.
#[no_mangle]
pub unsafe extern "C" fn gpu_begin_frame(_handle: *mut std::ffi::c_void) -> c_int {
    with_renderer(0, |r| if r.begin_frame_timed() { 1 } else { 0 })
}

/// End the frame: present to screen and record frame timing.
#[no_mangle]
pub unsafe extern "C" fn gpu_end_frame(_handle: *mut std::ffi::c_void) {
    with_renderer((), |r| r.end_frame_timed());
}

/// Query whether the window close button was pressed.
/// Returns 1 if yes, 0 if no.
#[no_mangle]
pub unsafe extern "C" fn gpu_should_close(_handle: *mut std::ffi::c_void) -> c_int {
    with_renderer(1, |r| if r.should_close() { 1 } else { 0 })
}

/// Load a glTF/GLB file and upload its meshes, materials, and textures to the GPU.
/// Returns a scene ID > 0 on success, 0 on failure.
#[no_mangle]
pub unsafe extern "C" fn gpu_load_gltf(
    _handle: *mut std::ffi::c_void,
    path: *const c_char,
) -> c_int {
    if path.is_null() {
        eprintln!("[AXIOM GPU] Error: null path passed to gpu_load_gltf");
        return 0;
    }

    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[AXIOM GPU] Error: invalid UTF-8 path: {e}");
            return 0;
        }
    };

    with_renderer(0, |r| r.load_gltf(path_str) as c_int)
}

/// Set the camera position, look-at target, and vertical FOV (degrees).
/// Parameters are f64 from AXIOM (which uses f64 for all floats), cast to f32 internally.
#[no_mangle]
pub unsafe extern "C" fn gpu_set_camera(
    _handle: *mut std::ffi::c_void,
    ex: c_double,
    ey: c_double,
    ez: c_double,
    tx: c_double,
    ty: c_double,
    tz: c_double,
    fov: c_double,
) {
    with_renderer((), |r| {
        r.set_camera(
            [ex as f32, ey as f32, ez as f32],
            [tx as f32, ty as f32, tz as f32],
            fov as f32,
        );
    });
}

/// Render the currently loaded scene with the current camera.
/// Must be called between gpu_begin_frame() and gpu_end_frame().
#[no_mangle]
pub unsafe extern "C" fn gpu_render(_handle: *mut std::ffi::c_void) {
    with_renderer((), |r| r.render_scene());
}

/// Capture a screenshot of the current frame and save to a PNG file.
/// Returns 0 on success, non-zero on failure.
#[no_mangle]
pub unsafe extern "C" fn gpu_screenshot(
    _handle: *mut std::ffi::c_void,
    path: *const c_char,
) -> c_int {
    if path.is_null() {
        eprintln!("[AXIOM GPU] Error: null path passed to gpu_screenshot");
        return 1;
    }

    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[AXIOM GPU] Error: invalid UTF-8 path: {e}");
            return 1;
        }
    };

    with_renderer(1, |r| {
        match r.screenshot(path_str) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("[AXIOM GPU] Screenshot error: {e}");
                1
            }
        }
    })
}

/// Returns the time (in seconds) of the last completed frame.
#[no_mangle]
pub unsafe extern "C" fn gpu_get_frame_time(_handle: *mut std::ffi::c_void) -> c_double {
    with_renderer(0.0, |r| r.last_frame_time())
}

/// Returns the GPU adapter name as a null-terminated C string.
/// The pointer is valid until gpu_shutdown().
#[no_mangle]
pub unsafe extern "C" fn gpu_get_gpu_name(
    _handle: *mut std::ffi::c_void,
) -> *const c_char {
    with_renderer(std::ptr::null(), |r| r.gpu_name_cstr())
}

// ---------------------------------------------------------------------------
// G2: Input System — C ABI exports
// ---------------------------------------------------------------------------

/// Check if a key is currently pressed.
/// Returns 1 if pressed, 0 if not.
#[no_mangle]
pub unsafe extern "C" fn axiom_is_key_down(key_code: c_int) -> c_int {
    with_renderer(0, |r| if r.is_key_down(key_code) { 1 } else { 0 })
}

/// Get the current mouse X position in client coordinates.
#[no_mangle]
pub unsafe extern "C" fn axiom_get_mouse_x() -> c_int {
    with_renderer(0, |r| r.get_mouse_x())
}

/// Get the current mouse Y position in client coordinates.
#[no_mangle]
pub unsafe extern "C" fn axiom_get_mouse_y() -> c_int {
    with_renderer(0, |r| r.get_mouse_y())
}

/// Check if a mouse button is pressed (0=left, 1=right, 2=middle).
/// Returns 1 if pressed, 0 if not.
#[no_mangle]
pub unsafe extern "C" fn axiom_is_mouse_down(button: c_int) -> c_int {
    with_renderer(0, |r| if r.is_mouse_down(button) { 1 } else { 0 })
}

// ---------------------------------------------------------------------------
// R5: Multi-light support — C ABI exports
// ---------------------------------------------------------------------------

/// Add a point light to the scene.
/// Returns 1 on success, 0 if max lights (8) already reached.
#[no_mangle]
pub unsafe extern "C" fn gpu_add_light(
    _handle: *mut std::ffi::c_void,
    x: c_double, y: c_double, z: c_double,
    r: c_double, g: c_double, b: c_double,
    intensity: c_double,
) -> c_int {
    with_renderer(0, |renderer| {
        if renderer.add_light(
            x as f32, y as f32, z as f32,
            r as f32, g as f32, b as f32,
            intensity as f32,
        ) { 1 } else { 0 }
    })
}

/// Clear all point lights.
#[no_mangle]
pub unsafe extern "C" fn gpu_clear_lights(_handle: *mut std::ffi::c_void) {
    with_renderer((), |r| r.clear_lights());
}

// ---------------------------------------------------------------------------
// R5: Instanced rendering — C ABI exports
// ---------------------------------------------------------------------------

/// Draw the loaded scene instanced with the provided transform matrices.
/// `transforms_ptr` points to `count` contiguous 4x4 column-major f32 matrices (16 floats each).
/// `scene_id` is currently ignored (uses the loaded scene).
#[no_mangle]
pub unsafe extern "C" fn gpu_draw_instanced(
    _handle: *mut std::ffi::c_void,
    _scene_id: c_int,
    transforms_ptr: *const c_float,
    count: c_int,
) {
    if transforms_ptr.is_null() || count <= 0 {
        return;
    }
    let n = count as usize;
    let raw = std::slice::from_raw_parts(transforms_ptr, n * 16);
    // Reinterpret as array of [f32; 16]
    let transforms: Vec<[f32; 16]> = raw
        .chunks_exact(16)
        .map(|chunk| {
            let mut arr = [0.0f32; 16];
            arr.copy_from_slice(chunk);
            arr
        })
        .collect();
    with_renderer((), |r| r.render_scene_instanced(&transforms, count as u32));
}
