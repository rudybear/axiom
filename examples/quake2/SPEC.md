# Quake 2 OpenGL Renderer -- ARCHITECT Specification

## Overview

A pure-AXIOM OpenGL 3.3 renderer for Quake 2 BSP maps. Two components:

1. **`q2_gl_loader.c`** (~120 lines) -- Tiny C DLL that loads GL 3.3 function pointers via `wglGetProcAddress` and re-exports them as plain C functions. NO game logic.
2. **Pure AXIOM modules** -- ALL rendering code (window, shaders, textures, meshes, camera, input, drawing).

The existing Q2 modules (q2_pak, q2_bsp, q2_texture, q2_lightmap, q2_camera, q2_render, quake2) remain unchanged. This spec covers only the OpenGL infrastructure layer they call into.

---

## File List and Responsibilities

```
examples/quake2/
  q2_gl_loader.c          -- C DLL: loads ~30 GL 3.3 functions, re-exports as C ABI
  q2_gl_extern.axm        -- extern declarations for Win32, GL 1.1, GL 3.3 loader, WGL
  q2_gl_constants.axm     -- All GL/Win32 constants as @pure fn
  q2_window.axm           -- Win32 window creation, pixel format, WGL context, message pump
  q2_shader.axm           -- GLSL shader compilation, program linking, uniform cache
  q2_gpu.axm              -- Texture upload, mesh creation (VAO/VBO), draw calls, camera
  q2_input.axm            -- Keyboard state, mouse deltas, cursor grab via raw input
  q2_math.axm             -- mat4 multiply, look_at, perspective, inverse (pure AXIOM)
```

Build command:
```bash
# 1. Build the GL loader DLL
cl /LD q2_gl_loader.c opengl32.lib /Fe:q2_gl_loader.dll
# or with gcc:
gcc -shared -o q2_gl_loader.dll q2_gl_loader.c -lopengl32

# 2. Compile the AXIOM program (links opengl32.dll, user32.dll, gdi32.dll, q2_gl_loader.dll)
axiom compile quake2.axm -o quake2.exe
```

---

## 1. q2_gl_loader.c -- Complete Code

```c
/*
 * q2_gl_loader.c -- Minimal GL 3.3 function pointer loader for AXIOM Quake 2
 *
 * Build: cl /LD q2_gl_loader.c opengl32.lib /Fe:q2_gl_loader.dll
 *    or: gcc -shared -o q2_gl_loader.dll q2_gl_loader.c -lopengl32
 *
 * This DLL loads ~30 GL 3.3 extension functions via wglGetProcAddress and
 * re-exports each as a plain C function. No game logic. Just infrastructure.
 */

#include <windows.h>
#include <GL/gl.h>

/* GL 3.3 types not in gl.h */
typedef char GLchar;
typedef ptrdiff_t GLsizeiptr;
typedef ptrdiff_t GLintptr;

/* Function pointer typedefs */
typedef void   (APIENTRY *PFNGLGENVERTEXARRAYSPROC)(GLsizei n, GLuint *arrays);
typedef void   (APIENTRY *PFNGLBINDVERTEXARRAYPROC)(GLuint array);
typedef void   (APIENTRY *PFNGLDELETEVERTEXARRAYSPROC)(GLsizei n, const GLuint *arrays);
typedef void   (APIENTRY *PFNGLGENBUFFERSPROC)(GLsizei n, GLuint *buffers);
typedef void   (APIENTRY *PFNGLBINDBUFFERPROC)(GLenum target, GLuint buffer);
typedef void   (APIENTRY *PFNGLBUFFERDATAPROC)(GLenum target, GLsizeiptr size, const void *data, GLenum usage);
typedef void   (APIENTRY *PFNGLDELETEBUFFERSPROC)(GLsizei n, const GLuint *buffers);
typedef void   (APIENTRY *PFNGLENABLEVERTEXATTRIBARRAYPROC)(GLuint index);
typedef void   (APIENTRY *PFNGLVERTEXATTRIBPOINTERPROC)(GLuint index, GLint size, GLenum type, GLboolean normalized, GLsizei stride, const void *pointer);
typedef GLuint (APIENTRY *PFNGLCREATESHADERPROC)(GLenum type);
typedef void   (APIENTRY *PFNGLSHADERSOURCEPROC)(GLuint shader, GLsizei count, const GLchar **string, const GLint *length);
typedef void   (APIENTRY *PFNGLCOMPILESHADERPROC)(GLuint shader);
typedef void   (APIENTRY *PFNGLGETSHADERIVPROC)(GLuint shader, GLenum pname, GLint *params);
typedef void   (APIENTRY *PFNGLGETSHADERINFOLOGPROC)(GLuint shader, GLsizei maxLength, GLsizei *length, GLchar *infoLog);
typedef GLuint (APIENTRY *PFNGLCREATEPROGRAMPROC)(void);
typedef void   (APIENTRY *PFNGLATTACHSHADERPROC)(GLuint program, GLuint shader);
typedef void   (APIENTRY *PFNGLLINKPROGRAMPROC)(GLuint program);
typedef void   (APIENTRY *PFNGLGETPROGRAMIVPROC)(GLuint program, GLenum pname, GLint *params);
typedef void   (APIENTRY *PFNGLGETPROGRAMINFOLOGPROC)(GLuint program, GLsizei maxLength, GLsizei *length, GLchar *infoLog);
typedef void   (APIENTRY *PFNGLUSEPROGRAMPROC)(GLuint program);
typedef GLint  (APIENTRY *PFNGLGETUNIFORMLOCATIONPROC)(GLuint program, const GLchar *name);
typedef void   (APIENTRY *PFNGLUNIFORMMATRIX4FVPROC)(GLint location, GLsizei count, GLboolean transpose, const GLfloat *value);
typedef void   (APIENTRY *PFNGLUNIFORM1IPROC)(GLint location, GLint v0);
typedef void   (APIENTRY *PFNGLUNIFORM1FPROC)(GLint location, GLfloat v0);
typedef void   (APIENTRY *PFNGLUNIFORM3FPROC)(GLint location, GLfloat v0, GLfloat v1, GLfloat v2);
typedef void   (APIENTRY *PFNGLUNIFORM4FPROC)(GLint location, GLfloat v0, GLfloat v1, GLfloat v2, GLfloat v3);
typedef void   (APIENTRY *PFNGLACTIVETEXTUREPROC)(GLenum texture);
typedef void   (APIENTRY *PFNGLGENERATEMIPMAPPROC)(GLenum target);
typedef void   (APIENTRY *PFNGLDELETESHADERPROC)(GLuint shader);
typedef void   (APIENTRY *PFNGLDELETEPROGRAMPROC)(GLuint program);

/* Static function pointers */
static PFNGLGENVERTEXARRAYSPROC       _glGenVertexArrays;
static PFNGLBINDVERTEXARRAYPROC       _glBindVertexArray;
static PFNGLDELETEVERTEXARRAYSPROC    _glDeleteVertexArrays;
static PFNGLGENBUFFERSPROC            _glGenBuffers;
static PFNGLBINDBUFFERPROC            _glBindBuffer;
static PFNGLBUFFERDATAPROC            _glBufferData;
static PFNGLDELETEBUFFERSPROC         _glDeleteBuffers;
static PFNGLENABLEVERTEXATTRIBARRAYPROC _glEnableVertexAttribArray;
static PFNGLVERTEXATTRIBPOINTERPROC   _glVertexAttribPointer;
static PFNGLCREATESHADERPROC          _glCreateShader;
static PFNGLSHADERSOURCEPROC          _glShaderSource;
static PFNGLCOMPILESHADERPROC         _glCompileShader;
static PFNGLGETSHADERIVPROC           _glGetShaderiv;
static PFNGLGETSHADERINFOLOGPROC      _glGetShaderInfoLog;
static PFNGLCREATEPROGRAMPROC         _glCreateProgram;
static PFNGLATTACHSHADERPROC          _glAttachShader;
static PFNGLLINKPROGRAMPROC           _glLinkProgram;
static PFNGLGETPROGRAMIVPROC          _glGetProgramiv;
static PFNGLGETPROGRAMINFOLOGPROC     _glGetProgramInfoLog;
static PFNGLUSEPROGRAMPROC            _glUseProgram;
static PFNGLGETUNIFORMLOCATIONPROC    _glGetUniformLocation;
static PFNGLUNIFORMMATRIX4FVPROC      _glUniformMatrix4fv;
static PFNGLUNIFORM1IPROC            _glUniform1i;
static PFNGLUNIFORM1FPROC            _glUniform1f;
static PFNGLUNIFORM3FPROC            _glUniform3f;
static PFNGLUNIFORM4FPROC            _glUniform4f;
static PFNGLACTIVETEXTUREPROC         _glActiveTexture;
static PFNGLGENERATEMIPMAPPROC        _glGenerateMipmap;
static PFNGLDELETESHADERPROC          _glDeleteShader;
static PFNGLDELETEPROGRAMPROC         _glDeleteProgram;

/* Load all function pointers. Call AFTER wglMakeCurrent. */
__declspec(dllexport) void gl_LoadFunctions(void) {
    _glGenVertexArrays       = (PFNGLGENVERTEXARRAYSPROC)      wglGetProcAddress("glGenVertexArrays");
    _glBindVertexArray       = (PFNGLBINDVERTEXARRAYPROC)      wglGetProcAddress("glBindVertexArray");
    _glDeleteVertexArrays    = (PFNGLDELETEVERTEXARRAYSPROC)   wglGetProcAddress("glDeleteVertexArrays");
    _glGenBuffers            = (PFNGLGENBUFFERSPROC)           wglGetProcAddress("glGenBuffers");
    _glBindBuffer            = (PFNGLBINDBUFFERPROC)           wglGetProcAddress("glBindBuffer");
    _glBufferData            = (PFNGLBUFFERDATAPROC)           wglGetProcAddress("glBufferData");
    _glDeleteBuffers         = (PFNGLDELETEBUFFERSPROC)        wglGetProcAddress("glDeleteBuffers");
    _glEnableVertexAttribArray = (PFNGLENABLEVERTEXATTRIBARRAYPROC)wglGetProcAddress("glEnableVertexAttribArray");
    _glVertexAttribPointer   = (PFNGLVERTEXATTRIBPOINTERPROC)  wglGetProcAddress("glVertexAttribPointer");
    _glCreateShader          = (PFNGLCREATESHADERPROC)         wglGetProcAddress("glCreateShader");
    _glShaderSource          = (PFNGLSHADERSOURCEPROC)         wglGetProcAddress("glShaderSource");
    _glCompileShader         = (PFNGLCOMPILESHADERPROC)        wglGetProcAddress("glCompileShader");
    _glGetShaderiv           = (PFNGLGETSHADERIVPROC)          wglGetProcAddress("glGetShaderiv");
    _glGetShaderInfoLog      = (PFNGLGETSHADERINFOLOGPROC)     wglGetProcAddress("glGetShaderInfoLog");
    _glCreateProgram         = (PFNGLCREATEPROGRAMPROC)        wglGetProcAddress("glCreateProgram");
    _glAttachShader          = (PFNGLATTACHSHADERPROC)         wglGetProcAddress("glAttachShader");
    _glLinkProgram           = (PFNGLLINKPROGRAMPROC)          wglGetProcAddress("glLinkProgram");
    _glGetProgramiv          = (PFNGLGETPROGRAMIVPROC)         wglGetProcAddress("glGetProgramiv");
    _glGetProgramInfoLog     = (PFNGLGETPROGRAMINFOLOGPROC)    wglGetProcAddress("glGetProgramInfoLog");
    _glUseProgram            = (PFNGLUSEPROGRAMPROC)           wglGetProcAddress("glUseProgram");
    _glGetUniformLocation    = (PFNGLGETUNIFORMLOCATIONPROC)   wglGetProcAddress("glGetUniformLocation");
    _glUniformMatrix4fv      = (PFNGLUNIFORMMATRIX4FVPROC)     wglGetProcAddress("glUniformMatrix4fv");
    _glUniform1i             = (PFNGLUNIFORM1IPROC)            wglGetProcAddress("glUniform1i");
    _glUniform1f             = (PFNGLUNIFORM1FPROC)            wglGetProcAddress("glUniform1f");
    _glUniform3f             = (PFNGLUNIFORM3FPROC)            wglGetProcAddress("glUniform3f");
    _glUniform4f             = (PFNGLUNIFORM4FPROC)            wglGetProcAddress("glUniform4f");
    _glActiveTexture         = (PFNGLACTIVETEXTUREPROC)        wglGetProcAddress("glActiveTexture");
    _glGenerateMipmap        = (PFNGLGENERATEMIPMAPPROC)       wglGetProcAddress("glGenerateMipmap");
    _glDeleteShader          = (PFNGLDELETESHADERPROC)         wglGetProcAddress("glDeleteShader");
    _glDeleteProgram         = (PFNGLDELETEPROGRAMPROC)        wglGetProcAddress("glDeleteProgram");
}

/* Re-export each function with C ABI. Names prefixed gl_ to avoid collision. */
__declspec(dllexport) void   gl_GenVertexArrays(int n, unsigned int *arrays)     { _glGenVertexArrays(n, arrays); }
__declspec(dllexport) void   gl_BindVertexArray(unsigned int array)              { _glBindVertexArray(array); }
__declspec(dllexport) void   gl_DeleteVertexArrays(int n, const unsigned int *a) { _glDeleteVertexArrays(n, a); }
__declspec(dllexport) void   gl_GenBuffers(int n, unsigned int *buffers)         { _glGenBuffers(n, buffers); }
__declspec(dllexport) void   gl_BindBuffer(unsigned int target, unsigned int buf){ _glBindBuffer(target, buf); }
__declspec(dllexport) void   gl_BufferData(unsigned int target, long long size, const void *data, unsigned int usage) { _glBufferData(target, (GLsizeiptr)size, data, usage); }
__declspec(dllexport) void   gl_DeleteBuffers(int n, const unsigned int *bufs)   { _glDeleteBuffers(n, bufs); }
__declspec(dllexport) void   gl_EnableVertexAttribArray(unsigned int index)       { _glEnableVertexAttribArray(index); }
__declspec(dllexport) void   gl_VertexAttribPointer(unsigned int idx, int size, unsigned int type, unsigned char norm, int stride, long long offset) { _glVertexAttribPointer(idx, size, type, norm, stride, (const void*)offset); }
__declspec(dllexport) unsigned int gl_CreateShader(unsigned int type)             { return _glCreateShader(type); }
__declspec(dllexport) void   gl_ShaderSource(unsigned int shader, int count, const char **string, const int *length) { _glShaderSource(shader, count, (const GLchar**)string, length); }
__declspec(dllexport) void   gl_CompileShader(unsigned int shader)               { _glCompileShader(shader); }
__declspec(dllexport) void   gl_GetShaderiv(unsigned int shader, unsigned int pname, int *params) { _glGetShaderiv(shader, pname, params); }
__declspec(dllexport) void   gl_GetShaderInfoLog(unsigned int shader, int max, int *len, char *log) { _glGetShaderInfoLog(shader, max, len, (GLchar*)log); }
__declspec(dllexport) unsigned int gl_CreateProgram(void)                        { return _glCreateProgram(); }
__declspec(dllexport) void   gl_AttachShader(unsigned int prog, unsigned int sh) { _glAttachShader(prog, sh); }
__declspec(dllexport) void   gl_LinkProgram(unsigned int program)                { _glLinkProgram(program); }
__declspec(dllexport) void   gl_GetProgramiv(unsigned int prog, unsigned int pname, int *params) { _glGetProgramiv(prog, pname, params); }
__declspec(dllexport) void   gl_GetProgramInfoLog(unsigned int prog, int max, int *len, char *log) { _glGetProgramInfoLog(prog, max, len, (GLchar*)log); }
__declspec(dllexport) void   gl_UseProgram(unsigned int program)                 { _glUseProgram(program); }
__declspec(dllexport) int    gl_GetUniformLocation(unsigned int prog, const char *name) { return _glGetUniformLocation(prog, (const GLchar*)name); }
__declspec(dllexport) void   gl_UniformMatrix4fv(int loc, int count, unsigned char transpose, const float *value) { _glUniformMatrix4fv(loc, count, transpose, value); }
__declspec(dllexport) void   gl_Uniform1i(int loc, int v0)                       { _glUniform1i(loc, v0); }
__declspec(dllexport) void   gl_Uniform1f(int loc, float v0)                     { _glUniform1f(loc, v0); }
__declspec(dllexport) void   gl_Uniform3f(int loc, float v0, float v1, float v2) { _glUniform3f(loc, v0, v1, v2); }
__declspec(dllexport) void   gl_Uniform4f(int loc, float v0, float v1, float v2, float v3) { _glUniform4f(loc, v0, v1, v2, v3); }
__declspec(dllexport) void   gl_ActiveTexture(unsigned int texture)              { _glActiveTexture(texture); }
__declspec(dllexport) void   gl_GenerateMipmap(unsigned int target)              { _glGenerateMipmap(target); }
__declspec(dllexport) void   gl_DeleteShader(unsigned int shader)                { _glDeleteShader(shader); }
__declspec(dllexport) void   gl_DeleteProgram(unsigned int program)              { _glDeleteProgram(program); }
```

---

## 2. q2_gl_extern.axm -- All Extern Declarations

```axiom
@module q2_gl_extern;
@intent("Extern declarations for Win32, OpenGL 1.1, GL 3.3 loader, and WGL");

// =============================================================================
// Win32 Kernel
// =============================================================================

@link("kernel32")
extern "stdcall" fn GetModuleHandleW(lpModuleName: ptr[i8]) -> ptr[i8];

@link("kernel32")
extern "stdcall" fn GetLastError() -> i32;

@link("kernel32")
extern "stdcall" fn LoadLibraryA(lpFileName: ptr[i8]) -> ptr[i8];

@link("kernel32")
extern "stdcall" fn GetProcAddress(hModule: ptr[i8], lpProcName: ptr[i8]) -> ptr[i8];

@link("kernel32")
extern "stdcall" fn QueryPerformanceCounter(lpPerformanceCount: ptr[i64]) -> i32;

@link("kernel32")
extern "stdcall" fn QueryPerformanceFrequency(lpFrequency: ptr[i64]) -> i32;

@link("kernel32")
extern "stdcall" fn Sleep(dwMilliseconds: i32);

// =============================================================================
// Win32 User
// =============================================================================

@link("user32")
extern "stdcall" fn RegisterClassExW(lpWndClass: ptr[i8]) -> i32;

@link("user32")
extern "stdcall" fn CreateWindowExW(
    dwExStyle: i32, lpClassName: ptr[i8], lpWindowName: ptr[i8],
    dwStyle: i32, x: i32, y: i32, nWidth: i32, nHeight: i32,
    hWndParent: ptr[i8], hMenu: ptr[i8], hInstance: ptr[i8], lpParam: ptr[i8]
) -> ptr[i8];

@link("user32")
extern "stdcall" fn DestroyWindow(hWnd: ptr[i8]) -> i32;

@link("user32")
extern "stdcall" fn ShowWindow(hWnd: ptr[i8], nCmdShow: i32) -> i32;

@link("user32")
extern "stdcall" fn UpdateWindow(hWnd: ptr[i8]) -> i32;

@link("user32")
extern "stdcall" fn PeekMessageW(
    lpMsg: ptr[i8], hWnd: ptr[i8], wMsgFilterMin: i32, wMsgFilterMax: i32,
    wRemoveMsg: i32
) -> i32;

@link("user32")
extern "stdcall" fn TranslateMessage(lpMsg: ptr[i8]) -> i32;

@link("user32")
extern "stdcall" fn DispatchMessageW(lpMsg: ptr[i8]) -> i64;

@link("user32")
extern "stdcall" fn DefWindowProcW(
    hWnd: ptr[i8], msg: i32, wParam: i64, lParam: i64
) -> i64;

@link("user32")
extern "stdcall" fn PostQuitMessage(nExitCode: i32);

@link("user32")
extern "stdcall" fn GetDC(hWnd: ptr[i8]) -> ptr[i8];

@link("user32")
extern "stdcall" fn ReleaseDC(hWnd: ptr[i8], hDC: ptr[i8]) -> i32;

@link("user32")
extern "stdcall" fn SetWindowTextW(hWnd: ptr[i8], lpString: ptr[i8]) -> i32;

@link("user32")
extern "stdcall" fn AdjustWindowRectEx(
    lpRect: ptr[i32], dwStyle: i32, bMenu: i32, dwExStyle: i32
) -> i32;

@link("user32")
extern "stdcall" fn GetClientRect(hWnd: ptr[i8], lpRect: ptr[i32]) -> i32;

@link("user32")
extern "stdcall" fn ShowCursor(bShow: i32) -> i32;

@link("user32")
extern "stdcall" fn ClipCursor(lpRect: ptr[i32]) -> i32;

@link("user32")
extern "stdcall" fn GetCursorPos(lpPoint: ptr[i32]) -> i32;

@link("user32")
extern "stdcall" fn SetCursorPos(x: i32, y: i32) -> i32;

@link("user32")
extern "stdcall" fn ClientToScreen(hWnd: ptr[i8], lpPoint: ptr[i32]) -> i32;

@link("user32")
extern "stdcall" fn GetWindowRect(hWnd: ptr[i8], lpRect: ptr[i32]) -> i32;

@link("user32")
extern "stdcall" fn RegisterRawInputDevices(
    pRawInputDevices: ptr[i8], uiNumDevices: i32, cbSize: i32
) -> i32;

@link("user32")
extern "stdcall" fn GetRawInputData(
    hRawInput: ptr[i8], uiCommand: i32, pData: ptr[i8],
    pcbSize: ptr[i32], cbSizeHeader: i32
) -> i32;

@link("user32")
extern "stdcall" fn SetProcessDPIAware() -> i32;

@link("user32")
extern "stdcall" fn LoadCursorW(hInstance: ptr[i8], lpCursorName: ptr[i8]) -> ptr[i8];

// =============================================================================
// Win32 GDI
// =============================================================================

@link("gdi32")
extern "stdcall" fn ChoosePixelFormat(hdc: ptr[i8], ppfd: ptr[i8]) -> i32;

@link("gdi32")
extern "stdcall" fn SetPixelFormat(hdc: ptr[i8], format: i32, ppfd: ptr[i8]) -> i32;

@link("gdi32")
extern "stdcall" fn SwapBuffers(hdc: ptr[i8]) -> i32;

@link("gdi32")
extern "stdcall" fn GetStockObject(fnObject: i32) -> ptr[i8];

// =============================================================================
// WGL (from opengl32.dll -- linked statically)
// =============================================================================

@link("opengl32")
extern "stdcall" fn wglCreateContext(hdc: ptr[i8]) -> ptr[i8];

@link("opengl32")
extern "stdcall" fn wglMakeCurrent(hdc: ptr[i8], hglrc: ptr[i8]) -> i32;

@link("opengl32")
extern "stdcall" fn wglDeleteContext(hglrc: ptr[i8]) -> i32;

@link("opengl32")
extern "stdcall" fn wglGetProcAddress(lpszProc: ptr[i8]) -> ptr[i8];

// =============================================================================
// OpenGL 1.1 (from opengl32.dll -- linked statically)
// =============================================================================

@link("opengl32")
extern "stdcall" fn glEnable(cap: i32);

@link("opengl32")
extern "stdcall" fn glDisable(cap: i32);

@link("opengl32")
extern "stdcall" fn glClear(mask: i32);

@link("opengl32")
extern "stdcall" fn glClearColor(red: f32, green: f32, blue: f32, alpha: f32);

@link("opengl32")
extern "stdcall" fn glViewport(x: i32, y: i32, width: i32, height: i32);

@link("opengl32")
extern "stdcall" fn glDepthFunc(func: i32);

@link("opengl32")
extern "stdcall" fn glDepthMask(flag: u8);

@link("opengl32")
extern "stdcall" fn glBlendFunc(sfactor: i32, dfactor: i32);

@link("opengl32")
extern "stdcall" fn glCullFace(mode: i32);

@link("opengl32")
extern "stdcall" fn glFrontFace(mode: i32);

@link("opengl32")
extern "stdcall" fn glDrawArrays(mode: i32, first: i32, count: i32);

@link("opengl32")
extern "stdcall" fn glDrawElements(mode: i32, count: i32, type_: i32, indices: ptr[i8]);

@link("opengl32")
extern "stdcall" fn glGenTextures(n: i32, textures: ptr[i32]);

@link("opengl32")
extern "stdcall" fn glDeleteTextures(n: i32, textures: ptr[i32]);

@link("opengl32")
extern "stdcall" fn glBindTexture(target: i32, texture: i32);

@link("opengl32")
extern "stdcall" fn glTexImage2D(
    target: i32, level: i32, internalformat: i32,
    width: i32, height: i32, border: i32,
    format: i32, type_: i32, data: ptr[i8]
);

@link("opengl32")
extern "stdcall" fn glTexParameteri(target: i32, pname: i32, param: i32);

@link("opengl32")
extern "stdcall" fn glPixelStorei(pname: i32, param: i32);

@link("opengl32")
extern "stdcall" fn glReadPixels(
    x: i32, y: i32, width: i32, height: i32,
    format: i32, type_: i32, data: ptr[i8]
);

@link("opengl32")
extern "stdcall" fn glGetString(name: i32) -> ptr[i8];

@link("opengl32")
extern "stdcall" fn glGetIntegerv(pname: i32, params: ptr[i32]);

@link("opengl32")
extern "stdcall" fn glGetError() -> i32;

@link("opengl32")
extern "stdcall" fn glPolygonMode(face: i32, mode: i32);

@link("opengl32")
extern "stdcall" fn glPolygonOffset(factor: f32, units: f32);

@link("opengl32")
extern "stdcall" fn glFlush();

@link("opengl32")
extern "stdcall" fn glFinish();

// =============================================================================
// GL 3.3 Functions (from q2_gl_loader.dll)
// These are plain C functions exported by the loader DLL.
// =============================================================================

// --- Vertex Arrays ---
@link("q2_gl_loader")
extern "C" fn gl_GenVertexArrays(n: i32, arrays: ptr[i32]);

@link("q2_gl_loader")
extern "C" fn gl_BindVertexArray(array: i32);

@link("q2_gl_loader")
extern "C" fn gl_DeleteVertexArrays(n: i32, arrays: ptr[i32]);

// --- Buffers ---
@link("q2_gl_loader")
extern "C" fn gl_GenBuffers(n: i32, buffers: ptr[i32]);

@link("q2_gl_loader")
extern "C" fn gl_BindBuffer(target: i32, buffer: i32);

@link("q2_gl_loader")
extern "C" fn gl_BufferData(target: i32, size: i64, data: ptr[i8], usage: i32);

@link("q2_gl_loader")
extern "C" fn gl_DeleteBuffers(n: i32, buffers: ptr[i32]);

// --- Vertex Attributes ---
@link("q2_gl_loader")
extern "C" fn gl_EnableVertexAttribArray(index: i32);

@link("q2_gl_loader")
extern "C" fn gl_VertexAttribPointer(
    index: i32, size: i32, type_: i32, normalized: u8,
    stride: i32, offset: i64
);

// --- Shaders ---
@link("q2_gl_loader")
extern "C" fn gl_CreateShader(type_: i32) -> i32;

@link("q2_gl_loader")
extern "C" fn gl_ShaderSource(shader: i32, count: i32, string: ptr[ptr[i8]], length: ptr[i32]);

@link("q2_gl_loader")
extern "C" fn gl_CompileShader(shader: i32);

@link("q2_gl_loader")
extern "C" fn gl_GetShaderiv(shader: i32, pname: i32, params: ptr[i32]);

@link("q2_gl_loader")
extern "C" fn gl_GetShaderInfoLog(shader: i32, maxLength: i32, length: ptr[i32], infoLog: ptr[i8]);

@link("q2_gl_loader")
extern "C" fn gl_DeleteShader(shader: i32);

// --- Programs ---
@link("q2_gl_loader")
extern "C" fn gl_CreateProgram() -> i32;

@link("q2_gl_loader")
extern "C" fn gl_AttachShader(program: i32, shader: i32);

@link("q2_gl_loader")
extern "C" fn gl_LinkProgram(program: i32);

@link("q2_gl_loader")
extern "C" fn gl_GetProgramiv(program: i32, pname: i32, params: ptr[i32]);

@link("q2_gl_loader")
extern "C" fn gl_GetProgramInfoLog(program: i32, maxLength: i32, length: ptr[i32], infoLog: ptr[i8]);

@link("q2_gl_loader")
extern "C" fn gl_UseProgram(program: i32);

@link("q2_gl_loader")
extern "C" fn gl_DeleteProgram(program: i32);

// --- Uniforms ---
@link("q2_gl_loader")
extern "C" fn gl_GetUniformLocation(program: i32, name: ptr[i8]) -> i32;

@link("q2_gl_loader")
extern "C" fn gl_UniformMatrix4fv(location: i32, count: i32, transpose: u8, value: ptr[f32]);

@link("q2_gl_loader")
extern "C" fn gl_Uniform1i(location: i32, v0: i32);

@link("q2_gl_loader")
extern "C" fn gl_Uniform1f(location: i32, v0: f32);

@link("q2_gl_loader")
extern "C" fn gl_Uniform3f(location: i32, v0: f32, v1: f32, v2: f32);

@link("q2_gl_loader")
extern "C" fn gl_Uniform4f(location: i32, v0: f32, v1: f32, v2: f32, v3: f32);

// --- Textures (GL 3.3) ---
@link("q2_gl_loader")
extern "C" fn gl_ActiveTexture(texture: i32);

@link("q2_gl_loader")
extern "C" fn gl_GenerateMipmap(target: i32);

// =============================================================================
// C Runtime (for memset/memcpy needed in struct setup)
// =============================================================================

extern "C" fn memset(dest: ptr[i8], c: i32, n: i64) -> ptr[i8];
extern "C" fn memcpy(dest: ptr[i8], src: ptr[i8], n: i64) -> ptr[i8];
```

---

## 3. q2_gl_constants.axm -- All GL and Win32 Constants

```axiom
@module q2_gl_constants;
@intent("GL and Win32 numeric constants as @pure fns for zero-cost inlining");

// =============================================================================
// OpenGL Constants
// =============================================================================

// --- Clear buffer bits ---
@pure fn GL_COLOR_BUFFER_BIT() -> i32 { return 16384; }     // 0x4000
@pure fn GL_DEPTH_BUFFER_BIT() -> i32 { return 256; }       // 0x0100
@pure fn GL_STENCIL_BUFFER_BIT() -> i32 { return 1024; }    // 0x0400

// --- Primitive types ---
@pure fn GL_POINTS() -> i32 { return 0; }
@pure fn GL_LINES() -> i32 { return 1; }
@pure fn GL_LINE_STRIP() -> i32 { return 3; }
@pure fn GL_TRIANGLES() -> i32 { return 4; }
@pure fn GL_TRIANGLE_STRIP() -> i32 { return 5; }
@pure fn GL_TRIANGLE_FAN() -> i32 { return 6; }

// --- Data types ---
@pure fn GL_BYTE() -> i32 { return 5120; }                  // 0x1400
@pure fn GL_UNSIGNED_BYTE() -> i32 { return 5121; }         // 0x1401
@pure fn GL_SHORT() -> i32 { return 5122; }                 // 0x1402
@pure fn GL_UNSIGNED_SHORT() -> i32 { return 5123; }        // 0x1403
@pure fn GL_INT() -> i32 { return 5124; }                   // 0x1404
@pure fn GL_UNSIGNED_INT() -> i32 { return 5125; }          // 0x1405
@pure fn GL_FLOAT() -> i32 { return 5126; }                 // 0x1406

// --- Enable/disable caps ---
@pure fn GL_DEPTH_TEST() -> i32 { return 2929; }            // 0x0B71
@pure fn GL_CULL_FACE_CAP() -> i32 { return 2884; }         // 0x0B44
@pure fn GL_BLEND() -> i32 { return 3042; }                 // 0x0BE2
@pure fn GL_TEXTURE_2D() -> i32 { return 3553; }            // 0x0DE1
@pure fn GL_POLYGON_OFFSET_FILL() -> i32 { return 32823; }  // 0x8037

// --- Depth function ---
@pure fn GL_LESS() -> i32 { return 513; }                   // 0x0201
@pure fn GL_LEQUAL() -> i32 { return 515; }                 // 0x0203

// --- Blend factors ---
@pure fn GL_SRC_ALPHA() -> i32 { return 770; }              // 0x0302
@pure fn GL_ONE_MINUS_SRC_ALPHA() -> i32 { return 771; }    // 0x0303
@pure fn GL_ONE() -> i32 { return 1; }

// --- Cull face ---
@pure fn GL_FRONT() -> i32 { return 1028; }                 // 0x0404
@pure fn GL_BACK() -> i32 { return 1029; }                  // 0x0405
@pure fn GL_CW() -> i32 { return 2304; }                    // 0x0900
@pure fn GL_CCW() -> i32 { return 2305; }                   // 0x0901

// --- Texture parameters ---
@pure fn GL_TEXTURE_MIN_FILTER() -> i32 { return 10241; }   // 0x2801
@pure fn GL_TEXTURE_MAG_FILTER() -> i32 { return 10240; }   // 0x2800
@pure fn GL_TEXTURE_WRAP_S() -> i32 { return 10242; }       // 0x2802
@pure fn GL_TEXTURE_WRAP_T() -> i32 { return 10243; }       // 0x2803
@pure fn GL_NEAREST() -> i32 { return 9728; }               // 0x2600
@pure fn GL_LINEAR() -> i32 { return 9729; }                // 0x2601
@pure fn GL_NEAREST_MIPMAP_LINEAR() -> i32 { return 9986; } // 0x2702
@pure fn GL_LINEAR_MIPMAP_LINEAR() -> i32 { return 9987; }  // 0x2703
@pure fn GL_REPEAT() -> i32 { return 10497; }               // 0x2901
@pure fn GL_CLAMP_TO_EDGE() -> i32 { return 33071; }        // 0x812F

// --- Texture formats ---
@pure fn GL_RGB() -> i32 { return 6407; }                   // 0x1907
@pure fn GL_RGBA() -> i32 { return 6408; }                  // 0x1908
@pure fn GL_LUMINANCE() -> i32 { return 6409; }             // 0x1909
@pure fn GL_RGB8() -> i32 { return 32849; }                 // 0x8051
@pure fn GL_RGBA8() -> i32 { return 32856; }                // 0x8058

// --- Buffer targets ---
@pure fn GL_ARRAY_BUFFER() -> i32 { return 34962; }         // 0x8892
@pure fn GL_ELEMENT_ARRAY_BUFFER() -> i32 { return 34963; } // 0x8893

// --- Buffer usage ---
@pure fn GL_STATIC_DRAW() -> i32 { return 35044; }          // 0x88E4
@pure fn GL_DYNAMIC_DRAW() -> i32 { return 35048; }         // 0x88E8

// --- Shader types ---
@pure fn GL_VERTEX_SHADER() -> i32 { return 35633; }        // 0x8B31
@pure fn GL_FRAGMENT_SHADER() -> i32 { return 35632; }      // 0x8B30

// --- Shader query ---
@pure fn GL_COMPILE_STATUS() -> i32 { return 35713; }       // 0x8B81
@pure fn GL_LINK_STATUS() -> i32 { return 35714; }          // 0x8B82
@pure fn GL_INFO_LOG_LENGTH() -> i32 { return 35716; }      // 0x8B84

// --- Texture units ---
@pure fn GL_TEXTURE0() -> i32 { return 33984; }             // 0x84C0
@pure fn GL_TEXTURE1() -> i32 { return 33985; }             // 0x84C1
@pure fn GL_TEXTURE2() -> i32 { return 33986; }             // 0x84C2

// --- Get queries ---
@pure fn GL_VENDOR() -> i32 { return 7936; }                // 0x1F00
@pure fn GL_RENDERER() -> i32 { return 7937; }              // 0x1F01
@pure fn GL_VERSION() -> i32 { return 7938; }               // 0x1F02

// --- Pixel store ---
@pure fn GL_UNPACK_ALIGNMENT() -> i32 { return 3317; }      // 0x0CF5

// --- Polygon mode ---
@pure fn GL_LINE() -> i32 { return 6913; }                  // 0x1B01
@pure fn GL_FILL() -> i32 { return 6914; }                  // 0x1B02
@pure fn GL_FRONT_AND_BACK() -> i32 { return 1032; }        // 0x0408

// --- Boolean ---
@pure fn GL_TRUE() -> u8 { return 1; }
@pure fn GL_FALSE() -> u8 { return 0; }

// =============================================================================
// Win32 Constants
// =============================================================================

// --- Window styles ---
@pure fn WS_OVERLAPPEDWINDOW() -> i32 { return 13565952; }  // 0x00CF0000
@pure fn WS_VISIBLE() -> i32 { return 268435456; }          // 0x10000000
@pure fn WS_CLIPCHILDREN() -> i32 { return 33554432; }      // 0x02000000
@pure fn WS_CLIPSIBLINGS() -> i32 { return 67108864; }      // 0x04000000

// --- Extended window styles ---
@pure fn WS_EX_APPWINDOW() -> i32 { return 262144; }        // 0x00040000

// --- ShowWindow constants ---
@pure fn SW_SHOW() -> i32 { return 5; }

// --- Window messages ---
@pure fn WM_DESTROY() -> i32 { return 2; }
@pure fn WM_SIZE() -> i32 { return 5; }
@pure fn WM_CLOSE() -> i32 { return 16; }                   // 0x0010
@pure fn WM_QUIT() -> i32 { return 18; }                    // 0x0012
@pure fn WM_KEYDOWN() -> i32 { return 256; }                // 0x0100
@pure fn WM_KEYUP() -> i32 { return 257; }                  // 0x0101
@pure fn WM_MOUSEMOVE() -> i32 { return 512; }              // 0x0200
@pure fn WM_LBUTTONDOWN() -> i32 { return 513; }            // 0x0201
@pure fn WM_LBUTTONUP() -> i32 { return 514; }              // 0x0202
@pure fn WM_RBUTTONDOWN() -> i32 { return 516; }            // 0x0204
@pure fn WM_RBUTTONUP() -> i32 { return 517; }              // 0x0205
@pure fn WM_MBUTTONDOWN() -> i32 { return 519; }            // 0x0207
@pure fn WM_MBUTTONUP() -> i32 { return 520; }              // 0x0208
@pure fn WM_INPUT() -> i32 { return 255; }                  // 0x00FF

// --- Virtual key codes ---
@pure fn VK_ESCAPE() -> i32 { return 27; }
@pure fn VK_SPACE() -> i32 { return 32; }
@pure fn VK_SHIFT() -> i32 { return 16; }
@pure fn VK_CONTROL() -> i32 { return 17; }
@pure fn VK_W() -> i32 { return 87; }
@pure fn VK_A() -> i32 { return 65; }
@pure fn VK_S() -> i32 { return 83; }
@pure fn VK_D() -> i32 { return 68; }
@pure fn VK_E() -> i32 { return 69; }
@pure fn VK_Q() -> i32 { return 81; }

// --- PeekMessage flags ---
@pure fn PM_REMOVE() -> i32 { return 1; }
@pure fn PM_NOREMOVE() -> i32 { return 0; }

// --- CreateWindowEx positioning ---
@pure fn CW_USEDEFAULT() -> i32 { return -2147483648; }     // 0x80000000 (i32 min)

// --- Pixel format descriptor flags ---
@pure fn PFD_DRAW_TO_WINDOW() -> i32 { return 4; }
@pure fn PFD_SUPPORT_OPENGL() -> i32 { return 32; }
@pure fn PFD_DOUBLEBUFFER() -> i32 { return 1; }
@pure fn PFD_TYPE_RGBA() -> u8 { return 0; }
@pure fn PFD_MAIN_PLANE() -> u8 { return 0; }

// --- Raw input constants ---
@pure fn RID_INPUT() -> i32 { return 268435459; }           // 0x10000003
@pure fn RIM_TYPEMOUSE() -> i32 { return 0; }
@pure fn MOUSE_MOVE_RELATIVE() -> i32 { return 0; }
@pure fn RIDEV_INPUTSINK() -> i32 { return 256; }           // 0x100
@pure fn HID_USAGE_PAGE_GENERIC() -> i32 { return 1; }
@pure fn HID_USAGE_GENERIC_MOUSE() -> i32 { return 2; }

// --- Cursor ---
@pure fn IDC_ARROW() -> i64 { return 32512; }

// --- GetStockObject ---
@pure fn BLACK_BRUSH() -> i32 { return 4; }

// --- PixelFormatDescriptor size ---
@pure fn SIZEOF_PFD() -> i32 { return 40; }

// --- WNDCLASSEXW size ---
@pure fn SIZEOF_WNDCLASSEXW() -> i32 { return 80; }

// --- MSG size ---
@pure fn SIZEOF_MSG() -> i32 { return 48; }

// --- RAWINPUTDEVICE size ---
@pure fn SIZEOF_RAWINPUTDEVICE() -> i32 { return 16; }

// --- RAWINPUTHEADER size ---
@pure fn SIZEOF_RAWINPUTHEADER() -> i32 { return 24; }

// --- RAWINPUT size (header + mouse union) ---
@pure fn SIZEOF_RAWINPUT() -> i32 { return 48; }
```

---

## 4. q2_window.axm -- Win32 Window + OpenGL Context

### Struct Layouts (byte offsets for manual ptr_write)

#### WNDCLASSEXW (80 bytes on x64)
```
Offset  Size  Field
  0       4   cbSize = 80
  4       4   style
  8       8   lpfnWndProc (function pointer)
 16       4   cbClsExtra
 20       4   cbWndExtra
 24       8   hInstance
 32       8   hIcon
 40       8   hCursor
 48       8   hbrBackground
 56       8   lpszMenuName
 64       8   lpszClassName
 72       8   hIconSm
```

#### PIXELFORMATDESCRIPTOR (40 bytes)
```
Offset  Size  Field
  0       2   nSize = 40
  2       2   nVersion = 1
  4       4   dwFlags (PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER)
  8       1   iPixelType (PFD_TYPE_RGBA = 0)
  9       1   cColorBits = 32
 10       1   cRedBits
 11       1   cRedShift
 12       1   cGreenBits
 13       1   cGreenShift
 14       1   cBlueBits
 15       1   cBlueShift
 16       1   cAlphaBits = 8
 17       1   cAlphaShift
 18       1   cAccumBits
 19       1   cAccumRedBits
 20       1   cAccumGreenBits
 21       1   cAccumBlueBits
 22       1   cAccumAlphaBits
 23       1   cDepthBits = 24
 24       1   cStencilBits = 8
 25       1   cAuxBuffers
 26       1   iLayerType (PFD_MAIN_PLANE = 0)
 27       1   bReserved
 28       4   dwLayerMask
 32       4   dwVisibleMask
 36       4   dwDamageMask
```

#### MSG (48 bytes on x64)
```
Offset  Size  Field
  0       8   hwnd
  8       4   message
 12       4   (padding)
 16       8   wParam
 24       8   lParam
 32       4   time
 36       4   pt.x
 40       4   pt.y
 44       4   (padding)
```

#### RAWINPUTDEVICE (16 bytes on x64)
```
Offset  Size  Field
  0       2   usUsagePage
  2       2   usUsage
  4       4   dwFlags
  8       8   hwndTarget
```
Total: 16 bytes on x64 (8-byte aligned due to pointer field).

#### RAWINPUT (header + union, ~48 bytes on x64)
```
Offset  Size  Field (RAWINPUTHEADER)
  0       4   dwSize
  4       4   dwType
  8       8   hDevice
 16       8   wParam

                Field (RAWMOUSE, at offset 24)
 24       2   usFlags
 26       2   usButtonFlags (union)
 28       2   usButtonData  (union)
 30       2   (padding)
 32       4   ulRawButtons
 36       4   lLastX
 40       4   lLastY
 44       4   ulExtraInformation
```

### Module Design

```axiom
@module q2_window;
@intent("Win32 window creation, OpenGL context, message pump");

// ---------------------------------------------------------------------------
// Global state (module-level mutable globals accessed via ptr)
// ---------------------------------------------------------------------------
// g_hwnd:        ptr[i8]  -- window handle
// g_hdc:         ptr[i8]  -- device context
// g_hglrc:       ptr[i8]  -- OpenGL rendering context
// g_should_close: i32     -- 1 when window should close
// g_width:       i32      -- client width
// g_height:      i32      -- client height
//
// Stored as a single ptr[i64] "window state block" (128 bytes):
//   [0]  = g_hwnd        (as i64, cast from ptr)
//   [1]  = g_hdc         (as i64, cast from ptr)
//   [2]  = g_hglrc       (as i64, cast from ptr)
//   [3]  = g_should_close (i64, 0 or 1)
//   [4]  = g_width       (i64)
//   [5]  = g_height      (i64)

// ---------------------------------------------------------------------------
// WndProc callback
// ---------------------------------------------------------------------------
// CRITICAL: Must be @export so it gets C ABI (not fastcc).
// Win32 calls this with stdcall convention on x86, but on x64 there is
// only one calling convention (Microsoft x64), so @export (= dso_local)
// with default CC is correct.
//
// The function pointer is obtained via fn_ptr(q2_wndproc) and written
// into the WNDCLASSEXW at byte offset 8.

@export
fn q2_wndproc(hwnd: ptr[i8], msg: i32, wparam: i64, lparam: i64) -> i64 {
    // Dispatch based on msg:
    // WM_CLOSE (16), WM_DESTROY (2): set g_should_close = 1, return 0
    // WM_KEYDOWN (256):   set key_state[wparam & 0xFF] = 1
    // WM_KEYUP (257):     set key_state[wparam & 0xFF] = 0
    // WM_INPUT (255):     extract raw mouse deltas
    // WM_SIZE (5):        update g_width, g_height
    // default:            return DefWindowProcW(hwnd, msg, wparam, lparam)
    //
    // Access global state via a module-level ptr stored in a known location.
    // (See q2_input.axm for key_state and mouse_delta access pattern.)

    if msg == WM_CLOSE() or msg == WM_DESTROY() {
        // Mark window for closure
        // Write 1 to the global should_close flag
        // (Implementation: write to a known global ptr)
        return 0;
    }

    return DefWindowProcW(hwnd, msg, wparam, lparam);
}

// ---------------------------------------------------------------------------
// Window Creation Flow
// ---------------------------------------------------------------------------
//
// fn q2_window_create(width: i32, height: i32, title: ptr[i8]) -> ptr[i64]
//
// Steps:
//   1. SetProcessDPIAware()
//   2. GetModuleHandleW(0) -> hinstance
//   3. Allocate WNDCLASSEXW (80 bytes) via heap_alloc_zeroed
//   4. Fill WNDCLASSEXW:
//      - ptr_write_i32(wc_buf, 0, 80)           // cbSize at offset 0
//      - ptr_write_i32(wc_buf, 1, 35)           // style = CS_OWNDC|CS_HREDRAW|CS_VREDRAW at offset 4
//      - ptr_write_i64(wc_ptr, 1, fn_ptr(q2_wndproc))  // lpfnWndProc at offset 8
//      - ptr_write_i64(wc_ptr, 3, hinstance)    // hInstance at offset 24
//      - ptr_write_i64(wc_ptr, 5, LoadCursorW(0, IDC_ARROW()))  // hCursor at offset 40
//      - ptr_write_i64(wc_ptr, 6, GetStockObject(BLACK_BRUSH())) // hbrBackground at offset 48
//      - ptr_write_i64(wc_ptr, 8, class_name_ptr) // lpszClassName at offset 64
//   5. RegisterClassExW(wc_buf)
//   6. Compute adjusted window rect via AdjustWindowRectEx
//   7. CreateWindowExW(...)
//   8. GetDC(hwnd) -> hdc
//   9. Set up PIXELFORMATDESCRIPTOR (40 bytes):
//      - ptr_write_u8 for nSize(2 bytes at 0), nVersion(2 bytes at 2)
//      - ptr_write_i32 for dwFlags at offset 4
//      - ptr_write_u8 for cColorBits(32), cDepthBits(24), cStencilBits(8), cAlphaBits(8)
//  10. ChoosePixelFormat(hdc, pfd) -> format
//  11. SetPixelFormat(hdc, format, pfd)
//  12. wglCreateContext(hdc) -> hglrc
//  13. wglMakeCurrent(hdc, hglrc)
//  14. gl_LoadFunctions()  -- load GL 3.3 function pointers
//  15. ShowWindow(hwnd, SW_SHOW)
//  16. UpdateWindow(hwnd)
//  17. Return state block ptr

// ---------------------------------------------------------------------------
// Message Pump
// ---------------------------------------------------------------------------
//
// fn q2_window_pump_messages(state: ptr[i64])
//
// Steps:
//   1. Allocate MSG struct (48 bytes) via heap_alloc_zeroed (or reuse)
//   2. while PeekMessageW(msg, 0, 0, 0, PM_REMOVE) != 0 {
//        // Read msg.message at offset 8 (i32)
//        let message: i32 = ptr_read_i32(msg_as_i32, 2);
//        if message == WM_QUIT() { set should_close = 1; break; }
//        TranslateMessage(msg);
//        DispatchMessageW(msg);
//      }
//
// Note: WndProc handles WM_KEYDOWN/WM_KEYUP/WM_INPUT during DispatchMessageW.

// ---------------------------------------------------------------------------
// Shutdown
// ---------------------------------------------------------------------------
//
// fn q2_window_destroy(state: ptr[i64])
//
// Steps:
//   1. wglMakeCurrent(0, 0)
//   2. wglDeleteContext(hglrc)
//   3. ReleaseDC(hwnd, hdc)
//   4. DestroyWindow(hwnd)
//   5. heap_free(state)
```

### WndProc Callback Mechanism (Critical Detail)

The key challenge: Win32's `RegisterClassExW` expects a function pointer to `WndProc`. In AXIOM:

1. Declare the WndProc function with `@export` so it gets C ABI (not `fastcc`):
   ```axiom
   @export
   fn q2_wndproc(hwnd: ptr[i8], msg: i32, wparam: i64, lparam: i64) -> i64 { ... }
   ```

2. On x64 Windows, there is only one calling convention (Microsoft x64 ABI). The `@export` annotation causes AXIOM to emit `define dso_local i64 @q2_wndproc(ptr, i32, i64, i64)` which matches what Win32 expects for WNDPROC on x64.

3. Get the function pointer via `fn_ptr(q2_wndproc)` and write it into the WNDCLASSEXW struct at byte offset 8:
   ```axiom
   let wndproc_ptr: ptr[i8] = fn_ptr(q2_wndproc);
   // Write to WNDCLASSEXW byte offset 8 (lpfnWndProc)
   // Since WNDCLASSEXW is accessed as ptr[i64], offset 8 bytes = index 1
   ptr_write_i64(wc_as_i64, 1, wndproc_ptr);
   ```

4. The WndProc accesses global state (key array, should_close flag) through a module-level pointer. Since AXIOM does not have mutable globals, we use a heap-allocated state block that is initialized at startup and accessed by pointer from both the main loop and the WndProc.

**Global state sharing pattern:**
```axiom
// At startup, allocate a shared state block
let g_state: ptr[i64] = heap_alloc_zeroed(64, 8);
// Store ptr in a well-known global (see implementation note below)

// In WndProc, read the global state ptr to access key_state, should_close, etc.
```

**Implementation note on global mutable state:** AXIOM does not have global mutable variables. The WndProc needs access to shared state (key array, should_close flag). Solution: store the state pointer in a C-side global variable exported by q2_gl_loader.c:

Add to q2_gl_loader.c:
```c
__declspec(dllexport) void* g_axiom_state = NULL;

__declspec(dllexport) void  gl_SetState(void* state) { g_axiom_state = state; }
__declspec(dllexport) void* gl_GetState(void) { return g_axiom_state; }
```

Add to q2_gl_extern.axm:
```axiom
@link("q2_gl_loader")
extern "C" fn gl_SetState(state: ptr[i8]);

@link("q2_gl_loader")
extern "C" fn gl_GetState() -> ptr[i8];
```

Then in WndProc:
```axiom
@export
fn q2_wndproc(hwnd: ptr[i8], msg: i32, wparam: i64, lparam: i64) -> i64 {
    let state: ptr[i8] = gl_GetState();
    // ... access key_state, should_close via state pointer ...
}
```

---

## 5. q2_shader.axm -- GLSL Shader Compilation

```axiom
@module q2_shader;
@intent("GLSL shader compilation, program linking, uniform location caching in pure AXIOM");

// =============================================================================
// GLSL Source Strings
// =============================================================================
// String literals in AXIOM are ptr[i8] (null-terminated C strings).
// Multi-line GLSL is embedded as concatenated single-line constants.
// AXIOM string literals support \n for newlines.

// --- Lightmapped BSP Vertex Shader ---
// Inputs: position (vec3), texcoord (vec2), lightmap_uv (vec2)
// Uniforms: u_view_proj (mat4)
// Outputs: v_texcoord, v_lm_uv

@pure fn BSP_VERT_SRC() -> ptr[i8] {
    return "#version 330 core\nlayout(location=0) in vec3 a_pos;\nlayout(location=1) in vec2 a_uv;\nlayout(location=2) in vec2 a_lm_uv;\nuniform mat4 u_view_proj;\nout vec2 v_uv;\nout vec2 v_lm_uv;\nvoid main() {\n  gl_Position = u_view_proj * vec4(a_pos, 1.0);\n  v_uv = a_uv;\n  v_lm_uv = a_lm_uv;\n}\n";
}

// --- Lightmapped BSP Fragment Shader ---
// Inputs: v_texcoord, v_lm_uv
// Uniforms: u_texture (sampler2D, unit 0), u_lightmap (sampler2D, unit 1)
// Output: frag_color = texture_color * lightmap_color * 2.0

@pure fn BSP_FRAG_SRC() -> ptr[i8] {
    return "#version 330 core\nin vec2 v_uv;\nin vec2 v_lm_uv;\nuniform sampler2D u_texture;\nuniform sampler2D u_lightmap;\nout vec4 frag_color;\nvoid main() {\n  vec4 tex = texture(u_texture, v_uv);\n  vec3 lm = texture(u_lightmap, v_lm_uv).rgb;\n  frag_color = vec4(tex.rgb * lm * 2.0, tex.a);\n}\n";
}

// --- Colored mesh Vertex Shader (for debug, sky, etc.) ---

@pure fn COLOR_VERT_SRC() -> ptr[i8] {
    return "#version 330 core\nlayout(location=0) in vec3 a_pos;\nlayout(location=1) in vec4 a_color;\nuniform mat4 u_view_proj;\nout vec4 v_color;\nvoid main() {\n  gl_Position = u_view_proj * vec4(a_pos, 1.0);\n  v_color = a_color;\n}\n";
}

@pure fn COLOR_FRAG_SRC() -> ptr[i8] {
    return "#version 330 core\nin vec4 v_color;\nout vec4 frag_color;\nvoid main() {\n  frag_color = v_color;\n}\n";
}

// =============================================================================
// Shader Compilation
// =============================================================================
//
// fn compile_shader(shader_type: i32, source: ptr[i8]) -> i32
//
// Steps:
//   1. let shader: i32 = gl_CreateShader(shader_type);
//   2. Prepare source pointer array:
//      let src_ptrs: ptr[ptr[i8]] = heap_alloc(1, 8);
//      ptr_write_i64(src_ptrs, 0, source);  // write ptr as i64
//   3. gl_ShaderSource(shader, 1, src_ptrs, 0);  // 0 = null (use null-terminated)
//   4. gl_CompileShader(shader);
//   5. Check compilation:
//      let status: ptr[i32] = heap_alloc(1, 4);
//      gl_GetShaderiv(shader, GL_COMPILE_STATUS(), status);
//      if ptr_read_i32(status, 0) == 0 {
//          // Compilation failed -- get error log
//          let log_len: ptr[i32] = heap_alloc(1, 4);
//          gl_GetShaderiv(shader, GL_INFO_LOG_LENGTH(), log_len);
//          let log_buf: ptr[i8] = heap_alloc(ptr_read_i32(log_len, 0), 1);
//          gl_GetShaderInfoLog(shader, ptr_read_i32(log_len, 0), log_len, log_buf);
//          print("Shader compile error:");
//          // print the log_buf...
//          gl_DeleteShader(shader);
//          return 0;  // 0 = invalid shader
//      }
//   6. return shader;

// =============================================================================
// Program Linking
// =============================================================================
//
// fn link_program(vert_shader: i32, frag_shader: i32) -> i32
//
// Steps:
//   1. let program: i32 = gl_CreateProgram();
//   2. gl_AttachShader(program, vert_shader);
//   3. gl_AttachShader(program, frag_shader);
//   4. gl_LinkProgram(program);
//   5. Check link status (same pattern as compile check with GL_LINK_STATUS)
//   6. gl_DeleteShader(vert_shader);  -- no longer needed after linking
//   7. gl_DeleteShader(frag_shader);
//   8. return program;

// =============================================================================
// Uniform Location Cache
// =============================================================================
//
// For each shader program, cache uniform locations at creation time.
// Store in a SOA layout:
//
// Shader program state block (per program, ptr[i32]):
//   [0] = program_id
//   [1] = u_view_proj location
//   [2] = u_texture location
//   [3] = u_lightmap location
//   [4] = u_color location (for colored shader)
//
// fn cache_bsp_uniforms(program: i32) -> ptr[i32]
//   Allocate 8 i32s, fill with gl_GetUniformLocation results.
//
// fn cache_color_uniforms(program: i32) -> ptr[i32]
//   Same pattern for color shader.

// =============================================================================
// Convenience: Create complete shader programs
// =============================================================================
//
// fn create_bsp_program() -> ptr[i32]
//   1. compile_shader(GL_VERTEX_SHADER(), BSP_VERT_SRC())
//   2. compile_shader(GL_FRAGMENT_SHADER(), BSP_FRAG_SRC())
//   3. link_program(vs, fs)
//   4. cache_bsp_uniforms(program)
//   5. Return uniform cache ptr (program_id stored at index 0)
//
// fn create_color_program() -> ptr[i32]
//   Same for color shader.
```

---

## 6. q2_gpu.axm -- Texture Upload, Mesh, Draw Calls, Camera

```axiom
@module q2_gpu;
@intent("OpenGL mesh/texture/draw operations and camera matrix in pure AXIOM");

// =============================================================================
// Texture Upload
// =============================================================================
//
// fn upload_texture(rgba_data: ptr[i8], width: i32, height: i32) -> i32
//
// Steps:
//   1. let tex_id: ptr[i32] = heap_alloc(1, 4);
//   2. glGenTextures(1, tex_id);
//   3. let id: i32 = ptr_read_i32(tex_id, 0);
//   4. glBindTexture(GL_TEXTURE_2D(), id);
//   5. glTexParameteri(GL_TEXTURE_2D(), GL_TEXTURE_MIN_FILTER(), GL_LINEAR_MIPMAP_LINEAR());
//   6. glTexParameteri(GL_TEXTURE_2D(), GL_TEXTURE_MAG_FILTER(), GL_LINEAR());
//   7. glTexParameteri(GL_TEXTURE_2D(), GL_TEXTURE_WRAP_S(), GL_REPEAT());
//   8. glTexParameteri(GL_TEXTURE_2D(), GL_TEXTURE_WRAP_T(), GL_REPEAT());
//   9. glPixelStorei(GL_UNPACK_ALIGNMENT(), 1);
//  10. glTexImage2D(GL_TEXTURE_2D(), 0, GL_RGBA8(), width, height, 0,
//                   GL_RGBA(), GL_UNSIGNED_BYTE(), rgba_data);
//  11. gl_GenerateMipmap(GL_TEXTURE_2D());
//  12. glBindTexture(GL_TEXTURE_2D(), 0);
//  13. heap_free(tex_id);
//  14. return id;

// =============================================================================
// Mesh Types
// =============================================================================
//
// Mesh state block (SOA in ptr[i32], 16 i32 fields):
//   [0] = vao_id
//   [1] = vbo_positions    (GL buffer for vec3 positions)
//   [2] = vbo_uvs          (GL buffer for vec2 tex coords)
//   [3] = vbo_lm_uvs       (GL buffer for vec2 lightmap UVs)
//   [4] = vbo_colors        (GL buffer for vec4 colors, used for colored meshes)
//   [5] = vertex_count
//   [6] = texture_id        (diffuse texture)
//   [7] = lightmap_id       (lightmap texture)
//   [8] = mesh_type         (0 = lightmapped, 1 = colored)
//
// All data is f32 on the GPU side. AXIOM computes in f64, converts to f32 before upload.

// =============================================================================
// create_lightmapped_mesh
// =============================================================================
//
// fn create_lightmapped_mesh(
//     positions: ptr[f32],     // 3 floats per vertex (x,y,z)
//     uvs: ptr[f32],           // 2 floats per vertex (u,v)
//     lm_uvs: ptr[f32],        // 2 floats per vertex (lm_u, lm_v)
//     vertex_count: i32,
//     tex_id: i32,
//     lm_tex_id: i32
// ) -> ptr[i32]
//
// Steps:
//   1. Allocate mesh state block (16 i32s)
//   2. Create VAO:
//      let vao_buf: ptr[i32] = heap_alloc(1, 4);
//      gl_GenVertexArrays(1, vao_buf);
//      let vao: i32 = ptr_read_i32(vao_buf, 0);
//      gl_BindVertexArray(vao);
//   3. Create VBO for positions:
//      let vbo_buf: ptr[i32] = heap_alloc(1, 4);
//      gl_GenBuffers(1, vbo_buf);
//      let vbo_pos: i32 = ptr_read_i32(vbo_buf, 0);
//      gl_BindBuffer(GL_ARRAY_BUFFER(), vbo_pos);
//      gl_BufferData(GL_ARRAY_BUFFER(), widen(vertex_count * 3 * 4), positions, GL_STATIC_DRAW());
//      gl_EnableVertexAttribArray(0);
//      gl_VertexAttribPointer(0, 3, GL_FLOAT(), GL_FALSE(), 12, 0);
//            // index=0, size=3, type=FLOAT, normalized=false, stride=12 bytes, offset=0
//   4. Create VBO for tex coords:
//      gl_GenBuffers(1, vbo_buf);
//      let vbo_uv: i32 = ptr_read_i32(vbo_buf, 0);
//      gl_BindBuffer(GL_ARRAY_BUFFER(), vbo_uv);
//      gl_BufferData(GL_ARRAY_BUFFER(), widen(vertex_count * 2 * 4), uvs, GL_STATIC_DRAW());
//      gl_EnableVertexAttribArray(1);
//      gl_VertexAttribPointer(1, 2, GL_FLOAT(), GL_FALSE(), 8, 0);
//   5. Create VBO for lightmap UVs:
//      gl_GenBuffers(1, vbo_buf);
//      let vbo_lm: i32 = ptr_read_i32(vbo_buf, 0);
//      gl_BindBuffer(GL_ARRAY_BUFFER(), vbo_lm);
//      gl_BufferData(GL_ARRAY_BUFFER(), widen(vertex_count * 2 * 4), lm_uvs, GL_STATIC_DRAW());
//      gl_EnableVertexAttribArray(2);
//      gl_VertexAttribPointer(2, 2, GL_FLOAT(), GL_FALSE(), 8, 0);
//   6. Unbind: gl_BindVertexArray(0);
//   7. Store all IDs in mesh state block
//   8. return mesh state block

// =============================================================================
// create_colored_mesh
// =============================================================================
//
// fn create_colored_mesh(
//     positions: ptr[f32],     // 3 floats per vertex
//     colors: ptr[f32],        // 4 floats per vertex (r,g,b,a)
//     vertex_count: i32
// ) -> ptr[i32]
//
// Same pattern as lightmapped but with 2 VBOs (positions, colors).
// Vertex attrib 0 = position (vec3), attrib 1 = color (vec4).

// =============================================================================
// draw_mesh
// =============================================================================
//
// fn draw_mesh(mesh: ptr[i32], bsp_program: ptr[i32], color_program: ptr[i32])
//
// Steps:
//   1. let mesh_type: i32 = ptr_read_i32(mesh, 8);
//   2. if mesh_type == 0 {  // lightmapped
//        let prog_id: i32 = ptr_read_i32(bsp_program, 0);
//        gl_UseProgram(prog_id);
//        // Bind diffuse texture to unit 0
//        gl_ActiveTexture(GL_TEXTURE0());
//        glBindTexture(GL_TEXTURE_2D(), ptr_read_i32(mesh, 6));
//        gl_Uniform1i(ptr_read_i32(bsp_program, 2), 0);  // u_texture = 0
//        // Bind lightmap to unit 1
//        gl_ActiveTexture(GL_TEXTURE1());
//        glBindTexture(GL_TEXTURE_2D(), ptr_read_i32(mesh, 7));
//        gl_Uniform1i(ptr_read_i32(bsp_program, 3), 1);  // u_lightmap = 1
//      } else {  // colored
//        gl_UseProgram(ptr_read_i32(color_program, 0));
//      }
//   3. gl_BindVertexArray(ptr_read_i32(mesh, 0));  // VAO
//   4. glDrawArrays(GL_TRIANGLES(), 0, ptr_read_i32(mesh, 5));  // vertex_count
//   5. gl_BindVertexArray(0);

// =============================================================================
// set_camera -- builds view_proj matrix
// =============================================================================
//
// fn set_camera(
//     program: ptr[i32],       // shader program with cached uniforms
//     eye_x: f64, eye_y: f64, eye_z: f64,
//     target_x: f64, target_y: f64, target_z: f64,
//     fov_degrees: f64,
//     aspect: f64, near: f64, far: f64
// )
//
// Steps:
//   1. Build view matrix via look_at() from q2_math.axm (returns ptr[f64], 16 doubles)
//   2. Build projection matrix via perspective() from q2_math.axm
//   3. Multiply: view_proj = proj * view via mat4_multiply()
//   4. Convert 16 f64 values to f32 for OpenGL:
//      let mat_f32: ptr[f32] = heap_alloc(16, 4);
//      for i in range(0, 16) { ptr_write_f32(mat_f32, i, f64_to_f32(ptr_read_f64(mat_f64, i))); }
//   5. Upload: gl_UniformMatrix4fv(ptr_read_i32(program, 1), 1, GL_FALSE(), mat_f32);
//   6. heap_free temporaries

// =============================================================================
// begin_frame / end_frame
// =============================================================================
//
// fn begin_frame(r: f32, g: f32, b: f32) {
//     glClearColor(r, g, b, 1.0);
//     glClear(bor(GL_COLOR_BUFFER_BIT(), GL_DEPTH_BUFFER_BIT()));
// }
//
// fn end_frame(hdc: ptr[i8]) {
//     glFlush();
//     SwapBuffers(hdc);
// }

// =============================================================================
// GPU Init / Shutdown
// =============================================================================
//
// fn gpu_init() {
//     glEnable(GL_DEPTH_TEST());
//     glDepthFunc(GL_LEQUAL());
//     glEnable(GL_CULL_FACE_CAP());
//     glCullFace(GL_BACK());
//     glFrontFace(GL_CCW());
//     glEnable(GL_BLEND());
//     glBlendFunc(GL_SRC_ALPHA(), GL_ONE_MINUS_SRC_ALPHA());
// }
//
// fn delete_mesh(mesh: ptr[i32]) {
//     // Delete VBOs, VAO, free mesh state block
// }
//
// fn delete_texture(tex_id: i32) {
//     let id_buf: ptr[i32] = heap_alloc(1, 4);
//     ptr_write_i32(id_buf, 0, tex_id);
//     glDeleteTextures(1, id_buf);
//     heap_free(id_buf);
// }
```

---

## 7. q2_input.axm -- Keyboard, Mouse, Cursor Grab

```axiom
@module q2_input;
@intent("Win32 keyboard state, raw mouse deltas, cursor grab");

// =============================================================================
// Input State Layout
// =============================================================================
//
// Stored in a shared state block (ptr[i8], allocated in q2_window_create):
//
// Input state block (1024 bytes):
//   Bytes [0..255]:      key_state array (256 u8 values, 0 or 1)
//   Bytes [256..259]:    mouse_dx (i32, accumulated raw delta X)
//   Bytes [260..263]:    mouse_dy (i32, accumulated raw delta Y)
//   Bytes [264..267]:    mouse_button_0 (i32, left button: 0 or 1)
//   Bytes [268..271]:    mouse_button_1 (i32, right button: 0 or 1)
//   Bytes [272..275]:    mouse_button_2 (i32, middle button: 0 or 1)
//   Bytes [276..279]:    cursor_grabbed (i32, 0 or 1)
//   Bytes [280..287]:    center_x (i32), center_y (i32) -- screen center for re-centering

// =============================================================================
// Key State Access
// =============================================================================
//
// fn is_key_down(input: ptr[i8], key_code: i32) -> i32
//   return ptr_read_u8(input, band(key_code, 255));
//
// fn set_key(input: ptr[i8], key_code: i32, state: i32)
//   ptr_write_u8(input, band(key_code, 255), state);

// =============================================================================
// Mouse Delta Access
// =============================================================================
//
// fn get_mouse_dx(input: ptr[i8]) -> i32
//   // Read i32 at byte offset 256
//   return ptr_read_i32(input_as_i32_ptr, 64);  // 256 / 4 = 64
//
// fn get_mouse_dy(input: ptr[i8]) -> i32
//   return ptr_read_i32(input_as_i32_ptr, 65);  // 260 / 4 = 65
//
// fn reset_mouse_deltas(input: ptr[i8])
//   ptr_write_i32(input_as_i32_ptr, 64, 0);
//   ptr_write_i32(input_as_i32_ptr, 65, 0);
//
// fn accumulate_mouse_delta(input: ptr[i8], dx: i32, dy: i32)
//   let old_dx: i32 = ptr_read_i32(input_as_i32_ptr, 64);
//   let old_dy: i32 = ptr_read_i32(input_as_i32_ptr, 65);
//   ptr_write_i32(input_as_i32_ptr, 64, old_dx + dx);
//   ptr_write_i32(input_as_i32_ptr, 65, old_dy + dy);

// =============================================================================
// Raw Input Registration
// =============================================================================
//
// fn register_raw_mouse(hwnd: ptr[i8])
//
// Steps:
//   1. Allocate RAWINPUTDEVICE (16 bytes on x64):
//      let rid: ptr[i8] = heap_alloc_zeroed(16, 1);
//   2. Fill fields:
//      ptr_write_u8(rid, 0, HID_USAGE_PAGE_GENERIC());   // usUsagePage at offset 0 (2 bytes)
//      ptr_write_u8(rid, 1, 0);
//      ptr_write_u8(rid, 2, HID_USAGE_GENERIC_MOUSE());  // usUsage at offset 2 (2 bytes)
//      ptr_write_u8(rid, 3, 0);
//      // dwFlags at offset 4 (i32) = 0 (no special flags for foreground input)
//      // hwndTarget at offset 8 (ptr) = hwnd
//      ptr_write_i64(rid_as_i64, 1, hwnd_as_i64);
//   3. RegisterRawInputDevices(rid, 1, 16);
//   4. heap_free(rid);

// =============================================================================
// Raw Input Processing (called from WndProc on WM_INPUT)
// =============================================================================
//
// fn process_raw_input(input: ptr[i8], lparam: i64)
//
// Steps:
//   1. Allocate RAWINPUT buffer (48 bytes):
//      let raw: ptr[i8] = heap_alloc_zeroed(48, 1);
//      let size_buf: ptr[i32] = heap_alloc(1, 4);
//      ptr_write_i32(size_buf, 0, 48);
//   2. GetRawInputData(lparam_as_ptr, RID_INPUT(), raw, size_buf, SIZEOF_RAWINPUTHEADER());
//   3. Check dwType at offset 4 == RIM_TYPEMOUSE():
//      let raw_type: i32 = ptr_read_i32(raw_as_i32, 1);  // offset 4
//      if raw_type == RIM_TYPEMOUSE() {
//          // Read lLastX at byte offset 36 (i32 index 9)
//          let dx: i32 = ptr_read_i32(raw_as_i32, 9);
//          // Read lLastY at byte offset 40 (i32 index 10)
//          let dy: i32 = ptr_read_i32(raw_as_i32, 10);
//          accumulate_mouse_delta(input, dx, dy);
//      }
//   4. heap_free(raw); heap_free(size_buf);

// =============================================================================
// Cursor Grab
// =============================================================================
//
// fn grab_cursor(hwnd: ptr[i8], input: ptr[i8])
//   1. ShowCursor(0);    // hide cursor
//   2. Get window rect: let rect: ptr[i32] = heap_alloc(4, 4);
//      GetClientRect(hwnd, rect);
//      ClientToScreen(hwnd, rect);        // convert top-left
//      ClientToScreen(hwnd, rect + 8);    // convert bottom-right (via ptr_offset)
//   3. ClipCursor(rect);                  // confine cursor to window
//   4. Compute center: center_x = (rect[0] + rect[2]) / 2, center_y = (rect[1] + rect[3]) / 2
//   5. SetCursorPos(center_x, center_y);
//   6. Store center coords in input state block
//   7. Set cursor_grabbed = 1
//
// fn release_cursor(input: ptr[i8])
//   1. ClipCursor(0);     // 0 = null, unconfine
//   2. ShowCursor(1);     // show cursor
//   3. Set cursor_grabbed = 0
```

---

## 8. q2_math.axm -- Matrix Math (Pure AXIOM)

```axiom
@module q2_math;
@intent("4x4 matrix math for OpenGL: look_at, perspective, multiply. Pure AXIOM, no extern.");

// =============================================================================
// Matrix Layout
// =============================================================================
//
// Matrices are stored as 16 f64 values in column-major order (OpenGL convention):
//
//   [ m0  m4  m8   m12 ]
//   [ m1  m5  m9   m13 ]
//   [ m2  m6  m10  m14 ]
//   [ m3  m7  m11  m15 ]
//
// Stored in ptr[f64] at indices [0..15].

// =============================================================================
// mat4_identity
// =============================================================================
//
// fn mat4_identity(out: ptr[f64])
//   Zero all 16 elements, then set [0]=1, [5]=1, [10]=1, [15]=1.

// =============================================================================
// mat4_multiply
// =============================================================================
//
// fn mat4_multiply(out: ptr[f64], a: ptr[f64], b: ptr[f64])
//
// Column-major multiplication: out = a * b
//
// @pure
// fn mat4_multiply(out: ptr[f64], a: ptr[f64], b: ptr[f64]) {
//     for col: i32 in range(0, 4) {
//         for row: i32 in range(0, 4) {
//             let sum: f64 = 0.0;
//             for k: i32 in range(0, 4) {
//                 // a[row + k*4] * b[k + col*4]
//                 sum = sum + ptr_read_f64(a, row + k * 4) * ptr_read_f64(b, k + col * 4);
//             }
//             ptr_write_f64(out, row + col * 4, sum);
//         }
//     }
// }

// =============================================================================
// look_at (right-handed, OpenGL convention)
// =============================================================================
//
// fn look_at(out: ptr[f64],
//            eye_x: f64, eye_y: f64, eye_z: f64,
//            target_x: f64, target_y: f64, target_z: f64,
//            up_x: f64, up_y: f64, up_z: f64)
//
// Steps:
//   1. forward = normalize(eye - target)     // camera looks along -Z
//   2. right   = normalize(cross(up, forward))
//   3. cam_up  = cross(forward, right)
//   4. Build rotation + translation matrix (column-major):
//      out[0]  = right.x    out[4]  = right.y    out[8]  = right.z    out[12] = -dot(right, eye)
//      out[1]  = cam_up.x   out[5]  = cam_up.y   out[9]  = cam_up.z   out[13] = -dot(cam_up, eye)
//      out[2]  = forward.x  out[6]  = forward.y  out[10] = forward.z  out[14] = -dot(forward, eye)
//      out[3]  = 0           out[7]  = 0           out[11] = 0           out[15] = 1
//
// Uses vec3 SIMD for cross/dot/normalize operations.

// =============================================================================
// perspective (right-handed, depth [0,1] or [-1,1])
// =============================================================================
//
// fn perspective(out: ptr[f64], fov_rad: f64, aspect: f64, near: f64, far: f64)
//
// OpenGL standard perspective (depth [-1, 1]):
//   let f: f64 = 1.0 / tan(fov_rad / 2.0);
//   out[0]  = f / aspect
//   out[5]  = f
//   out[10] = (far + near) / (near - far)
//   out[11] = -1.0
//   out[14] = (2.0 * far * near) / (near - far)
//   All others = 0.0

// =============================================================================
// Utility
// =============================================================================
//
// fn deg_to_rad(degrees: f64) -> f64 { return degrees * 3.14159265358979 / 180.0; }
//
// fn vec3_cross_xyz(ax,ay,az, bx,by,bz) -> (rx,ry,rz)
//   Uses AXIOM vec3 cross() builtin.
//
// fn vec3_normalize_xyz(x,y,z) -> (nx,ny,nz)
//   Uses AXIOM vec3 normalize() builtin.
```

---

## 9. Integration Flow

### Initialization Sequence

```
main() {
    // 1. Create window + GL context
    let win: ptr[i64] = q2_window_create(1280, 720, "AXIOM Quake 2");

    // 2. Initialize GL state
    gpu_init();

    // 3. Compile shaders
    let bsp_prog: ptr[i32] = create_bsp_program();
    let color_prog: ptr[i32] = create_color_program();

    // 4. Load Q2 data (existing modules -- unchanged)
    //    q2_pak_open("baseq2/pak0.pak")
    //    q2_bsp_load("maps/q2dm1.bsp")
    //    Upload textures via upload_texture()
    //    Upload lightmaps via upload_texture()
    //    Build meshes via create_lightmapped_mesh()

    // 5. Input setup
    let input: ptr[i8] = heap_alloc_zeroed(1024, 1);
    gl_SetState(input);  // make accessible from WndProc
    register_raw_mouse(hwnd);
    grab_cursor(hwnd, input);

    // 6. Main loop
    while q2_window_should_close(win) == 0 {
        // Pump messages (calls WndProc which updates key_state + mouse_deltas)
        q2_window_pump_messages(win);

        // Process input -> update camera
        let dx: i32 = get_mouse_dx(input);
        let dy: i32 = get_mouse_dy(input);
        reset_mouse_deltas(input);
        q2_camera_rotate(cam, dx, dy);
        if is_key_down(input, VK_W()) == 1 { q2_camera_move_forward(cam, speed); }
        // ... etc

        // Render
        begin_frame(0.1, 0.1, 0.15);
        set_camera(bsp_prog, cam_eye, cam_target, 90.0, aspect, 0.1, 4096.0);
        // Draw all BSP face meshes
        for i in range(0, mesh_count) {
            draw_mesh(meshes[i], bsp_prog, color_prog);
        }
        end_frame(hdc);
    }

    // 7. Shutdown
    release_cursor(input);
    // Delete meshes, textures, programs
    q2_window_destroy(win);
}
```

### Render Frame Sequence

```
begin_frame(r, g, b)
  -> glClearColor(r, g, b, 1.0)
  -> glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT)

set_camera(prog, eye, target, fov, aspect, near, far)
  -> look_at() -> perspective() -> mat4_multiply() -> f64_to_f32 x 16
  -> gl_UniformMatrix4fv(u_view_proj_loc, 1, GL_FALSE, mat_f32)

for each mesh:
  draw_mesh(mesh, bsp_prog, color_prog)
    -> gl_UseProgram(program)
    -> gl_ActiveTexture(GL_TEXTURE0) + glBindTexture(tex_id)
    -> gl_ActiveTexture(GL_TEXTURE1) + glBindTexture(lm_id)
    -> gl_Uniform1i(u_texture, 0) + gl_Uniform1i(u_lightmap, 1)
    -> gl_BindVertexArray(vao)
    -> glDrawArrays(GL_TRIANGLES, 0, vertex_count)
    -> gl_BindVertexArray(0)

end_frame(hdc)
  -> glFlush()
  -> SwapBuffers(hdc)
```

### Shutdown Sequence

```
release_cursor(input)
for each mesh: delete_mesh(mesh)
for each texture: delete_texture(tex_id)
gl_DeleteProgram(bsp_prog_id)
gl_DeleteProgram(color_prog_id)
wglMakeCurrent(0, 0)
wglDeleteContext(hglrc)
ReleaseDC(hwnd, hdc)
DestroyWindow(hwnd)
```

---

## 10. Updated q2_gl_loader.c (with State Pointer)

Add these lines to the q2_gl_loader.c code above (after the function pointer block, before `gl_LoadFunctions`):

```c
/* Global state pointer for AXIOM WndProc to access input state. */
static void *g_axiom_state = NULL;

__declspec(dllexport) void  gl_SetState(void *state) { g_axiom_state = state; }
__declspec(dllexport) void *gl_GetState(void)        { return g_axiom_state; }
```

And add to q2_gl_extern.axm:
```axiom
@link("q2_gl_loader")
extern "C" fn gl_SetState(state: ptr[i8]);

@link("q2_gl_loader")
extern "C" fn gl_GetState() -> ptr[i8];
```

---

## 11. AXIOM-Specific Implementation Notes

### Note 1: No Struct Values for Win32 Structs
AXIOM structs exist but Win32 structs need specific memory layouts with padding. Use `heap_alloc_zeroed(size, 1)` to get a zero-initialized byte buffer, then `ptr_write_u8`, `ptr_write_i32`, `ptr_write_i64` at computed offsets. This is the same pattern used in the existing raytracer and JSON parser examples.

### Note 2: String Constants
AXIOM string literals (`"hello"`) compile to global constant `ptr[i8]` (null-terminated). GLSL source is stored as `@pure fn` returning string literals, which LLVM inlines to a global constant reference. No heap allocation needed.

### Note 3: Type Conversions for OpenGL
OpenGL wants f32 (float), AXIOM computes in f64 (double). Use `f64_to_f32()` builtin for conversion before uploading to GL. Similarly, `widen()` converts i32 to i64 for `gl_BufferData` size parameter.

### Note 4: Pointer Casting
AXIOM's `ptr[T]` is always LLVM `ptr` (opaque pointer). Passing `ptr[f32]` where `ptr[i8]` is expected works because LLVM uses opaque pointers. The type parameter in `ptr[T]` is only for AXIOM type checking, not code generation.

### Note 5: fn_ptr for WndProc
`fn_ptr(q2_wndproc)` returns LLVM `@q2_wndproc` as a `ptr` value. Since the function is `@export`, it uses C calling convention (not `fastcc`). On x64 Windows, this matches WNDPROC signature exactly. The pointer is written into the WNDCLASSEXW struct via `ptr_write_i64`.

### Note 6: Bit Operations for Win32 Style Flags
Use `bor()` to combine Win32 style flags:
```axiom
let style: i32 = bor(bor(WS_OVERLAPPEDWINDOW(), WS_VISIBLE()), bor(WS_CLIPCHILDREN(), WS_CLIPSIBLINGS()));
```

### Note 7: Module Imports
Each `.axm` file uses `@module` declaration. The main `quake2.axm` file imports all modules. When using `axiom compile`, all `.axm` files are passed as arguments and linked together via the AXIOM module system.

### Note 8: Arena Allocator for Per-Frame Data
For temporary per-frame data (transformed vertices, etc.), use AXIOM's `arena_create` / `arena_alloc` / `arena_reset` pattern as demonstrated in `examples/game_loop/frame_alloc_demo.axm`. This eliminates per-frame malloc/free overhead.

---

## 12. Build Commands

```bash
# Step 1: Build the GL loader DLL (one time)
# Using MSVC:
cl /LD examples/quake2/q2_gl_loader.c opengl32.lib /Fe:examples/quake2/q2_gl_loader.dll

# Using MinGW/GCC:
gcc -shared -o examples/quake2/q2_gl_loader.dll examples/quake2/q2_gl_loader.c -lopengl32

# Step 2: Build AXIOM compiler
cargo build --release

# Step 3: Compile the Quake 2 renderer
# The @link annotations in q2_gl_extern.axm cause the driver to add:
#   -lq2_gl_loader -lopengl32 -luser32 -lgdi32 -lkernel32
# The driver auto-discovers library paths from the source directory.
axiom compile examples/quake2/quake2.axm -o examples/quake2/quake2.exe
```

---

## 13. Complete GL Function List (30 functions in loader)

| # | Category | Function | C Return | C Parameters |
|---|----------|----------|----------|--------------|
| 1 | VAO | gl_GenVertexArrays | void | (int n, uint* arrays) |
| 2 | VAO | gl_BindVertexArray | void | (uint array) |
| 3 | VAO | gl_DeleteVertexArrays | void | (int n, const uint* arrays) |
| 4 | Buffer | gl_GenBuffers | void | (int n, uint* buffers) |
| 5 | Buffer | gl_BindBuffer | void | (uint target, uint buffer) |
| 6 | Buffer | gl_BufferData | void | (uint target, i64 size, const void* data, uint usage) |
| 7 | Buffer | gl_DeleteBuffers | void | (int n, const uint* buffers) |
| 8 | VAttrib | gl_EnableVertexAttribArray | void | (uint index) |
| 9 | VAttrib | gl_VertexAttribPointer | void | (uint idx, int size, uint type, u8 norm, int stride, i64 offset) |
| 10 | Shader | gl_CreateShader | uint | (uint type) |
| 11 | Shader | gl_ShaderSource | void | (uint shader, int count, const char** string, const int* length) |
| 12 | Shader | gl_CompileShader | void | (uint shader) |
| 13 | Shader | gl_GetShaderiv | void | (uint shader, uint pname, int* params) |
| 14 | Shader | gl_GetShaderInfoLog | void | (uint shader, int max, int* len, char* log) |
| 15 | Shader | gl_DeleteShader | void | (uint shader) |
| 16 | Program | gl_CreateProgram | uint | (void) |
| 17 | Program | gl_AttachShader | void | (uint program, uint shader) |
| 18 | Program | gl_LinkProgram | void | (uint program) |
| 19 | Program | gl_GetProgramiv | void | (uint program, uint pname, int* params) |
| 20 | Program | gl_GetProgramInfoLog | void | (uint program, int max, int* len, char* log) |
| 21 | Program | gl_UseProgram | void | (uint program) |
| 22 | Program | gl_DeleteProgram | void | (uint program) |
| 23 | Uniform | gl_GetUniformLocation | int | (uint program, const char* name) |
| 24 | Uniform | gl_UniformMatrix4fv | void | (int loc, int count, u8 transpose, const float* value) |
| 25 | Uniform | gl_Uniform1i | void | (int loc, int v0) |
| 26 | Uniform | gl_Uniform1f | void | (int loc, float v0) |
| 27 | Uniform | gl_Uniform3f | void | (int loc, float v0, float v1, float v2) |
| 28 | Uniform | gl_Uniform4f | void | (int loc, float v0, float v1, float v2, float v3) |
| 29 | Texture | gl_ActiveTexture | void | (uint texture) |
| 30 | Texture | gl_GenerateMipmap | void | (uint target) |
| +1 | State | gl_LoadFunctions | void | (void) |
| +2 | State | gl_SetState | void | (void* state) |
| +3 | State | gl_GetState | void* | (void) |

Total: 33 exported functions (30 GL + 3 infrastructure).
