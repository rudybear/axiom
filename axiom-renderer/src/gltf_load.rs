// axiom-renderer/src/gltf_load.rs
//
// glTF/GLB scene loading. Parses meshes, extracts vertex/index data,
// decodes textures, and uploads everything to the GPU via wgpu.

use wgpu::util::DeviceExt;

/// PBR vertex: position + normal + uv + tangent = 48 bytes.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PbrVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4],
}

impl PbrVertex {
    pub const ATTRIBS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x3,   // position
        1 => Float32x3,   // normal
        2 => Float32x2,   // uv
        3 => Float32x4,   // tangent
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PbrVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Material parameters extracted from glTF, uploaded as a uniform.
/// Layout must match the WGSL `MaterialParams` struct exactly.
/// Total size: 48 bytes (3 x vec4, aligned to 16).
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialParams {
    pub base_color: [f32; 4],      // offset 0, 16 bytes
    pub metallic: f32,              // offset 16
    pub roughness: f32,             // offset 20
    pub emissive_x: f32,            // offset 24
    pub emissive_y: f32,            // offset 28
    pub emissive_z: f32,            // offset 32
    pub _pad0: f32,                 // offset 36
    pub _pad1: f32,                 // offset 40
    pub _pad2: f32,                 // offset 44
    // Total: 48 bytes
}

/// A draw range: contiguous indices sharing a material.
pub struct DrawRange {
    pub index_offset: u32,
    pub index_count: u32,
    pub material_index: usize,
}

/// Per-material GPU resources.
pub struct GpuMaterial {
    pub params: MaterialParams,
    pub params_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

/// A fully loaded scene on the GPU.
pub struct GpuScene {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub draw_ranges: Vec<DrawRange>,
    pub materials: Vec<GpuMaterial>,
}

/// Load a glTF/GLB file and upload its meshes and materials to the GPU.
pub fn load_gltf(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    path: &str,
    material_bind_group_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    fallback_white: &wgpu::TextureView,
    fallback_normal: &wgpu::TextureView,
    fallback_mr: &wgpu::TextureView,
) -> Result<GpuScene, String> {
    let (document, buffers, images) = gltf::import(path).map_err(|e| format!("glTF load error: {e}"))?;

    println!("[AXIOM glTF] Loading: {path}");
    println!(
        "[AXIOM glTF]   meshes={}, materials={}, images={}",
        document.meshes().count(),
        document.materials().count(),
        images.len(),
    );

    // --- Upload textures from glTF images ---
    let mut texture_views: Vec<wgpu::TextureView> = Vec::new();
    // Keep textures alive
    let mut _textures: Vec<wgpu::Texture> = Vec::new();
    for (i, img_data) in images.iter().enumerate() {
        let rgba_data = match img_data.format {
            gltf::image::Format::R8G8B8A8 => img_data.pixels.clone(),
            gltf::image::Format::R8G8B8 => {
                // Convert RGB to RGBA
                let mut rgba = Vec::with_capacity(img_data.pixels.len() / 3 * 4);
                for chunk in img_data.pixels.chunks(3) {
                    rgba.push(chunk[0]);
                    rgba.push(chunk[1]);
                    rgba.push(chunk[2]);
                    rgba.push(255);
                }
                rgba
            }
            _ => {
                // For other formats, create a white pixel
                println!("[AXIOM glTF]   Warning: unsupported image format {:?} for image {i}, using fallback", img_data.format);
                vec![255, 255, 255, 255]
            }
        };

        let width = if rgba_data.len() == 4 { 1 } else { img_data.width };
        let height = if rgba_data.len() == 4 { 1 } else { img_data.height };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("glTF image {i}")),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            texture.as_image_copy(),
            &rgba_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        texture_views.push(view);
        _textures.push(texture);
    }

    // --- Extract materials ---
    let mut gpu_materials: Vec<GpuMaterial> = Vec::new();

    for mat in document.materials() {
        let pbr = mat.pbr_metallic_roughness();

        let emissive = mat.emissive_factor();
        let params = MaterialParams {
            base_color: pbr.base_color_factor(),
            metallic: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
            emissive_x: emissive[0],
            emissive_y: emissive[1],
            emissive_z: emissive[2],
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        };

        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("material params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Get texture views (or fallback)
        let base_color_view = if let Some(info) = pbr.base_color_texture() {
            let idx = info.texture().source().index();
            if idx < texture_views.len() {
                &texture_views[idx]
            } else {
                fallback_white
            }
        } else {
            fallback_white
        };

        let normal_view_actual = if let Some(normal_tex) = mat.normal_texture() {
            let idx = normal_tex.texture().source().index();
            if idx < texture_views.len() {
                &texture_views[idx]
            } else {
                fallback_normal
            }
        } else {
            fallback_normal
        };

        let mr_view = if let Some(mr_tex) = pbr.metallic_roughness_texture() {
            let idx = mr_tex.texture().source().index();
            if idx < texture_views.len() {
                &texture_views[idx]
            } else {
                fallback_mr
            }
        } else {
            fallback_mr
        };

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material bind group"),
            layout: material_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(base_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(normal_view_actual),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(mr_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        gpu_materials.push(GpuMaterial {
            params,
            params_buffer,
            bind_group,
        });
    }

    // If no materials in the file, create a default one
    if gpu_materials.is_empty() {
        let params = MaterialParams {
            base_color: [0.8, 0.8, 0.8, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            emissive_x: 0.0,
            emissive_y: 0.0,
            emissive_z: 0.0,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        };

        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("default material params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("default material bind group"),
            layout: material_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(fallback_white),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(fallback_normal),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(fallback_mr),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        gpu_materials.push(GpuMaterial {
            params,
            params_buffer,
            bind_group,
        });
    }

    // --- Extract meshes ---
    let mut all_vertices: Vec<PbrVertex> = Vec::new();
    let mut all_indices: Vec<u32> = Vec::new();
    let mut draw_ranges: Vec<DrawRange> = Vec::new();

    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            // Positions (required)
            let positions: Vec<[f32; 3]> = match reader.read_positions() {
                Some(iter) => iter.collect(),
                None => {
                    eprintln!("[AXIOM glTF] Warning: mesh primitive without positions, skipping");
                    continue;
                }
            };

            let vertex_count = positions.len();

            // Normals (optional, default to up)
            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|iter| iter.collect())
                .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; vertex_count]);

            // UVs (optional, default to 0,0)
            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|iter| iter.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; vertex_count]);

            // Tangents (optional, default to +X)
            let tangents: Vec<[f32; 4]> = reader
                .read_tangents()
                .map(|iter| iter.collect())
                .unwrap_or_else(|| vec![[1.0, 0.0, 0.0, 1.0]; vertex_count]);

            // Build vertices
            let base_vertex = all_vertices.len() as u32;
            for i in 0..vertex_count {
                all_vertices.push(PbrVertex {
                    position: positions[i],
                    normal: normals[i],
                    uv: uvs[i],
                    tangent: tangents[i],
                });
            }

            // Indices
            let index_offset = all_indices.len() as u32;
            if let Some(indices) = reader.read_indices() {
                let indices: Vec<u32> = indices.into_u32().collect();
                let index_count = indices.len() as u32;
                for idx in &indices {
                    all_indices.push(base_vertex + idx);
                }
                // Material index (default to 0 if none)
                let material_index = primitive.material().index().unwrap_or(0);
                draw_ranges.push(DrawRange {
                    index_offset,
                    index_count,
                    material_index,
                });
            } else {
                // No indices — generate sequential indices
                let index_count = vertex_count as u32;
                for i in 0..vertex_count as u32 {
                    all_indices.push(base_vertex + i);
                }
                let material_index = primitive.material().index().unwrap_or(0);
                draw_ranges.push(DrawRange {
                    index_offset,
                    index_count,
                    material_index,
                });
            }
        }
    }

    println!(
        "[AXIOM glTF]   vertices={}, indices={}, draw_ranges={}",
        all_vertices.len(),
        all_indices.len(),
        draw_ranges.len(),
    );

    if all_vertices.is_empty() {
        return Err("glTF file contains no mesh data".to_string());
    }

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("glTF vertex buffer"),
        contents: bytemuck::cast_slice(&all_vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("glTF index buffer"),
        contents: bytemuck::cast_slice(&all_indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let index_count = all_indices.len() as u32;

    // Keep texture data alive by storing in materials (they hold bind groups referencing views)
    // The _textures Vec is dropped here but the wgpu::TextureView references remain valid
    // because wgpu internally ref-counts them. This is safe.

    Ok(GpuScene {
        vertex_buffer,
        index_buffer,
        index_count,
        draw_ranges,
        materials: gpu_materials,
    })
}
