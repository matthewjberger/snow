use crate::constants::DEPTH_FORMAT;
use crate::render::geometry::{StaticMesh, upload_mesh};
use crate::render::gpu::SnowGpu;
use crate::render::pipelines::{sampler_entry, texture_entry, uniform_entry};
use crate::render::uniforms::SnowUniforms;
use crate::shaders::{self, ShaderLibrary};
use crate::systems::spray::SPRAY_CAPACITY;
use nightshade::prelude::wgpu;

/// The billboard field: one static quad grid and the program that places it.
pub struct SprayRender {
    quads: StaticMesh,
    pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    textures: Option<wgpu::BindGroup>,
    live: u32,
}

pub fn new(
    device: &wgpu::Device,
    library: &mut ShaderLibrary,
    color_format: wgpu::TextureFormat,
) -> SprayRender {
    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_spray_uniforms"),
        entries: &[uniform_entry(0)],
    });
    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("snow_spray_textures"),
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

    let module = shaders::compile(library, device, "spray", shaders::SPRAY);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("snow_spray"),
        bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
        immediate_size: 0,
    });

    let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snow_spray"),
        size: std::mem::size_of::<SnowUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    SprayRender {
        quads: build_quads(device),
        pipeline: device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("snow_spray"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertexMain"),
                buffers: &[QUAD_LAYOUT],
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
            label: Some("snow_spray_uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        }),
        texture_layout,
        uniforms,
        textures: None,
        live: 0,
    }
}

pub fn bind(spray: &mut SprayRender, device: &wgpu::Device, gpu: &SnowGpu) {
    spray.textures = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("snow_spray_textures"),
        layout: &spray.texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&gpu.spray.view),
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

/// Uploads this frame's uniform block and particle state.
pub fn write(
    spray: &mut SprayRender,
    queue: &wgpu::Queue,
    gpu: &SnowGpu,
    uniforms: &SnowUniforms,
    texels: &[f32],
    live: u32,
) {
    queue.write_buffer(&spray.uniforms, 0, bytemuck::bytes_of(uniforms));
    spray.live = live;
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &gpu.spray.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(texels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SPRAY_CAPACITY as u32 * 16),
            rows_per_image: Some(2),
        },
        wgpu::Extent3d {
            width: SPRAY_CAPACITY as u32,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
}

pub fn draw(spray: &SprayRender, pass: &mut wgpu::RenderPass<'_>) {
    let Some(textures) = &spray.textures else {
        return;
    };
    if spray.live == 0 {
        return;
    }
    pass.set_pipeline(&spray.pipeline);
    pass.set_bind_group(0, &spray.uniform_bind_group, &[]);
    pass.set_bind_group(1, textures, &[]);
    pass.set_vertex_buffer(0, spray.quads.vertices.slice(..));
    pass.set_index_buffer(spray.quads.indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..spray.quads.index_count, 0, 0..1);
}

pub fn draw_calls(spray: &SprayRender) -> u32 {
    u32::from(spray.live > 0)
}

/// Two triangles per live grain.
pub fn triangles(spray: &SprayRender) -> u32 {
    spray.live * 2
}

/// A static grid of quads.
fn build_quads(device: &wgpu::Device) -> StaticMesh {
    const CORNERS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
    let mut data = Vec::with_capacity(SPRAY_CAPACITY * 4 * 3);
    let mut indices = Vec::with_capacity(SPRAY_CAPACITY * 6);

    for particle in 0..SPRAY_CAPACITY as u32 {
        for corner in CORNERS {
            data.extend_from_slice(&[particle as f32, corner[0], corner[1]]);
        }
        let base = particle * 4;
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    upload_mesh(device, "snow_spray", &data, &indices)
}

const QUAD_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: 12,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    }],
};
