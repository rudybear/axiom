// axiom-renderer/src/lux_shaders.rs
//
// Lux SPIR-V shader loading and wgpu pipeline creation.
//
// Lux shaders are compiled to SPIR-V bytecode (.spv files) by the Lux
// compiler. This module reads those files, validates them, and creates
// wgpu shader modules and render pipelines from them.
//
// The SPIR-V loading logic mirrors the approach in
// `lux/playground_rust/src/spv_loader.rs`, but targets wgpu instead of
// ash/Vulkan directly.

use std::path::Path;

use crate::renderer::Vertex;

// ---------------------------------------------------------------------------
// SPIR-V magic number (little-endian u32)
// ---------------------------------------------------------------------------

const SPIRV_MAGIC: u32 = 0x07230203;

// ---------------------------------------------------------------------------
// Shader stage (mirrors the C ABI convention: 0 = vertex, 1 = fragment)
// ---------------------------------------------------------------------------

/// Shader stage identifier passed from AXIOM programs via the C ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderStage {
    Vertex = 0,
    Fragment = 1,
}

impl ShaderStage {
    pub fn from_int(v: i32) -> Result<Self, String> {
        match v {
            0 => Ok(ShaderStage::Vertex),
            1 => Ok(ShaderStage::Fragment),
            _ => Err(format!("unknown shader stage: {v} (expected 0=vertex, 1=fragment)")),
        }
    }
}

// ---------------------------------------------------------------------------
// A loaded SPIR-V shader, ready to be turned into a wgpu module
// ---------------------------------------------------------------------------

/// Holds validated SPIR-V words and metadata about a loaded shader.
pub struct LuxShader {
    pub spirv: Vec<u32>,
    pub stage: ShaderStage,
    pub label: String,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Read a SPIR-V binary file and return its contents as a `Vec<u32>`.
///
/// Validates:
/// - File is at least 4 bytes
/// - File size is a multiple of 4
/// - SPIR-V magic number is correct
pub fn load_spirv(path: &Path) -> Result<Vec<u32>, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("failed to read {:?}: {}", path, e))?;

    if bytes.len() < 4 {
        return Err(format!("{:?}: file too small to be valid SPIR-V", path));
    }

    if bytes.len() % 4 != 0 {
        return Err(format!(
            "{:?}: file size {} is not a multiple of 4",
            path,
            bytes.len()
        ));
    }

    // Reinterpret as u32 words (little-endian)
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    if words[0] != SPIRV_MAGIC {
        return Err(format!(
            "{:?}: bad SPIR-V magic 0x{:08X} (expected 0x{:08X})",
            path, words[0], SPIRV_MAGIC
        ));
    }

    Ok(words)
}

/// Load a SPIR-V file and wrap it in a `LuxShader`.
pub fn load_shader(path: &str, stage: ShaderStage) -> Result<LuxShader, String> {
    let p = Path::new(path);
    let spirv = load_spirv(p)?;

    // Derive a label from the filename
    let label = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    println!(
        "[Lux Shader] Loaded {:?} ({} words, stage={:?})",
        label,
        spirv.len(),
        stage,
    );

    Ok(LuxShader {
        spirv,
        stage,
        label,
    })
}

/// Auto-detect shader stage from filename extension.
///
/// Recognized patterns (case-insensitive):
/// - `*.vert.spv` -> Vertex
/// - `*.frag.spv` -> Fragment
pub fn detect_stage(filename: &str) -> Result<ShaderStage, String> {
    let lower = filename.to_lowercase();
    if lower.ends_with(".vert.spv") {
        Ok(ShaderStage::Vertex)
    } else if lower.ends_with(".frag.spv") {
        Ok(ShaderStage::Fragment)
    } else {
        Err(format!(
            "cannot detect shader stage from filename: {filename}"
        ))
    }
}

// ---------------------------------------------------------------------------
// wgpu shader module creation
// ---------------------------------------------------------------------------

/// Create a `wgpu::ShaderModule` from SPIR-V words.
///
/// # Safety
/// Uses `create_shader_module_spirv` which is unsafe because wgpu cannot
/// fully validate arbitrary SPIR-V. The caller must ensure the SPIR-V is
/// well-formed (e.g., produced by the Lux compiler or `glslc`).
pub unsafe fn create_shader_module(
    device: &wgpu::Device,
    spirv: &[u32],
    label: &str,
) -> wgpu::ShaderModule {
    device.create_shader_module_spirv(&wgpu::ShaderModuleDescriptorSpirV {
        label: Some(label),
        source: std::borrow::Cow::Borrowed(spirv),
    })
}

// ---------------------------------------------------------------------------
// Pipeline creation
// ---------------------------------------------------------------------------

/// A Lux render pipeline wrapping a `wgpu::RenderPipeline`.
pub struct LuxPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub label: String,
}

/// Create a wgpu render pipeline from a vertex and fragment SPIR-V shader.
///
/// The pipeline uses the same vertex layout as the built-in AXIOM renderer
/// (`Vertex` with position `vec2<f32>` + color `vec4<f32>`) so that existing
/// draw commands (draw_triangles, draw_points) work unchanged.
///
/// # Safety
/// Delegates to `create_shader_module` which is unsafe.
pub unsafe fn create_render_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    vert_shader: &LuxShader,
    frag_shader: &LuxShader,
) -> Result<LuxPipeline, String> {
    if vert_shader.stage != ShaderStage::Vertex {
        return Err(format!(
            "expected vertex shader, got {:?}",
            vert_shader.stage
        ));
    }
    if frag_shader.stage != ShaderStage::Fragment {
        return Err(format!(
            "expected fragment shader, got {:?}",
            frag_shader.stage
        ));
    }

    let vert_module = create_shader_module(
        device,
        &vert_shader.spirv,
        &format!("lux_vert_{}", vert_shader.label),
    );
    let frag_module = create_shader_module(
        device,
        &frag_shader.spirv,
        &format!("lux_frag_{}", frag_shader.label),
    );

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("lux_pipeline_layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("lux_render_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vert_module,
            entry_point: Some("main"),
            buffers: &[Vertex::layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &frag_module,
            entry_point: Some("main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
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

    let label = format!(
        "lux_pipeline({} + {})",
        vert_shader.label, frag_shader.label
    );
    println!("[Lux Shader] Created pipeline: {label}");

    Ok(LuxPipeline { pipeline, label })
}
