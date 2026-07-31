use crate::constants::DEPTH_FORMAT;
use crate::render::geometry::{StaticMesh, upload_mesh};
use crate::render::gpu::SnowGpu;
use crate::render::pipelines::{sampler_entry, texture_entry, uniform_entry};
use crate::render::uniforms::SnowUniforms;
use crate::shaders::{self, ShaderLibrary};
use crate::systems::spell::water::{LATTICE_COLS, RING, STRAND_COLS, STRAND_MAX};
use nightshade::prelude::wgpu;

/// The bent water: one static lattice and the program that places it.
pub struct WaterRender {
    lattice: StaticMesh,
    pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    textures: Option<wgpu::BindGroup>,
    pub visible: bool,
    live: u32,
}

pub fn new(
    device: &wgpu::Device,
    library: &mut ShaderLibrary,
    color_format: wgpu::TextureFormat,
) -> WaterRender {
    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_water_uniforms"),
        entries: &[uniform_entry(0)],
    });
    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_water_textures"),
        entries: &[
            texture_entry(0, false),
            texture_entry(1, true),
            sampler_entry(2, true),
            texture_entry(3, true),
            texture_entry(4, true),
            texture_entry(5, true),
            sampler_entry(6, true),
        ],
    });

    let module = shaders::compile(library, device, "water", shaders::WATER);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("snow_water"),
        bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
        immediate_size: 0,
    });

    let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snow_water"),
        size: std::mem::size_of::<SnowUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    WaterRender {
        lattice: build_lattice(device),
        pipeline: device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("snow_water"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertexMain"),
                buffers: &[LATTICE_LAYOUT],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fragmentMain"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            // A transparent body seen from both sides: looking through the
            // near wall at the far one is most of what makes it a volume.
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        }),
        uniform_bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("snow_water_uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        }),
        texture_layout,
        uniforms,
        textures: None,
        visible: false,
        live: 0,
    }
}

pub fn bind(water: &mut WaterRender, device: &wgpu::Device, gpu: &SnowGpu) {
    water.textures = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("snow_water_textures"),
        layout: &water.texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&gpu.water.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&gpu.sky_lut.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&gpu.sky_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&gpu.cascades[0].view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&gpu.cascades[1].view),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(&gpu.cascades[2].view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(&gpu.linear_clamp),
            },
        ],
    }));
}

pub fn write(
    water: &mut WaterRender,
    queue: &wgpu::Queue,
    gpu: &SnowGpu,
    uniforms: &SnowUniforms,
    texels: &[f32],
    live: u32,
) {
    queue.write_buffer(&water.uniforms, 0, bytemuck::bytes_of(uniforms));
    water.live = live;
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &gpu.water.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(texels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(STRAND_COLS as u32 * 16),
            rows_per_image: Some(STRAND_MAX as u32 * 3),
        },
        wgpu::Extent3d {
            width: STRAND_COLS as u32,
            height: STRAND_MAX as u32 * 3,
            depth_or_array_layers: 1,
        },
    );
}

pub fn draw(water: &WaterRender, pass: &mut wgpu::RenderPass<'_>) {
    let Some(textures) = &water.textures else {
        return;
    };
    if !water.visible {
        return;
    }
    pass.set_pipeline(&water.pipeline);
    pass.set_bind_group(0, &water.uniform_bind_group, &[]);
    pass.set_bind_group(1, textures, &[]);
    pass.set_vertex_buffer(0, water.lattice.vertices.slice(..));
    pass.set_index_buffer(water.lattice.indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..water.lattice.index_count, 0, 0..1);
}

pub fn draw_calls(water: &WaterRender) -> u32 {
    u32::from(water.visible)
}

/// Only the live strands count as geometry: a dead one collapses to a point
/// in the vertex stage and produces no fragments.
pub fn triangles(water: &WaterRender) -> u32 {
    if !water.visible {
        return 0;
    }
    (water.lattice.index_count / 3) * water.live / STRAND_MAX as u32
}

/// The static lattice of column, ring and strand, with no geometry at all.
///
/// Strands are separate index ranges in one buffer rather than separate meshes,
/// so the whole system is a single draw however many spells are up.
fn build_lattice(device: &wgpu::Device) -> StaticMesh {
    let per_strand = LATTICE_COLS * RING;
    let mut data = Vec::with_capacity(per_strand * STRAND_MAX * 3);
    let mut indices = Vec::with_capacity((LATTICE_COLS - 1) * (RING - 1) * 6 * STRAND_MAX);

    for strand in 0..STRAND_MAX {
        let base = (strand * per_strand) as u32;
        for column in 0..LATTICE_COLS {
            for ring in 0..RING {
                data.extend_from_slice(&[column as f32, ring as f32, strand as f32]);
            }
        }
        for column in 0..LATTICE_COLS as u32 - 1 {
            for ring in 0..RING as u32 - 1 {
                let a = base + column * RING as u32 + ring;
                let b = a + RING as u32;
                indices.extend_from_slice(&[a, b, b + 1, a, b + 1, a + 1]);
            }
        }
    }

    upload_mesh(device, "snow_water", &data, &indices)
}

const LATTICE_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: 12,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    }],
};
