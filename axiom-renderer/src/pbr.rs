// axiom-renderer/src/pbr.rs
//
// PBR render pipeline with embedded WGSL shader. Supports camera uniform,
// per-material textures (base color, normal, metallic-roughness), and a
// simple directional light with Cook-Torrance BRDF.

use wgpu::util::DeviceExt;

use crate::camera::CameraUniform;
use crate::gltf_load::PbrVertex;

/// Embedded PBR WGSL shader source.
const PBR_SHADER_SRC: &str = r#"
// ---- Camera uniform (bind group 0, binding 0) ----
struct CameraUniform {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    eye_pos: vec3<f32>,
};
@group(0) @binding(0) var<uniform> camera: CameraUniform;

// ---- Material uniform (bind group 1, binding 0) ----
struct MaterialParams {
    base_color: vec4<f32>,
    metallic: f32,
    roughness: f32,
    emissive_x: f32,
    emissive_y: f32,
    emissive_z: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};
@group(1) @binding(0) var<uniform> material: MaterialParams;
@group(1) @binding(1) var base_color_tex: texture_2d<f32>;
@group(1) @binding(2) var base_color_samp: sampler;
@group(1) @binding(3) var normal_tex: texture_2d<f32>;
@group(1) @binding(4) var normal_samp: sampler;
@group(1) @binding(5) var metallic_roughness_tex: texture_2d<f32>;
@group(1) @binding(6) var metallic_roughness_samp: sampler;

// ---- Vertex I/O ----
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec3<f32>,
    @location(4) bitangent: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = in.position; // No model transform in Phase 1
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.world_pos = world_pos;
    out.world_normal = normalize(in.normal);
    out.uv = in.uv;
    out.tangent = normalize(in.tangent.xyz);
    out.bitangent = cross(out.world_normal, out.tangent) * in.tangent.w;
    return out;
}

// ---- PBR Fragment Shader ----
const PI: f32 = 3.14159265359;

// Directional light
const LIGHT_DIR: vec3<f32> = vec3<f32>(0.5, 1.0, 0.8);
const LIGHT_COLOR: vec3<f32> = vec3<f32>(1.0, 0.98, 0.95);
const LIGHT_INTENSITY: f32 = 3.0;
const AMBIENT: vec3<f32> = vec3<f32>(0.03, 0.03, 0.03);

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * denom * denom + 0.0001);
}

fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_v / (n_dot_v * (1.0 - k) + k + 0.0001);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(n_dot_v, roughness) * geometry_schlick_ggx(n_dot_l, roughness);
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (1.0 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample textures
    let base_color_sample = textureSample(base_color_tex, base_color_samp, in.uv);
    let albedo = base_color_sample.rgb * material.base_color.rgb;
    let alpha = base_color_sample.a * material.base_color.a;

    let mr_sample = textureSample(metallic_roughness_tex, metallic_roughness_samp, in.uv);
    let metallic = mr_sample.b * material.metallic;
    let roughness = clamp(mr_sample.g * material.roughness, 0.04, 1.0);

    // Normal mapping
    let normal_sample = textureSample(normal_tex, normal_samp, in.uv).rgb;
    let tangent_normal = normal_sample * 2.0 - 1.0;
    let tbn = mat3x3<f32>(
        normalize(in.tangent),
        normalize(in.bitangent),
        normalize(in.world_normal),
    );
    let n = normalize(tbn * tangent_normal);

    let v = normalize(camera.eye_pos - in.world_pos);
    let l = normalize(LIGHT_DIR);
    let h = normalize(v + l);

    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 0.001);
    let n_dot_h = max(dot(n, h), 0.0);
    let h_dot_v = max(dot(h, v), 0.0);

    // PBR: Cook-Torrance BRDF
    let f0_dielectric = vec3<f32>(0.04, 0.04, 0.04);
    let f0 = mix(f0_dielectric, albedo, metallic);

    let d = distribution_ggx(n_dot_h, roughness);
    let g = geometry_smith(n_dot_v, n_dot_l, roughness);
    let f = fresnel_schlick(h_dot_v, f0);

    let specular = (d * g * f) / (4.0 * n_dot_v * n_dot_l + 0.0001);

    let ks = f;
    let kd = (1.0 - ks) * (1.0 - metallic);
    let diffuse = kd * albedo / PI;

    let radiance = LIGHT_COLOR * LIGHT_INTENSITY;
    var color = (diffuse + specular) * radiance * n_dot_l;

    // Ambient
    color += AMBIENT * albedo;

    // Emissive
    let emissive = vec3<f32>(material.emissive_x, material.emissive_y, material.emissive_z);
    color += emissive;

    return vec4<f32>(color, alpha);
}
"#;

/// The PBR render pipeline and associated GPU resources.
pub struct PbrPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    pub material_bind_group_layout: wgpu::BindGroupLayout,
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub default_sampler: wgpu::Sampler,
    // Fallback textures for materials missing images
    pub fallback_white_tex: wgpu::Texture,
    pub fallback_white_view: wgpu::TextureView,
    pub fallback_normal_tex: wgpu::Texture,
    pub fallback_normal_view: wgpu::TextureView,
    pub fallback_mr_tex: wgpu::Texture,
    pub fallback_mr_view: wgpu::TextureView,
}

impl PbrPipeline {
    /// Create the PBR pipeline, bind group layouts, camera buffer, and fallback textures.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        // --- Camera bind group layout (group 0) ---
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("PBR camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // --- Material bind group layout (group 1) ---
        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("PBR material bind group layout"),
                entries: &[
                    // binding 0: MaterialParams uniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 1: base_color texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 2: base_color sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // binding 3: normal texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 4: normal sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // binding 5: metallic-roughness texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // binding 6: metallic-roughness sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // --- Camera uniform buffer ---
        let camera_uniform = CameraUniform::from_state(
            &crate::camera::CameraState::default(),
            16.0 / 9.0,
        );
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("PBR camera uniform"),
            contents: bytemuck::bytes_of(&camera_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("PBR camera bind group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // --- Default sampler ---
        let default_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("PBR default sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // --- Fallback textures ---
        let (fallback_white_tex, fallback_white_view) =
            create_fallback_1x1(device, queue, [255, 255, 255, 255], "fallback_white", true);
        let (fallback_normal_tex, fallback_normal_view) =
            create_fallback_1x1(device, queue, [128, 128, 255, 255], "fallback_normal", false);
        let (fallback_mr_tex, fallback_mr_view) =
            create_fallback_1x1(device, queue, [0, 128, 0, 255], "fallback_mr", false);

        // --- Shader module ---
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("PBR WGSL Shader"),
            source: wgpu::ShaderSource::Wgsl(PBR_SHADER_SRC.into()),
        });

        // --- Pipeline layout ---
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("PBR Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &material_bind_group_layout],
            push_constant_ranges: &[],
        });

        // --- Render pipeline ---
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("PBR Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[PbrVertex::layout()],
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
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        println!("[AXIOM PBR] Pipeline created");

        Self {
            pipeline,
            camera_bind_group_layout,
            material_bind_group_layout,
            camera_buffer,
            camera_bind_group,
            default_sampler,
            fallback_white_tex,
            fallback_white_view,
            fallback_normal_tex,
            fallback_normal_view,
            fallback_mr_tex,
            fallback_mr_view,
        }
    }

    /// Update the camera uniform buffer.
    pub fn update_camera(&self, queue: &wgpu::Queue, uniform: &CameraUniform) {
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(uniform));
    }
}

/// Create a 1x1 fallback texture.
fn create_fallback_1x1(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    color: [u8; 4],
    label: &str,
    is_srgb: bool,
) -> (wgpu::Texture, wgpu::TextureView) {
    let format = if is_srgb {
        wgpu::TextureFormat::Rgba8UnormSrgb
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        texture.as_image_copy(),
        &color,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Create the depth texture and view for the given dimensions.
pub fn create_depth_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}
