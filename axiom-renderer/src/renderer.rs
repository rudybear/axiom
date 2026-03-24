// axiom-renderer/src/renderer.rs
//
// Core wgpu renderer. Manages a winit window, wgpu device/queue/surface, a
// simple render pipeline for colored geometry, and per-frame draw commands.

use std::sync::Arc;
use std::time::Instant;
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

// ---------------------------------------------------------------------------
// Vertex layout shared by points and triangles
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x4,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

// ---------------------------------------------------------------------------
// DrawCommand — accumulated per frame, flushed in end_frame
// ---------------------------------------------------------------------------

enum DrawCommand {
    Clear { r: f64, g: f64, b: f64 },
    Points(Vec<Vertex>),
    Triangles(Vec<Vertex>),
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub struct Renderer {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,

    // Frame state
    draw_commands: Vec<DrawCommand>,
    should_close: bool,
    frame_count: u32,
    start_time: Instant,

    // Dimensions
    width: u32,
    height: u32,
}

// Inline WGSL shader source
const SHADER_SRC: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Convert pixel coordinates to NDC:
    //   position comes in as pixel coords (0..width, 0..height)
    //   We convert to clip space (-1..1, -1..1) with Y flipped
    out.clip_position = vec4<f32>(in.position.x, in.position.y, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

impl Renderer {
    /// Create a new renderer with a window of the given size.
    ///
    /// This blocks on GPU initialization via pollster.
    pub fn new(width: u32, height: u32, title: &str) -> Result<Self, String> {
        // Create event loop and window
        let event_loop = EventLoop::new().map_err(|e| format!("EventLoop: {e}"))?;

        // We need to use a temporary ApplicationHandler to create the window
        // because winit 0.30 requires an ActiveEventLoop.
        // Instead, use EventLoop::create_proxy pattern or pump_events.
        // Actually, with winit 0.30 we can use EventLoopExtPumpEvents.

        // On Windows we can use the platform extension to pump events.
        // But first we need a window. Let's use the builder approach.
        // pump_events is used below for window creation and in poll_events

        // Create window via a bootstrap ApplicationHandler
        struct BootstrapApp {
            width: u32,
            height: u32,
            title: String,
            window: Option<Arc<Window>>,
        }

        impl ApplicationHandler for BootstrapApp {
            fn resumed(&mut self, event_loop: &ActiveEventLoop) {
                if self.window.is_none() {
                    let attrs = Window::default_attributes()
                        .with_title(&self.title)
                        .with_inner_size(PhysicalSize::new(self.width, self.height))
                        .with_resizable(true);
                    match event_loop.create_window(attrs) {
                        Ok(w) => self.window = Some(Arc::new(w)),
                        Err(e) => eprintln!("[AXIOM Renderer] Window creation failed: {e}"),
                    }
                    event_loop.exit();
                }
            }

            fn window_event(
                &mut self,
                _event_loop: &ActiveEventLoop,
                _window_id: WindowId,
                _event: WindowEvent,
            ) {
            }
        }

        let mut app = BootstrapApp {
            width,
            height,
            title: title.to_string(),
            window: None,
        };

        // Use pump_events to run the loop just long enough to create the window
        use winit::platform::pump_events::EventLoopExtPumpEvents;
        let mut event_loop = event_loop;
        // Pump until the window is created
        for _ in 0..100 {
            let _status = event_loop.pump_app_events(Some(std::time::Duration::from_millis(10)), &mut app);
            if app.window.is_some() {
                break;
            }
        }

        let window = app.window.ok_or("Failed to create window")?;

        // Initialize wgpu
        let (device, queue, surface, surface_config, pipeline) =
            pollster::block_on(Self::init_wgpu(window.clone(), width, height))?;

        println!(
            "[AXIOM Renderer] Created {width}x{height} window: \"{title}\" (wgpu/Vulkan)"
        );

        // Store event_loop in a thread-local so poll_events can use it
        EVENT_LOOP.with(|cell| {
            *cell.borrow_mut() = Some(event_loop);
        });

        Ok(Self {
            window,
            device,
            queue,
            surface,
            surface_config,
            pipeline,
            draw_commands: Vec::new(),
            should_close: false,
            frame_count: 0,
            start_time: Instant::now(),
            width,
            height,
        })
    }

    async fn init_wgpu(
        window: Arc<Window>,
        width: u32,
        height: u32,
    ) -> Result<
        (
            wgpu::Device,
            wgpu::Queue,
            wgpu::Surface<'static>,
            wgpu::SurfaceConfiguration,
            wgpu::RenderPipeline,
        ),
        String,
    > {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::DX12,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("Surface: {e}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or("No suitable GPU adapter found")?;

        println!(
            "[AXIOM Renderer] GPU: {} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("AXIOM Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None, // trace path
            )
            .await
            .map_err(|e| format!("Device: {e}"))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("AXIOM Shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        // Create pipeline layout (no bind groups needed for basic rendering)
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("AXIOM Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        // Create render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("AXIOM Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Ok((device, queue, surface, surface_config, pipeline))
    }

    /// Poll window events. Returns true if the window should close.
    pub fn poll_events(&mut self) -> bool {
        EVENT_LOOP.with(|cell| {
            let mut el = cell.borrow_mut();
            if let Some(event_loop) = el.as_mut() {
                use winit::platform::pump_events::EventLoopExtPumpEvents;

                struct PollApp<'a> {
                    renderer: &'a mut bool, // should_close flag
                    width: &'a mut u32,
                    height: &'a mut u32,
                    surface: &'a wgpu::Surface<'static>,
                    device: &'a wgpu::Device,
                    surface_config: &'a mut wgpu::SurfaceConfiguration,
                }

                impl ApplicationHandler for PollApp<'_> {
                    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

                    fn window_event(
                        &mut self,
                        event_loop: &ActiveEventLoop,
                        _window_id: WindowId,
                        event: WindowEvent,
                    ) {
                        match event {
                            WindowEvent::CloseRequested => {
                                *self.renderer = true;
                                event_loop.exit();
                            }
                            WindowEvent::Resized(new_size) => {
                                if new_size.width > 0 && new_size.height > 0 {
                                    *self.width = new_size.width;
                                    *self.height = new_size.height;
                                    self.surface_config.width = new_size.width;
                                    self.surface_config.height = new_size.height;
                                    self.surface.configure(self.device, self.surface_config);
                                }
                            }
                            _ => {}
                        }
                    }
                }

                let mut poll_app = PollApp {
                    renderer: &mut self.should_close,
                    width: &mut self.width,
                    height: &mut self.height,
                    surface: &self.surface,
                    device: &self.device,
                    surface_config: &mut self.surface_config,
                };

                let _ = event_loop.pump_app_events(
                    Some(std::time::Duration::ZERO),
                    &mut poll_app,
                );
            }
        });
        self.should_close
    }

    /// Begin a new frame. Returns false if window should close.
    pub fn begin_frame(&mut self) -> bool {
        self.poll_events();
        if self.should_close {
            return false;
        }
        self.draw_commands.clear();
        true
    }

    /// End the current frame: execute all draw commands and present.
    pub fn end_frame(&mut self) {
        let output = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.surface_config);
                match self.surface.get_current_texture() {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("[AXIOM Renderer] Surface error: {e}");
                        return;
                    }
                }
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                eprintln!("[AXIOM Renderer] Out of memory!");
                self.should_close = true;
                return;
            }
            Err(e) => {
                eprintln!("[AXIOM Renderer] Surface error: {e}");
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("AXIOM Frame Encoder"),
            });

        // Determine clear color from commands (use last clear, default black)
        let mut clear_color = wgpu::Color::BLACK;
        for cmd in &self.draw_commands {
            if let DrawCommand::Clear { r, g, b } = cmd {
                clear_color = wgpu::Color {
                    r: *r,
                    g: *g,
                    b: *b,
                    a: 1.0,
                };
            }
        }

        // Collect all geometry into vertex buffers
        let mut point_vertices: Vec<Vertex> = Vec::new();
        let mut tri_vertices: Vec<Vertex> = Vec::new();

        for cmd in &self.draw_commands {
            match cmd {
                DrawCommand::Points(verts) => point_vertices.extend_from_slice(verts),
                DrawCommand::Triangles(verts) => tri_vertices.extend_from_slice(verts),
                DrawCommand::Clear { .. } => {}
            }
        }

        // Create vertex buffers
        let point_buf = if !point_vertices.is_empty() {
            Some(self.device.create_buffer_init(
                &wgpu::util::BufferInitDescriptor {
                    label: Some("Point Vertex Buffer"),
                    contents: bytemuck::cast_slice(&point_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                },
            ))
        } else {
            None
        };

        let tri_buf = if !tri_vertices.is_empty() {
            Some(self.device.create_buffer_init(
                &wgpu::util::BufferInitDescriptor {
                    label: Some("Triangle Vertex Buffer"),
                    contents: bytemuck::cast_slice(&tri_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                },
            ))
        } else {
            None
        };

        // Render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("AXIOM Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline);

            // Draw triangles (from triangle commands)
            if let Some(buf) = &tri_buf {
                render_pass.set_vertex_buffer(0, buf.slice(..));
                render_pass.draw(0..tri_vertices.len() as u32, 0..1);
            }

            // Draw points (rendered as small quads — 2 triangles per point)
            if let Some(buf) = &point_buf {
                render_pass.set_vertex_buffer(0, buf.slice(..));
                render_pass.draw(0..point_vertices.len() as u32, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        self.frame_count += 1;
        if self.frame_count <= 3 || self.frame_count % 50 == 0 {
            println!("[AXIOM Renderer] Frame {} presented", self.frame_count);
        }
    }

    /// Queue a clear-screen command.
    pub fn clear(&mut self, color: u32) {
        let r = ((color >> 16) & 0xFF) as f64 / 255.0;
        let g = ((color >> 8) & 0xFF) as f64 / 255.0;
        let b = (color & 0xFF) as f64 / 255.0;
        self.draw_commands.push(DrawCommand::Clear { r, g, b });
    }

    /// Queue colored points as small quads (2 triangles each, 2x2 pixel).
    pub fn draw_points(
        &mut self,
        x_arr: &[f64],
        y_arr: &[f64],
        colors: &[u32],
        count: usize,
    ) {
        let w = self.width as f32;
        let h = self.height as f32;
        let mut verts = Vec::with_capacity(count * 6); // 2 tris * 3 verts per point

        // Point size in pixels
        let ps = 2.0_f32;
        // Half-pixel in NDC
        let hx = ps / w;
        let hy = ps / h;

        for i in 0..count {
            let px = x_arr[i] as f32;
            let py = y_arr[i] as f32;

            // Skip out-of-bounds points
            if px < -ps || px > w + ps || py < -ps || py > h + ps {
                continue;
            }

            // Convert pixel coords to NDC: x: [0,w] -> [-1,1], y: [0,h] -> [1,-1] (flip Y)
            let nx = (px / w) * 2.0 - 1.0;
            let ny = 1.0 - (py / h) * 2.0;

            let c = colors[i];
            let cr = ((c >> 16) & 0xFF) as f32 / 255.0;
            let cg = ((c >> 8) & 0xFF) as f32 / 255.0;
            let cb = (c & 0xFF) as f32 / 255.0;
            let color = [cr, cg, cb, 1.0];

            // Build a quad (2 triangles) centered on the point
            let x0 = nx - hx;
            let y0 = ny - hy;
            let x1 = nx + hx;
            let y1 = ny + hy;

            // Triangle 1: top-left, top-right, bottom-left
            verts.push(Vertex { position: [x0, y1], color });
            verts.push(Vertex { position: [x1, y1], color });
            verts.push(Vertex { position: [x0, y0], color });
            // Triangle 2: top-right, bottom-right, bottom-left
            verts.push(Vertex { position: [x1, y1], color });
            verts.push(Vertex { position: [x1, y0], color });
            verts.push(Vertex { position: [x0, y0], color });
        }

        if !verts.is_empty() {
            self.draw_commands.push(DrawCommand::Points(verts));
        }
    }

    /// Queue colored triangles.
    /// positions: [x0,y0, x1,y1, x2,y2, ...] in pixel coords
    /// colors_f:  [r0,g0,b0, r1,g1,b1, ...] in [0,1] floats
    pub fn draw_triangles(
        &mut self,
        positions: &[f32],
        colors_f: Option<&[f32]>,
        vertex_count: usize,
    ) {
        let w = self.width as f32;
        let h = self.height as f32;
        let mut verts = Vec::with_capacity(vertex_count);

        for i in 0..vertex_count {
            let px = positions[i * 2];
            let py = positions[i * 2 + 1];

            // Convert pixel coords to NDC
            let nx = (px / w) * 2.0 - 1.0;
            let ny = 1.0 - (py / h) * 2.0;

            let color = if let Some(cf) = colors_f {
                [cf[i * 3], cf[i * 3 + 1], cf[i * 3 + 2], 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            };

            verts.push(Vertex {
                position: [nx, ny],
                color,
            });
        }

        if !verts.is_empty() {
            self.draw_commands.push(DrawCommand::Triangles(verts));
        }
    }

    pub fn should_close(&self) -> bool {
        self.should_close
    }

    #[allow(dead_code)]
    pub fn frame_count(&self) -> u32 {
        self.frame_count
    }

    pub fn get_time(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }

    pub fn destroy(&mut self) {
        println!(
            "[AXIOM Renderer] Destroyed after {} frames",
            self.frame_count
        );
        // Drop will handle cleanup via wgpu's Drop impls
    }
}

// ---------------------------------------------------------------------------
// Thread-local storage for the winit EventLoop
// ---------------------------------------------------------------------------
// winit's EventLoop is !Send, so we store it in a thread-local.
// The renderer must be created and used from the same thread (main thread).

thread_local! {
    static EVENT_LOOP: std::cell::RefCell<Option<EventLoop<()>>> = std::cell::RefCell::new(None);
}
